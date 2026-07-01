//! realraw -- an open source Lightroom alternative.
//!
//! This crate exposes:
//! * [`catalog`] -- SQLite-backed Lightroom-style catalog (photos, folders,
//!   collections, keywords).
//! * [`task`] -- background task system with progress reporting,
//!   dependencies, smart grouping, and egui widgets.
//! * [`thumb_grid`] -- shared thumbnail card + grid rendering used by
//!   both the import dialog and the main library page.
//! * [`import`] -- photo import pipeline: discovery, EXIF, embedded
//!   thumbnails, and the in-window import dialog.
//! * [`demos`] -- sample task graphs (e.g. an import batch) that exercise
//!   the task system and double as working feature stubs.
//! * [`app`] -- top-level `App` state + eframe integration.

pub mod app;
pub mod catalog;
pub mod demos;
pub mod import;
pub mod task;
pub mod thumb_grid;

/// Raw bytes of the application logo / icon (2048×2048 PNG).
pub static ICON_PNG: &[u8] = include_bytes!("../assets/icon-2048.png");
