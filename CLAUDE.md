# Siphon

A small desktop downloader for videos and files. Stack: Tauri 2 with a Rust backend (`src-tauri/`) and React + TypeScript (`src/`).

- Requirements and the feature list (v1, Later, Out of scope): `../app.md`. Read it before you add a feature.
- Design source: `../Siphon.html`. It is a bundled export. Each screen is a gzip+base64 entry in the `__bundler/manifest` script, and each entry's `__bundler/template` script holds the screen HTML as a JSON string. Unpack it with a script; Read cannot use it directly.

## Checks

A change is done when all of these are green:

| Where | Command |
|---|---|
| `siphon/` | `npm run build` (tsc + vite) |
| `siphon/src-tauri/` | `cargo test --lib`, `cargo clippy --all-targets` (0 warnings), `cargo fmt` |

- `cargo test --lib -- --ignored` also runs the network test, which installs the real yt-dlp release (18 MB).
- Five tests in `core.rs` use the real ffmpeg on PATH. They skip silently when ffmpeg is missing (the conversion test also when ffmpeg has no VP9 or Opus encoder). Check with `-- --nocapture` that they did not print "skipped".
- Run the app: `npm run tauri dev` from `siphon/`. It opens a window on the user's PC. Ask the user before you start it.

## How a download works

1. **Resolve** (`resolve/mod.rs`): a link goes through four steps in this order. The first step that finds streams wins, and the later steps are marked `skipped`.
   1. `direct.rs`: a `Range: bytes=0-0` probe classifies the response as file, playlist or page.
   2. `hls.rs`: reads a master or media playlist and gives one quality per variant.
   3. `ytdlp.rs`: runs `yt-dlp -J` and pairs video-only formats with audio.
   4. `page.rs`: scans the HTML for media links.

   The result holds a `Choice` (a `JobSpec`) for each stream and quality. `Core` caches it by `resolveId` for 1 hour.
2. **Add** (`core.rs` `add_download`): the `Choice` becomes a `StoredDownload` with three parts:
   - `view`: the `Download` that the UI shows
   - `spec`: the sources and their headers
   - `parts`: the resume state of each source
3. **Schedule**: `schedule()` starts `Queued` entries until `maxDownloads` jobs run. `start_now` ignores that limit.
4. **Execute** (`core.rs` `execute`): the path depends on the `JobSpec`:

   | `JobSpec` | Engine | Then |
   |---|---|---|
   | one HTTP source | `engine/http.rs` writes `<final>.part` | rename |
   | HLS, separate video + audio, a subtitle, or a compatible format | `engine/hls.rs` / `engine/http.rs` work in `<folder>/.siphon/<id>/` | ffmpeg (copy, or H.264/AAC for a compatible format) into `<stem>.siphon-tmp.<ext>`, then rename |

5. **Finish** (`finish`): the stop reason (`Pause`, `Cancel`, `Remove`) or the result sets the final status. Then the state is saved, the UI gets an event, and `schedule()` runs again.

## Contract between Rust and the UI

- `src-tauri/src/model.rs` and `src/api/types.ts` describe the same shapes. Change both files in the same edit. Serde uses camelCase.
- The UI calls Rust only through `src/api/index.ts`. A new command needs four edits:
  1. a method on `Core`
  2. a function in `commands.rs`
  3. an entry in `generate_handler!` in `lib.rs`
  4. a wrapper in `api/index.ts`

  Tauri maps camelCase argument keys from JavaScript to snake_case Rust arguments.
- Events: `download-updated` (a full `Download`), `download-removed` `{id}`, `tools-changed`, `clipboard-link` `{url}`, `tool-install-progress`.
- Window frame: on Windows and Linux the window has no system frame (`decorations: false`), and `WindowBar` draws the caption buttons. On macOS, `tauri.macos.conf.json` replaces the whole window entry (Tauri does not merge arrays, so change both files): native frame, `titleBarStyle: "Overlay"`, and traffic lights centred in the 56 px bar. `WindowBar` then draws no buttons (`lib/platform.ts` `isMac`). The link form sits in `WindowBar`, so `QueueScreen` renders the bar. The loading and error screens render an empty bar. On macOS a `Dialog` with a footer is a sheet (`dialog--sheet`): it hangs from the 56 px bar, has no Close button, and shows the title in the body. So every such dialog needs a Cancel or Close button in its footer.
- A JavaScript call to a window API needs its permission in `src-tauri/capabilities/default.json`. Rust-side plugin calls (dialog, opener, clipboard) need no permission.

## Backend invariants

- **State lock**: `Core.state` is a `std::sync::Mutex`. Take it, change the state, and release it before any `.await`. Engines write progress through `PartSink` (`engine/mod.rs`). Each write takes the lock once.
- **View refresh**: never set byte counts or `detail` by hand. Change `parts`, set `dirty`, and let `StoredDownload::refresh_view` (`job.rs`) compute the view.
- **Events and saving**: the 500 ms ticker (`flush(true)`) computes the speed (an EMA), emits the dirty entries, and saves `downloads.json` at most every 2 s. Status changes save at once (`after_change`).
- **Resume (HTTP)**: the chunks are stored in `downloads.json`. Resume happens only when the server's total size and its ETag/Last-Modified still match. Otherwise the download starts again. A `DlError` with `restart: true` (HTTP 416, or the server stopped accepting ranges) clears the part state, so Retry starts from zero.
- **Resume (HLS)**: a segment counts as done when its file `seg_NNNNNN.bin` exists. Each segment is written to `.tmp` and then renamed, so a partial file never looks done.
- **Stopping a job**: set `run.stop` first, then cancel the token. `finish()` reads the reason. A download that is removed while it runs stays in the list with `removed = true`, hidden from the UI, until its job ends and cleans up.
- **Startup**: `Downloading`, `Merging` and `Converting` entries come back as `Paused`.
- **needsFfmpeg**: `begin()` sets it when `spec.needs_ffmpeg()` is true and ffmpeg is missing. `schedule()` sets it back to `Queued` when ffmpeg is found.
- **ffmpeg**: it remuxes (`-c:v copy -c:a copy`). A text subtitle track is converted to the container's format (`mov_text` for MP4, `webvtt` for WebM, `srt` for MKV). Build arguments with `core::ffmpeg_args` (output part: `tools::output_args`) and run it with `tools::run_ffmpeg`. The temporary output keeps the real extension, because ffmpeg picks the container from it.
- **Compatible format** (the only re-encode): a video quality that the default player cannot open (VP9, AV1, Opus, WebM, MKV) gets `Quality.unsupportedFormat` and a `Choice.convert` from `resolve::compatible_convert` (yt-dlp and HLS). When `AddRequest.compatibleFormat` is true (Add link checkbox; default from `Settings.compatibleFormat`, on), `add_download` copies it to `JobSpec.convert` and the output becomes `.mp4`. `execute` then sets the `Converting` status and re-encodes only the incompatible track: video to H.264 at `core::h264_bitrate` (twice the source, with a minimum per height, max 20 Mbps), audio to AAC 192k. `tools::h264_encoder` picks the first encoder that passes a test encode (macOS `h264_videotoolbox`, Windows `h264_nvenc`/`h264_qsv`/`h264_amf`, then `libx264`) and caches it per ffmpeg file. An unknown codec counts as compatible. HDR is not tone-mapped. UI texts say "save in a compatible format", not "convert".
- **Output check**: after each ffmpeg step, `tools::check_output` decodes the first 10 s of the video. More than 20 error lines fail the job. This exists because an ffmpeg development build (`N-124294`, May 2026) wrote MP4 files with a broken H.264 track: the sound played, the video did not, and the file had the full size. Settings warns about development builds (`ToolStatus.devBuild`).
- **External programs**: start them with `tools::command` (no console window on Windows, killed when dropped).
- **HLS parsing**: always parse playlists with `engine/hls.rs` `parse_media_playlist`. It fixes an m3u8-rs 6.0.1 bug that drops `#EXT-X-KEY:METHOD=NONE`.
- **Async commands**: their futures must be `Send`. An `async move` block inside `stream.map(|x| ...)` that borrows data fails with "implementation of `FnOnce` is not general enough". Move the body into a named `async fn` (as in `resolve/hls.rs` `load_variant`).
- **Out of scope**: live HLS and DRM (SAMPLE-AES, key formats other than identity, yt-dlp `has_drm`) fail with a clear message. Keep it that way.
- **yt-dlp sources**: Siphon uses only `http`, `https`, `m3u8` and `m3u8_native` formats and skips DASH fragments. yt-dlp URLs expire after some hours, so an old queued job can fail with HTTP 403. Each height gets at most two rows: the best that `resolve::plays_everywhere` accepts (H.264/AAC in MP4, which QuickTime opens) and the best other one (VP9, AV1, Opus, WebM, MKV). The Add link dialog selects the first (best) row; rows that are not `playsEverywhere` show a "Not for QuickTime" note.
- **Subtitles** (yt-dlp only): `ytdlp.rs` `subtitle_tracks` offers the uploaded tracks and the speech-to-text tracks in the video's own language. It skips machine translations (`tlang=` in the URL), HLS caption playlists, and the duplicate `en` of `en-orig` (same URL). It takes SRT before VTT, because YouTube's speech-to-text VTT repeats each line. Only video choices in MP4, WebM or MKV get the tracks. `add_download` puts the chosen track in `JobSpec.subtitle`. `execute` downloads it in one GET in the merge step (no progress row). MP4 needs three-letter language codes (`ffmpeg` drops `en`), so `iso639_2` converts them. One track per file.
- **Clipboard watch** (`clipboard.rs` `link_in`): a copied link gives a prompt only when it is a video page on a known video site (`resolve::is_video_page`) or its path ends with an extension in `FILE_EXTENSIONS`. Home pages, profiles, articles and links without an extension give no prompt. Detection makes no network request. A new site in `resolve::VIDEO_SITES` needs its video page paths in `is_video_page`, or it never matches.
- **Smart prefetch** (`Core::prefetch`): when the clipboard watcher sees a new link, it resolves it in the background, but only when `smartPrefetch` is on, yt-dlp is found, and `resolve::is_video_page` accepts the link. Other links can work only once (password reset, login), so never prefetch them. One prefetch runs at a time. A new copy or Ignore (`cancel_prefetch`) stops it. `resolve_link` without a Referer and User-Agent takes the prefetch for the same URL and resolves again if it failed.
- **Tool installer**: `tools_install.rs` downloads the official builds (yt-dlp from GitHub; ffmpeg from gyan.dev on 64-bit Windows and from martin-riedl.de on macOS and Linux, x86_64 and arm64) and checks the SHA-256 from the release's hash file. martin-riedl.de answers a GET on `/redirect/latest/...` with a redirect to the newest build (a HEAD gives 404). Siphon reads the target without following it, then downloads the build and `<build>.sha256` from it. The old program is replaced only after the check passes. Tools go to `<app data>/tools/`. `ToolStatus.installable` is false on a system with no source: the UI then shows no Install or Update button, and the update check is skipped.
- **App data** (`%APPDATA%\dev.siphon.app\`): `settings.json` and `downloads.json`. A file that does not parse is renamed to `*.json.broken`, and Siphon uses the defaults.
- **User-facing errors**: `DlError { message, hint }` (`net.rs`) becomes `Download.error`. Write the message and the hint for the user, in short plain sentences.

## Tests

- `test_server.rs` is a local HTTP/1.1 server with `Range` support. Options: ignore ranges, a fixed error status, a delay per chunk, and counters for parallel requests and sent bytes. Engine tests use it with `engine::testing::MemorySink`.
- `Core` needs a Tauri `AppHandle`, so it has no unit tests. Put logic you want to test in plain functions (for example `ffmpeg_args`) and test those.

## Frontend

- `src/styles/tokens.css` is the only place that defines colors, sizes, radii and shadows. Components use its variables.
- State: `src/state/reducer.ts` + `store.tsx`. Events update downloads by id. `dialogs.ts` opens Add link, Settings, Options and Remove.
- Screens are in `src/features/<feature>/`, shared controls in `src/components/`.
- Only the colors of the light theme exist. The window works from 900 × 600 px, and narrow widths hide the Speed and Time-left columns.

## Known gaps

- No Browse button for the tool paths. The Tools button in Settings ("Check for updates" or "Detect again") checks only saved paths.
- There is no "retrying" state for segments and no speed for each connection.
- There is no command to open a web URL or the download folder.
- The install flow and most screens are not yet tested by hand in the running app.
