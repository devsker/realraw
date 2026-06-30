# realraw

An open-source Lightroom alternative, written in Rust.

Realraw is a native desktop photo management app with an SQLite-backed catalog,
a multi-stage import pipeline (discovery, EXIF extraction, hashing), and an
egui-based GUI.

## Features

- **SQLite catalog** -- photos, folders, collections, keywords, full-text search
- **Import pipeline** -- file discovery across ~25 raw/image formats, EXIF/IPTC
  metadata extraction, SHA-1 deduplication, embedded thumbnail extraction with
  smart JPEG scanning fallback
- **Background task system** -- concurrent workers with progress reporting,
  dependency tracking, and task-group cancellation
- **Thumbnail grid** -- GPU-cached thumbnails with 3:2 aspect-ratio cards,
  selection state, and lazy loading
- **egui/eframe GUI** -- library view, import dialog with preview, tasks panel,
  menubar and status bar

## Requirements

- Rust toolchain (edition 2024)

No system dependencies beyond what your desktop provides (OpenGL/Metal/Vulkan
are handled by eframe; SQLite is bundled).

## Build & Run

```bash
cargo build --release
cargo run
```

On first launch, a default catalog is created at `~/Pictures/realraw/catalog.sqlite`.

## Tests

```bash
cargo test
```

## License

AGPL-3.0-or-later. See [`LICENSE`](LICENSE).
