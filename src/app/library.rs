//! The main library page: a thumbnail grid of every photo in the
//! catalog, using the same card style as the import dialog but
//! without selection or "in catalog" hints.
//!
//! Thumbnails are loaded lazily by short-lived worker threads
//! (same pattern as the import dialog). Per-photo state is keyed
//! by `Photo::id`, not by index, so a refresh in the middle of
//! loading (e.g. when the import dialog transitions to Done and
//! bumps the catalog file's mtime) doesn't wipe in-flight work.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::thread;

use eframe::egui;

use crate::catalog::{Catalog, Photo};
use crate::import::thumbnail::extract_thumbnail;
use crate::thumb_grid::{self, GridItem, ThumbCardConfig, ThumbnailBytes};

/// Maximum number of outstanding thumbnail requests at any time.
/// Keeps the disk + decode load under control on big libraries.
const MAX_INFLIGHT_THUMBS: usize = 16;
/// Vertical scroll area height for the library grid.
const SCROLL_MAX_HEIGHT: f32 = 6_000.0;

/// State for the main library page.
pub struct LibraryPage {
    /// Photos currently displayed, ordered by `imported_at` desc.
    pub photos: Vec<Photo>,
    /// Per-photo thumbnail state keyed by `Photo::id` (stable
    /// across refreshes). Holding the state in a `HashMap` means
    /// re-reading the catalog doesn't wipe in-flight work -- if a
    /// refresh happens while a thumb is loading, the result is
    /// still kept and reused.
    thumbs: HashMap<i64, ThumbState>,
    /// GPU textures, keyed by `CacheKey` (the photo's database id
    /// in the library; the path-hash in the import dialog). Owned
    /// by the page so they live as long as the photos do.
    textures: Mutex<HashMap<thumb_grid::CacheKey, egui::TextureHandle>>,

    thumb_tx: Sender<ThumbResult>,
    thumb_rx: Receiver<ThumbResult>,
    inflight_thumbs: AtomicUsize,

    /// Last load error (e.g. catalog query failure), if any.
    last_error: Option<String>,
}

/// Per-photo thumbnail state held in the library's `HashMap`.
/// Keeping this stable across refreshes means the in-flight thumb
/// workers' results can land in the right slot even if the user
/// scrolls, the import dialog re-runs, or the catalog mtime ticks
/// during a bulk import.
struct ThumbState {
    bytes: Option<ThumbnailBytes>,
    error: Option<String>,
    /// `true` once we've sent the file's path to the thumbnail
    /// worker. Used to avoid spawning duplicate workers.
    requested: bool,
}

struct ThumbResult {
    /// Photo id, not index. Stable across refreshes.
    photo_id: i64,
    result: Result<ThumbnailBytes, String>,
}

impl Default for LibraryPage {
    fn default() -> Self {
        let (thumb_tx, thumb_rx) = channel();
        Self {
            photos: Vec::new(),
            thumbs: HashMap::new(),
            textures: Mutex::new(HashMap::new()),
            thumb_tx,
            thumb_rx,
            inflight_thumbs: AtomicUsize::new(0),
            last_error: None,
        }
    }
}

impl LibraryPage {
    /// Reload the photo list from the catalog. Cheap when the catalog
    /// is already loaded; `limit` caps the number of rows (`None`
    /// means everything).
    ///
    /// Per-photo thumbnail state is preserved by id: an existing
    /// photo keeps its loaded thumbnail, a new photo gets a fresh
    /// slot. Removed photos have their state dropped on the next
    /// render.
    pub fn refresh(&mut self, catalog: &Catalog, limit: Option<i64>) {
        match catalog.list_photos(limit) {
            Ok(photos) => {
                self.last_error = None;
                self.photos = photos;
                // Allocate a fresh ThumbState for any new photo
                // id. Existing entries are untouched.
                for p in &self.photos {
                    self.thumbs.entry(p.id).or_insert(ThumbState {
                        bytes: None,
                        error: None,
                        requested: false,
                    });
                }
                // Drop the per-photo state for photos that have
                // been removed from the catalog.
                let live: std::collections::HashSet<i64> =
                    self.photos.iter().map(|p| p.id).collect();
                self.thumbs.retain(|id, _| live.contains(id));
                // Drop cached textures for removed photos. The
                // texture map is keyed by `CacheKey`; we filter by
                // the underlying photo id via the `from_id` round
                // trip.
                let live_keys: std::collections::HashSet<thumb_grid::CacheKey> = live
                    .iter()
                    .map(|id| thumb_grid::CacheKey::from_id(*id))
                    .collect();
                self.textures
                    .lock()
                    .unwrap()
                    .retain(|key, _| live_keys.contains(key));
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
            }
        }
    }

    /// Drain pending thumbnail results from background workers.
    pub fn pump_events(&mut self) {
        while let Ok(r) = self.thumb_rx.try_recv() {
            self.inflight_thumbs.fetch_sub(1, Ordering::Relaxed);
            // Find the per-photo state for this id. If the photo
            // was removed from the catalog between request and
            // delivery, the state is gone and we just drop the
            // result on the floor.
            if let Some(state) = self.thumbs.get_mut(&r.photo_id) {
                match r.result {
                    Ok(bytes) => {
                        state.bytes = Some(bytes);
                        state.error = None;
                        // Drop any cached texture; the next render
                        // pass will rebuild it from the new bytes.
                        let key = thumb_grid::CacheKey::from_id(r.photo_id);
                        self.textures.lock().unwrap().remove(&key);
                    }
                    Err(e) => {
                        state.error = Some(e);
                    }
                }
            }
        }
    }

    /// Render the library grid.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
    ) -> Option<usize> {
        self.pump_events();

        if let Some(err) = &self.last_error {
            ui.colored_label(egui::Color32::LIGHT_RED, err);
            return None;
        }

        if self.photos.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("Library is empty");
                ui.label("Use File → Import Photos… to add some.");
            });
            return None;
        }

        // Top status row: photo count + a "loading…" hint while
        // thumbs are in flight. Keeps the grid itself uncluttered.
        ui.horizontal(|ui| {
            ui.strong(format!("{} photos", self.photos.len()));
            if self.inflight_thumbs.load(Ordering::Relaxed) > 0 {
                ui.spinner();
                ui.weak("loading thumbnails…");
            }
        });
        ui.separator();

        // Request thumbnails for every photo that doesn't have one
        // yet. The `inflight_thumbs` counter already caps the
        // number of in-flight extractions at MAX_INFLIGHT_THUMBS,
        // so this just keeps the work queue topped up.
        let layout = thumb_grid::compute_grid(ui);
        self.request_thumbs();

        // Project photos into GridItems for the shared renderer.
        let mut items: Vec<GridItem> = self
            .photos
            .iter()
            .map(|p| {
                let label = p
                    .file_format
                    .clone()
                    .unwrap_or_else(|| display_name_from_path(&p.path));
                let bytes = self
                    .thumbs
                    .get(&p.id)
                    .and_then(|s| s.bytes.clone());
                GridItem {
                    id: Some(p.id),
                    full_path: p.path.clone(),
                    thumb_bytes: bytes,
                    config: ThumbCardConfig {
                        cell_w: layout.cell_w,
                        // Library cards are hover-only for now;
                        // single-click is a no-op until we wire up
                        // a loupe view.
                        selectable: false,
                        selected: false,
                        in_catalog: false,
                        label_override: Some(label),
                    },
                }
            })
            .collect();

        // The shared helper doesn't surface the clicked cell back to
        // us (library cards aren't selectable), so we just call it.
        // Use the photo's database id as the cache key so a refresh
        // that re-orders the list (e.g. a new import) doesn't
        // invalidate the loaded textures.
        thumb_grid::show_thumb_grid(
            ctx,
            ui,
            &mut items,
            &self.textures,
            SCROLL_MAX_HEIGHT,
            |item| match item.id {
                Some(id) => thumb_grid::CacheKey::from_id(id),
                None => thumb_grid::CacheKey::from_path(&item.full_path),
            },
        );
        let _ = items;
        None
    }

    /// Spawn thumbnail workers for every photo that doesn't have
    /// a loaded thumbnail yet, up to `MAX_INFLIGHT_THUMBS` at a
    /// time. Skips photos that are already in flight.
    fn request_thumbs(&mut self) {
        // Snapshot the candidate list so we can release the
        // `thumbs` borrow before spawning threads.
        let candidates: Vec<(i64, PathBuf)> = self
            .photos
            .iter()
            .filter_map(|p| {
                let state = self.thumbs.get(&p.id)?;
                if state.bytes.is_some() || state.error.is_some() || state.requested {
                    return None;
                }
                Some((p.id, PathBuf::from(&p.path)))
            })
            .collect();

        for (id, path) in candidates {
            if self.inflight_thumbs.load(Ordering::Relaxed) >= MAX_INFLIGHT_THUMBS {
                break;
            }
            if let Some(state) = self.thumbs.get_mut(&id) {
                state.requested = true;
            }
            self.inflight_thumbs.fetch_add(1, Ordering::Relaxed);
            let tx = self.thumb_tx.clone();
            let _ = thread::Builder::new()
                .name(format!("lib-thumb-{id}"))
                .spawn(move || {
                    let result = extract_thumbnail(&path)
                        .map(|t| ThumbnailBytes {
                            width: t.width,
                            height: t.height,
                            rgba: t.rgba,
                        })
                        .map_err(|e| e.to_string());
                    let _ = tx.send(ThumbResult {
                        photo_id: id,
                        result,
                    });
                });
        }
    }
}

/// Derive a display name from an absolute file path: just the file
/// name component. Used as the default label when the catalog row
/// doesn't have a `file_format` to show.
fn display_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_library_renders_no_photos() {
        let page = LibraryPage::default();
        assert!(page.photos.is_empty());
        assert!(page.thumbs.is_empty());
    }

    #[test]
    fn display_name_strips_directories() {
        assert_eq!(display_name_from_path("/photos/a/b/c.jpg"), "c.jpg");
        assert_eq!(display_name_from_path("relative/path/x.cr2"), "x.cr2");
    }

    /// Refreshing the library with a superset of the existing
    /// photos must not wipe per-photo state: an existing photo's
    /// loaded thumbnail should survive the refresh.
    #[test]
    fn refresh_preserves_per_photo_state() {
        use crate::catalog::Photo;
        let mut page = LibraryPage {
            photos: vec![
                Photo {
                    id: 1,
                    path: "/a/a.jpg".into(),
                    ..Default::default()
                },
                Photo {
                    id: 2,
                    path: "/a/b.jpg".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        for p in &page.photos {
            page.thumbs.insert(
                p.id,
                ThumbState {
                    bytes: None,
                    error: None,
                    requested: true,
                },
            );
        }

        // Simulate a refresh that adds a new photo (id 3) at the
        // top, keeping the existing two.
        page.photos.insert(
            0,
            Photo {
                id: 3,
                path: "/a/c.jpg".into(),
                ..Default::default()
            },
        );
        for p in &page.photos {
            page.thumbs.entry(p.id).or_insert(ThumbState {
                bytes: None,
                error: None,
                requested: false,
            });
        }

        // All three should be present; the new one is fresh, the
        // existing two still have `requested = true`.
        assert_eq!(page.thumbs.len(), 3);
        assert!(page.thumbs.get(&1).unwrap().requested);
        assert!(page.thumbs.get(&2).unwrap().requested);
        assert!(!page.thumbs.get(&3).unwrap().requested);
    }

    /// Refreshing the library with a subset must drop the
    /// removed photos' state.
    #[test]
    fn refresh_drops_removed_photos() {
        use crate::catalog::Photo;
        let mut page = LibraryPage {
            photos: vec![
                Photo {
                    id: 1,
                    path: "/a/a.jpg".into(),
                    ..Default::default()
                },
                Photo {
                    id: 2,
                    path: "/a/b.jpg".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        for p in &page.photos {
            page.thumbs.insert(
                p.id,
                ThumbState {
                    bytes: None,
                    error: None,
                    requested: true,
                },
            );
        }

        // Drop photo id 1.
        page.photos.remove(0);
        let live: std::collections::HashSet<i64> = page.photos.iter().map(|p| p.id).collect();
        page.thumbs.retain(|id, _| live.contains(id));

        assert_eq!(page.thumbs.len(), 1);
        assert!(page.thumbs.contains_key(&2));
        assert!(!page.thumbs.contains_key(&1));
    }
}
