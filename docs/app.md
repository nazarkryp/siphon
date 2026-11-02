# Siphon — video and file downloader

> "Siphon" is a working name. It can change.

## Description

Siphon is a small desktop app that downloads videos and files from the web.
You paste a link. Siphon finds the video or file behind the link and downloads it to your PC.

Siphon is a lighter alternative to JDownloader 2. It keeps only the parts that a person needs to
download videos and files:

- It finds the stream on a page.
- It downloads fast with parallel connections.
- It continues a download after a stop.
- It joins video streams into one file.

It does not have site accounts, captcha solving, router reconnect, or archive extraction.

### Platform and stack

- Desktop app for Windows. macOS and Linux can follow later.
- Tauri 2 with a Rust backend. The Rust backend does all the download work.
- React with TypeScript for the window.
- ffmpeg is required. Siphon uses it to join HLS segments and to merge video with audio. It copies
  the streams and does not re-encode them.
- yt-dlp is optional. Siphon uses it only to find the streams on sites that Siphon cannot read by
  itself, for example YouTube.

## How Siphon gets a download

When you add a link, Siphon tries these steps in this order. It stops at the first step that works.

1. **Direct file.** The server sends a file, for example `.mp4`, `.zip` or `.pdf`. Siphon reads the
   name and size from the HTTP headers.
2. **HLS playlist.** The link is an `.m3u8` playlist. Siphon reads the qualities from the master
   playlist.
3. **yt-dlp.** yt-dlp is installed and knows the site. yt-dlp returns the available formats.
4. **Page scan.** Siphon loads the HTML page and finds `<video>` and `<source>` tags and `.m3u8`
   and `.mp4` links.

If more than one stream or quality is found, you choose one. The best quality is selected by default.

## Features

### v1 (first version)

**Add links**

1. Paste one or more links (one link per line) and add them to the queue.
2. Clipboard watch (on/off). When you copy a link that Siphon can download, Siphon asks you before
   it adds the link.
3. Set a Referer and a User-Agent for each link. Many HLS servers refuse a request without the
   Referer of the page.

**Find the stream**

4. Direct file: get the file name and size from the HTTP headers.
5. HLS: read the master `.m3u8` and show the qualities (resolution, bitrate, estimated size).
6. Page scan: find video links in the HTML of a page.
7. yt-dlp (optional): get the formats for sites that need it.
8. Choose a stream and a quality before the download starts.

**Download engine**

9. Chunked download: split a file into N parts and download them in parallel with HTTP `Range`
   requests. Write into a `.part` file. Rename the file when it is complete.
10. Resume: continue a download after a pause and after an app restart.
11. HLS download:
    - Download the segments in parallel.
    - Decrypt AES-128 segments.
    - Use ffmpeg to join the segments into one MP4 file.
12. Separate video and audio: when yt-dlp returns two streams, download both. Then ffmpeg merges
    them into one file.
13. Retry with a wait time that increases after each failure, for each part and each segment.

**Queue**

14. A limit for the number of downloads at the same time. Other downloads wait in the queue.
15. Pause, resume, cancel, retry and remove a download.
16. The queue stays after an app restart.

**Window**

17. Download list. Each row shows:
    - name, source site and size
    - type: file, HLS or yt-dlp
    - status: queued, downloading, paused, merging, done or failed
    - progress, speed and time left
    - a detailed progress view: one bar for each parallel connection, or the HLS segments, or
      the video and audio streams
    - the error message and a fix hint, when a download fails
18. Filter the list by status: all, active, queued, done, failed.
19. Total download speed.
20. Open the file, and show the file in its folder.
21. Settings:
    - download folder
    - downloads at the same time
    - connections per file
    - ffmpeg path and status
    - yt-dlp path and status
    - clipboard watch on/off

### Later

- Speed limit
- Cookies from a `cookies.txt` file or from a browser, for pages that need a login
- Subtitles
- Browser extension ("send to Siphon")
- Auto-update

### Not in scope

- Streams with DRM protection (Widevine and similar). Siphon does not try to remove DRM.
- Captcha solving, premium file-host accounts, router reconnect
- Archive extraction, download scheduler, remote control

## Screens

These screens are necessary for v1. The designer decides the layout and the visual style.

1. **Queue (main window)**
   - link field and "Fetch" button
   - filter by status
   - download list (see feature 17)
   - total speed
   - status line: ffmpeg status, yt-dlp status, download folder
2. **Add link**
   - the source link
   - the result of each step: direct file, HLS, yt-dlp, page scan
   - the list of streams and qualities to choose from
   - file name, folder, Referer and User-Agent
   - estimated size, "Cancel" and "Download"
3. **Settings** (see feature 21)
4. **Clipboard prompt.** A small message that shows the copied link, with "Add" and "Ignore".

### States to design

- Empty queue (first start)
- A download in each status: queued, downloading, paused, merging, done, failed
- ffmpeg not found. Downloads that need ffmpeg cannot start.
- yt-dlp not found. Siphon works, but sites that need yt-dlp fail.
- No stream found on a page
- Many streams found on a page

## Design rules

- Do not use the Windows 11 style: no Mica, no acrylic, no Fluent controls, no system title bar.
  The app draws its own window bar.
- The app window is a desktop window. The smallest window size is about 900 × 600 px.
- Text and controls must be easy to read. Text contrast is at least 4.5:1.
- Do not show a status with color only. Also use text or an icon.
