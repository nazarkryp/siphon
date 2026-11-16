# Siphon

Siphon is a small desktop downloader for videos and files. It uses Tauri 2 with a Rust backend (`src-tauri/`) and a React + TypeScript UI (`src/`).

## Requirements

- Node.js and npm
- Rust (stable) with cargo
- The Tauri system packages for your OS: <https://tauri.app/start/prerequisites/>. On macOS, this is the Xcode Command Line Tools.

## Run

```sh
npm install
npm run tauri dev
```

The first run compiles all Rust crates and takes some minutes.

## Build

```sh
npm run tauri build
```

The installers go to `src-tauri/target/release/bundle/`.

## External tools

- Siphon uses yt-dlp to resolve links from video sites.
- Siphon uses ffmpeg to join HLS segments and separate video and audio streams. It only copies the streams and never re-encodes them.

Install both from the Settings screen, or put them on your PATH. A download that needs ffmpeg waits until Siphon finds it.

## App data

Siphon keeps `settings.json`, `downloads.json` and the installed tools in its app data folder:

| OS | Folder |
|---|---|
| Windows | `%APPDATA%\dev.siphon.app\` |
| macOS | `~/Library/Application Support/dev.siphon.app/` |
| Linux | `~/.local/share/dev.siphon.app/` |

## Docs

- `docs/app.md`: requirements and the feature list.
- `docs/Siphon.html`: the design source.
- `CLAUDE.md`: architecture, the Rust–UI contract, backend rules, and the checks a change must pass.
