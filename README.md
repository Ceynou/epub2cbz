# epub2cbz

Cross-platform Rust tool to convert `.epub` manga and comic ebooks into `.cbz` files with `ComicInfo.xml` metadata.

## Download

Pre-built binaries for Linux, macOS and Windows are available from [Releases](../../releases/latest).

## Features

- Extracts images from EPUB (standard, Barnes & Noble, OpenMangaFormat)
- Generates `ComicInfo.xml` with title, author, series, reading direction, chapters, ISBN/ASIN
- Duplicate cover detection via perceptual image hashing
- Page spread alignment with blank page insertion (RTL/LTR)
- Image cropping (border removal), resizing, double-page splitting
- DRM detection (refuses encrypted files)
- Multi-threaded image processing with rayon
- Zero-copy passthrough when no processing is needed (fast)
- CLI with progress bars and parallel file processing
- Native GUI (Blitz/dioxus-native, WGPU rendered, experimental)

## CLI Usage

```
epub2cbz [OPTIONS] <INPUT>...
```

**Examples:**
```sh
# Convert a single file
epub2cbz manga.epub

# Convert all epubs in a folder, output to a different directory
epub2cbz -r ./epubs/ -o ./cbz/

# Convert with image cropping and resize to Kindle resolution
epub2cbz --crop true --resize 1264x1680 manga.epub

# Simple extraction (no processing, fastest)
epub2cbz -s manga.epub
```

**Options:**
| Flag | Description |
|------|-------------|
| `-o, --output <DIR>` | Output directory (default: same as input) |
| `-r, --recursive` | Process directories recursively |
| `-s, --simple` | Simple extraction (no processing) |
| `--split-spreads <BOOL>` | Split double-page spreads |
| `--crop <BOOL>` | Crop image borders |
| `--resize <WxH>` | Resize images (e.g. `1264x1680`) |
| `--comic-info <BOOL>` | Generate ComicInfo.xml |
| `--page-spread <BOOL>` | Page spread alignment |
| `--format <FMT>` | Output image format: `jpg`, `png`, `webp` |
| `-j, --jobs <N>` | Parallel jobs (default: CPU count) |
| `-q, --quiet` | Suppress output |

## Build from Source

### Requirements

- [Rust](https://rustup.rs/) 1.75 or later

### CLI only (recommended)

```sh
cargo build --release --features cli --no-default-features
```

The binary is at `target/release/epub2cbz`.

### With GUI (experimental)

The GUI uses [Blitz](https://github.com/DioxusLabs/blitz) (dioxus-native), a native HTML/CSS renderer powered by WGPU/Vello — no webview required.

```sh
cargo build --release --features gui,cli
```

Run with no arguments to open the GUI, or pass files/folders for CLI mode.

### Install system-wide

```sh
cargo install --path . --features cli --no-default-features
```

## Naming Convention

For best metadata extraction, name your files as `[Series] v[Volume].epub` or `[Series] [Number].epub`, e.g.:
- `Two Pieces v69.epub`
- `イエスタデイをうたって 1.epub`

The volume/number is extracted from the filename when calibre series metadata is not present in the EPUB.

## Configuration

Settings are stored in TOML format at:
- Linux: `~/.local/share/epub2cbz/epub2cbz.toml`
- macOS: `~/Library/Application Support/epub2cbz/epub2cbz.toml`
- Windows: `%LOCALAPPDATA%\epub2cbz\epub2cbz.toml`

## Licenses

| Library | Usage | License |
|---------|-------|---------|
| [image](https://github.com/image-rs/image) | Image decoding/encoding | MIT/Apache-2.0 |
| [zip](https://github.com/zip-rs/zip2) | ZIP/CBZ reading/writing | MIT |
| [roxmltree](https://github.com/razrfalcon/roxmltree) | XML parsing | MIT/Apache-2.0 |
| [quick-xml](https://github.com/tafia/quick-xml) | XML writing | MIT |
| [rayon](https://github.com/rayon-rs/rayon) | Parallel processing | MIT/Apache-2.0 |
| [clap](https://github.com/clap-rs/clap) | CLI argument parsing | MIT/Apache-2.0 |
| [blitz / dioxus-native](https://github.com/DioxusLabs/blitz) | Native GUI (WGPU/Vello) | MIT/Apache-2.0 |

## Privacy

This program processes all files locally and does not collect any data or make network requests.
