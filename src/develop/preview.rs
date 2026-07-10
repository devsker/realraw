//! Background progressive preview loader for Develop mode.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use super::decode::{
    decode_embedded_preview, decode_raw_preview, PreviewImage, PreviewSource,
};
use crate::catalog::{preview_cache, thumbnail_cache};

/// Crossfade duration from placeholder → demosaic (seconds).
const FADE_SECS: f32 = 0.28;

/// Result delivered from a background decode job.
struct PreviewResult {
    photo_id: i64,
    generation: u64,
    kind: ResultKind,
}

enum ResultKind {
    /// Fast first paint (disk cache or embedded JPEG).
    Placeholder(PreviewImage),
    Final(Result<PreviewImage, String>),
}

/// Owns the develop preview texture and drives progressive RAW decode.
pub struct DevelopPreview {
    pub photo_id: Option<i64>,
    generation: u64,
    /// Latest decoded image (cache / embedded / demosaic).
    image: Option<PreviewImage>,
    /// GPU texture for `image`.
    texture: Option<egui::TextureHandle>,
    /// Previous texture kept during placeholder → demosaic crossfade.
    underlay: Option<egui::TextureHandle>,
    /// When the crossfade started (`None` = not fading).
    fade_start: Option<Instant>,
    /// Human-readable status while loading or on error.
    pub status: Option<String>,
    /// True while a background job for the current generation is in flight.
    loading: bool,
    /// True after a job for `photo_id` finished (success or hard failure).
    settled: bool,
    tx: Sender<PreviewResult>,
    rx: Receiver<PreviewResult>,
}

impl Default for DevelopPreview {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            photo_id: None,
            generation: 0,
            image: None,
            texture: None,
            underlay: None,
            fade_start: None,
            status: None,
            loading: false,
            settled: false,
            tx,
            rx,
        }
    }
}

impl DevelopPreview {
    /// `true` if this photo is already loaded, loading, or finished with an error.
    pub fn is_active_for(&self, photo_id: i64) -> bool {
        self.photo_id == Some(photo_id) && (self.loading || self.settled || self.image.is_some())
    }

    /// Start loading `photo_id`.
    ///
    /// Progressive:
    /// 1. Demosaic disk cache (`Previews/`, cacache) — authoritative hit
    /// 2. Library `Thumbnails/` / embedded JPEG placeholder
    /// 3. Full demosaic develop → write through to disk cache
    pub fn open(
        &mut self,
        photo_id: i64,
        path: PathBuf,
        orientation: Option<i64>,
        catalog_dir: PathBuf,
    ) {
        if self.is_active_for(photo_id) {
            return;
        }

        self.photo_id = Some(photo_id);
        self.generation = self.generation.wrapping_add(1);
        self.image = None;
        self.texture = None;
        self.underlay = None;
        self.fade_start = None;
        self.loading = true;
        self.settled = false;

        let job_gen = self.generation;
        let tx = self.tx.clone();

        if !super::decode::is_raw_path(&path) {
            self.loading = false;
            self.settled = true;
            self.status = Some("Develop preview supports RAW files only".into());
            return;
        }

        self.status = Some("Loading…".into());

        thread::Builder::new()
            .name("develop-preview".into())
            .spawn(move || {
                // Phase 1: demosaic disk cache (skip re-decode on revisit).
                if let Some(img) = preview_cache::load_preview(&catalog_dir, photo_id) {
                    let _ = tx.send(PreviewResult {
                        photo_id,
                        generation: job_gen,
                        kind: ResultKind::Final(Ok(img)),
                    });
                    return;
                }

                // Phase 2: library thumb / embedded JPEG for fast first paint.
                let placeholder = load_cached_thumb(&catalog_dir, photo_id).or_else(|| {
                    decode_embedded_preview(&path, orientation).ok()
                });
                if let Some(img) = placeholder {
                    let _ = tx.send(PreviewResult {
                        photo_id,
                        generation: job_gen,
                        kind: ResultKind::Placeholder(img),
                    });
                }

                // Phase 3: demosaic (or decoder RGB fallback), then cache.
                let final_result =
                    decode_raw_preview(&path, orientation).map_err(|e| e.to_string());
                if let Ok(ref img) = final_result {
                    if let Err(e) = preview_cache::save_preview(&catalog_dir, photo_id, img) {
                        eprintln!(
                            "preview cache save failed for photo {photo_id}: {e}"
                        );
                    }
                }
                let _ = tx.send(PreviewResult {
                    photo_id,
                    generation: job_gen,
                    kind: ResultKind::Final(final_result),
                });
            })
            .expect("spawn develop-preview");
    }

    /// Mark the current photo as failed without starting a decode job.
    pub fn fail(&mut self, photo_id: i64, message: String) {
        self.photo_id = Some(photo_id);
        self.generation = self.generation.wrapping_add(1);
        self.image = None;
        self.texture = None;
        self.underlay = None;
        self.fade_start = None;
        self.loading = false;
        self.settled = true;
        self.status = Some(message);
    }

    /// Clear the current photo and any pending display.
    pub fn clear(&mut self) {
        self.photo_id = None;
        self.generation = self.generation.wrapping_add(1);
        self.image = None;
        self.texture = None;
        self.underlay = None;
        self.fade_start = None;
        self.status = None;
        self.loading = false;
        self.settled = false;
    }

    /// Drain background results. Call once per frame.
    pub fn pump(&mut self, ctx: &egui::Context) {
        let mut need_repaint = false;
        while let Ok(r) = self.rx.try_recv() {
            if r.generation != self.generation || Some(r.photo_id) != self.photo_id {
                continue;
            }
            need_repaint = true;
            match r.kind {
                ResultKind::Placeholder(img) => {
                    // Only apply placeholder if we don't already have a final preview.
                    let replace = match &self.image {
                        None => true,
                        Some(cur) => !cur.source.is_final(),
                    };
                    if replace {
                        self.apply_image(ctx, img, false);
                        if self.loading {
                            self.status = Some("Loading…".into());
                        }
                    }
                }
                ResultKind::Final(Ok(img)) => {
                    // Crossfade from placeholder when we already have one.
                    let crossfade = self.texture.is_some()
                        && matches!(
                            self.image.as_ref().map(|i| i.source),
                            Some(PreviewSource::CachedThumb | PreviewSource::Embedded)
                        );
                    self.apply_image(ctx, img, crossfade);
                    self.loading = false;
                    self.settled = true;
                    self.status = None;
                }
                ResultKind::Final(Err(e)) => {
                    self.loading = false;
                    self.settled = true;
                    if self.image.is_none() {
                        self.status = Some(e);
                    } else {
                        // Keep placeholder; clear loading status.
                        self.status = None;
                    }
                }
            }
        }

        // Advance / finish crossfade.
        if let Some(start) = self.fade_start {
            if start.elapsed().as_secs_f32() >= FADE_SECS {
                self.underlay = None;
                self.fade_start = None;
            }
            need_repaint = true;
        }

        if need_repaint {
            ctx.request_repaint();
        }
        if self.loading {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.fade_start.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// Apply a decoded image. When `crossfade` is true, keep the previous
    /// texture as an underlay and fade the new one in.
    fn apply_image(&mut self, ctx: &egui::Context, img: PreviewImage, crossfade: bool) {
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [img.width as usize, img.height as usize],
            &img.rgba,
        );
        let name = format!(
            "develop-preview-{}-{:?}",
            self.photo_id.unwrap_or(0),
            img.source
        );
        let new_tex = ctx.load_texture(name, color, egui::TextureOptions::LINEAR);

        if crossfade {
            // Move current texture to underlay for the fade-out layer.
            self.underlay = self.texture.take();
            self.fade_start = Some(Instant::now());
        } else {
            self.underlay = None;
            self.fade_start = None;
        }

        self.texture = Some(new_tex);
        self.image = Some(img);
    }

    /// Texture to draw on top (current image), if any.
    pub fn texture(&self) -> Option<&egui::TextureHandle> {
        self.texture.as_ref()
    }

    /// Previous texture under the crossfade, if any.
    pub fn underlay_texture(&self) -> Option<&egui::TextureHandle> {
        self.underlay.as_ref()
    }

    /// Smoothstep fade progress for the top texture: `0` = fully transparent
    /// (underlay only), `1` = fully opaque. `None` when not transitioning.
    pub fn fade_progress(&self) -> Option<f32> {
        let start = self.fade_start?;
        let t = (start.elapsed().as_secs_f32() / FADE_SECS).clamp(0.0, 1.0);
        // Smoothstep for a softer ease-in/out.
        Some(t * t * (3.0 - 2.0 * t))
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn source(&self) -> Option<PreviewSource> {
        self.image.as_ref().map(|i| i.source)
    }
}

fn load_cached_thumb(catalog_dir: &std::path::Path, photo_id: i64) -> Option<PreviewImage> {
    let bytes = thumbnail_cache::load_thumbnail(catalog_dir, photo_id)?;
    if bytes.rgba.is_empty() || bytes.width == 0 || bytes.height == 0 {
        return None;
    }
    Some(PreviewImage {
        width: bytes.width,
        height: bytes.height,
        rgba: bytes.rgba,
        source: PreviewSource::CachedThumb,
    })
}
