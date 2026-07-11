# realraw

<p align="center">
  <img src="assets/icon-2048.png" width="200" alt="realraw logo" />
</p>

An open-source Lightroom alternative, written in Rust.

## Nightly Builds
https://realraw.sker.lol

## Features

- **photo library management via collections** -- stores photos in a collection which can easily be migrated to another device or an external drive.
- ~~**image manipulation** -- raw photo processing~~

## Requirements

- Rust toolchain (edition 2024)

## Build & Run

```bash
cargo build --release
cargo run
```

## Tests

```bash
cargo test
```

## Packaging

The binary can be packaged into platform-specific installers via `scripts/package.sh` (Unix) or `scripts\package.bat` (Windows):

| Command | Requires | Output |
|---------|----------|--------|
| `./scripts/package.sh app-macos` | `cargo install cargo-bundle` | `.app` bundle |
| `./scripts/package.sh dmg` | `cargo-bundle` + `brew install create-dmg` | `.dmg` disk image |
| `./scripts/package.sh deb` | `cargo install cargo-deb` | `.deb` (Debian/Ubuntu) |
| `./scripts/package.sh appimage` | `wget`, `desktop-file-validate` | AppImage (any Linux) |
| `package.sh exe` / `package.bat exe` | nothing extra | `.exe` with icon embedded |
| `package.sh nsis` / `package.bat nsis` | [NSIS](https://nsis.sourceforge.io/) | `realraw-<ver>-setup.exe` |
| `package.sh wix` / `package.bat wix` | [WiX Toolset v3](https://wixtoolset.org/) | `realraw-<ver>-x64.msi` |
| `package.sh all` / `package.bat all` | tools for current OS | runs available commands |

**Windows packaging tools (local, Scoop):**

```bash
scoop bucket add extras
scoop install wixtoolset nsis
```

The Windows `.exe` icon is embedded automatically at compile time via `build.rs`.  
Installer sources live in `packaging/windows/` (`realraw.nsi`, `realraw.wxs`).  
The macOS `.icns` and Windows `.ico` are generated from `assets/icon-2048.png`.

## License

AGPL-3.0-or-later. See [`LICENSE`](LICENSE).
