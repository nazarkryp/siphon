//! The download queue: scheduling, running jobs, progress events and persistence.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared};
use reqwest::Client;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::engine::{self, PartSink};
use crate::job::{MediaSource, PartState, SourceKind, StoredDownload, SubtitleSource};
use crate::model::{
    AddLinksResult, AddRequest, AppState, Download, DownloadError, DownloadStatus, FailedLink,
    ProgressDetail, ResolveResult, ResolveStepState, Settings, ToolUpdate, ToolUpdates,
    ToolsStatus,
};
use crate::net::{self, DlError};
use crate::resolve::{self, Choice};
use crate::tools_install::{self, ToolId};
use crate::{store, tools, util};

const RESOLVE_TTL: Duration = Duration::from_secs(60 * 60);
const SAVE_INTERVAL: Duration = Duration::from_secs(2);

enum StopReason {
    Pause,
    Cancel,
    Remove { delete_file: bool },
}

struct Run {
    cancel: CancellationToken,
    stop: Option<StopReason>,
}

struct SpeedMeter {
    last_bytes: u64,
    last_at: Instant,
    ema: f64,
}

impl SpeedMeter {
    fn sample(&mut self, done: u64, now: Instant) -> u64 {
        let dt = now.duration_since(self.last_at).as_secs_f64();
        if dt >= 0.2 {
            let inst = done.saturating_sub(self.last_bytes) as f64 / dt;
            self.ema = if self.ema == 0.0 {
                inst
            } else {
                0.4 * inst + 0.6 * self.ema
            };
            self.last_bytes = done;
            self.last_at = now;
        }
        self.ema as u64
    }
}

struct Entry {
    stored: StoredDownload,
    run: Option<Run>,
    /// Removed while running. Hidden from the UI until the job stops and cleans up.
    removed: bool,
    dirty: bool,
    meter: Option<SpeedMeter>,
}

impl Entry {
    fn id(&self) -> &str {
        &self.stored.view.id
    }
}

struct State {
    settings: Settings,
    tools: ToolsStatus,
    /// False until the first detection ends. Until then `tools` shows nothing as found.
    tools_detected: bool,
    entries: Vec<Entry>,
    save_pending: bool,
    last_save: Instant,
}

struct CachedResolve {
    at: Instant,
    url: String,
    choices: HashMap<(String, String), Choice>,
}

struct JobContext {
    spec: crate::job::JobSpec,
    folder: PathBuf,
    file_name: String,
    referer: Option<String>,
    user_agent: Option<String>,
    connections: u32,
    ffmpeg: Option<PathBuf>,
}

/// A resolve that started when the user copied a video-site link.
struct Prefetch {
    url: String,
    at: Instant,
    result: Shared<BoxFuture<'static, CmdResult<ResolveResult>>>,
    task: tauri::async_runtime::JoinHandle<()>,
}

pub struct Core {
    app: AppHandle,
    client: Client,
    data_dir: PathBuf,
    state: Mutex<State>,
    resolves: Mutex<HashMap<String, CachedResolve>>,
    prefetch: Mutex<Option<Prefetch>>,
    installing: Mutex<HashSet<ToolId>>,
}

type CmdResult<T> = Result<T, String>;

pub fn final_path(view: &Download) -> PathBuf {
    Path::new(&view.folder).join(&view.file_name)
}

fn part_path(final_path: &Path) -> PathBuf {
    let mut s = final_path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

fn work_dir(view: &Download) -> PathBuf {
    Path::new(&view.folder).join(".siphon").join(&view.id)
}

/// ffmpeg picks the container from the extension, so the temporary name keeps it.
fn temp_output(final_path: &Path) -> PathBuf {
    let name = final_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output.mp4");
    let (stem, ext) = util::split_ext(name);
    final_path.with_file_name(format!("{stem}.siphon-tmp{ext}"))
}

/// Remuxes one input, or merges video from the first input with audio from the second.
/// A subtitle file becomes one more track. Video and audio are copied, unless `recode` says
/// to change them to a compatible format.
fn ffmpeg_args(
    video: &Path,
    audio: Option<&Path>,
    subtitle: Option<(&Path, &SubtitleSource)>,
    recode: Option<&tools::Recode>,
    out: &Path,
) -> Vec<String> {
    // ffmpeg applies an option to the next file, so all inputs come before the output options.
    let mut args = vec!["-i".to_string(), video.display().to_string()];
    for path in audio.into_iter().chain(subtitle.map(|(p, _)| p)) {
        args.extend(["-i".into(), path.display().to_string()]);
    }
    match audio {
        Some(_) => args.extend(["-map".into(), "0:v:0".into(), "-map".into(), "1:a:0".into()]),
        None => args.extend(["-map".into(), "0:v?".into(), "-map".into(), "0:a?".into()]),
    }
    if let Some((_, sub)) = subtitle {
        let input = if audio.is_some() { 2 } else { 1 };
        args.extend([
            "-map".into(),
            format!("{input}:s:0"),
            "-metadata:s:s:0".into(),
            format!("title={}", sub.label),
        ]);
        if let Some(lang) = &sub.language {
            args.extend(["-metadata:s:s:0".into(), format!("language={lang}")]);
        }
    }
    args.extend(tools::output_args(out, subtitle.is_some(), recode));
    args
}

/// The H.264 bitrate for a converted video. H.264 needs about twice the bitrate of VP9 or AV1
/// for the same picture. The minimum keeps a low source bitrate from making a blurry file.
fn h264_bitrate(source: Option<u64>, height: Option<u32>) -> u64 {
    const MAX: u64 = 20_000_000;
    let (min, unknown) = match height.filter(|h| *h > 0).unwrap_or(1080) {
        0..=480 => (800_000, 1_500_000),
        481..=720 => (1_500_000, 3_000_000),
        721..=1080 => (2_500_000, 5_000_000),
        1081..=1440 => (5_000_000, 9_000_000),
        _ => (8_000_000, 16_000_000),
    };
    match source.filter(|b| *b > 0) {
        Some(b) => (b * 2).clamp(min, MAX),
        None => unknown,
    }
}

/// Subtitle files are small text files. A bigger answer is not a subtitle.
const MAX_SUBTITLE_BYTES: usize = 20 * 1024 * 1024;

/// Downloads a subtitle file in one request. It is a few kilobytes, so it has no progress.
async fn fetch_subtitle(
    client: &reqwest::Client,
    sub: &SubtitleSource,
    headers: reqwest::header::HeaderMap,
    path: &Path,
    cancel: &CancellationToken,
) -> Result<(), DlError> {
    let failed = |reason: String| {
        DlError::fatal(
            format!("Siphon could not download the subtitles ({reason})."),
            Some("Add the link again. To get the video without subtitles, choose None."),
        )
    };
    let request = async {
        let response = client
            .get(&sub.url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| failed(DlError::from_reqwest(&e).message))?;
        let status = response.status();
        if !status.is_success() {
            return Err(failed(format!("HTTP {}", status.as_u16())));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_SUBTITLE_BYTES as u64)
        {
            return Err(failed("the file is too big".into()));
        }
        let body = response
            .bytes()
            .await
            .map_err(|e| failed(DlError::from_reqwest(&e).message))?;
        if body.len() > MAX_SUBTITLE_BYTES {
            return Err(failed("the file is too big".into()));
        }
        Ok(body)
    };
    let body = tokio::select! {
        _ = cancel.cancelled() => return Err(DlError::cancelled()),
        r = request => r?,
    };
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| DlError::io(&e))?;
    }
    tokio::fs::write(path, &body)
        .await
        .map_err(|e| DlError::io(&e))
}

fn cancelled_error() -> DownloadError {
    DownloadError {
        message: "Cancelled.".into(),
        hint: Some("Retry to download it again from the start.".into()),
    }
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl Core {
    pub fn new(
        app: AppHandle,
        data_dir: PathBuf,
        settings: Settings,
        downloads: Vec<StoredDownload>,
    ) -> Arc<Self> {
        let entries = downloads
            .into_iter()
            .map(|mut stored| {
                stored.refresh_view();
                Entry {
                    stored,
                    run: None,
                    removed: false,
                    dirty: false,
                    meter: None,
                }
            })
            .collect();
        Arc::new(Self {
            app,
            client: net::build_client(),
            data_dir,
            state: Mutex::new(State {
                settings,
                tools: ToolsStatus::default(),
                tools_detected: false,
                entries,
                save_pending: false,
                last_save: Instant::now(),
            }),
            resolves: Mutex::new(HashMap::new()),
            prefetch: Mutex::new(None),
            installing: Mutex::new(HashSet::new()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn app_state(&self) -> AppState {
        let s = self.lock();
        AppState {
            downloads: s
                .entries
                .iter()
                .filter(|e| !e.removed)
                .map(|e| e.stored.view.clone())
                .collect(),
            settings: s.settings.clone(),
            tools: s.tools_detected.then(|| s.tools.clone()),
        }
    }

    pub fn settings(&self) -> Settings {
        self.lock().settings.clone()
    }

    pub fn app(&self) -> &AppHandle {
        &self.app
    }

    pub fn has_source(&self, url: &str) -> bool {
        self.lock()
            .entries
            .iter()
            .any(|e| !e.removed && e.stored.view.source_url == url)
    }

    // ---- settings and tools ----

    pub fn set_tools(self: &Arc<Self>, tools: ToolsStatus) {
        {
            let mut s = self.lock();
            s.tools = tools.clone();
            s.tools_detected = true;
        }
        let _ = self.app.emit("tools-changed", tools);
        self.schedule();
    }

    pub async fn detect_tools(self: &Arc<Self>) -> ToolsStatus {
        let tools = tools::detect(&self.settings()).await;
        self.set_tools(tools.clone());
        tools
    }

    /// Reads the newest release of each tool. Siphon compares it with the found version in the UI.
    pub async fn check_tool_updates(&self) -> ToolUpdates {
        let (ffmpeg, yt_dlp) = tokio::join!(
            self.check_tool_update(ToolId::Ffmpeg),
            self.check_tool_update(ToolId::YtDlp),
        );
        ToolUpdates { ffmpeg, yt_dlp }
    }

    /// A tool that Siphon cannot install on this system gets no release check.
    async fn check_tool_update(&self, tool: ToolId) -> ToolUpdate {
        if !tools_install::can_install(tool) {
            return ToolUpdate {
                latest: None,
                error: None,
            };
        }
        tools_install::latest_version(&self.client, tool)
            .await
            .into()
    }

    /// Installs or updates a tool in the app data folder and saves its path in Settings.
    pub async fn install_tool(self: &Arc<Self>, tool: ToolId) -> CmdResult<ToolsStatus> {
        if !self
            .installing
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(tool)
        {
            return Err("This tool is already installing.".into());
        }
        let dir = match tool {
            ToolId::YtDlp => self.data_dir.join("tools"),
            ToolId::Ffmpeg => self.data_dir.join("tools").join("ffmpeg"),
        };
        let app = self.app.clone();
        let progress = move |p: tools_install::Progress| {
            let _ = app.emit("tool-install-progress", p);
        };
        let installed = match tools_install::official_asset(tool).await {
            Ok(asset) => tools_install::install(&self.client, tool, &asset, &dir, &progress).await,
            Err(e) => Err(e),
        };
        self.installing
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&tool);
        let path = Some(installed?.display().to_string());

        let mut settings = self.settings();
        match tool {
            ToolId::YtDlp => settings.yt_dlp_path = path,
            ToolId::Ffmpeg => settings.ffmpeg_path = path,
        }
        self.save_settings(settings).await?;
        // An update keeps the same path, so the version must be read again.
        Ok(self.detect_tools().await)
    }

    pub async fn save_settings(self: &Arc<Self>, mut new: Settings) -> CmdResult<Settings> {
        new.ffmpeg_path = non_empty(new.ffmpeg_path);
        new.yt_dlp_path = non_empty(new.yt_dlp_path);
        new.download_folder = new.download_folder.trim().to_string();
        new.validate()?;
        store::save_settings(&self.data_dir, &new)
            .map_err(|e| format!("Siphon could not save the settings: {e}."))?;
        let (paths_changed, titles_changed) = {
            let mut s = self.lock();
            let changed = s.settings.ffmpeg_path != new.ffmpeg_path
                || s.settings.yt_dlp_path != new.yt_dlp_path;
            let titles_changed = s.settings.english_titles != new.english_titles;
            s.settings = new.clone();
            (changed, titles_changed)
        };
        if titles_changed {
            // The prefetch asked yt-dlp with the old setting.
            self.cancel_prefetch();
        }
        if paths_changed {
            self.detect_tools().await;
        } else {
            self.schedule();
        }
        Ok(new)
    }

    // ---- resolving and adding ----

    /// Starts resolving a copied link in the background, so the Add link dialog
    /// opens without a wait. Only for video sites (some other links work only once)
    /// and only when yt-dlp is found. A new copy stops the previous prefetch.
    pub fn prefetch(self: &Arc<Self>, url: &str) {
        self.cancel_prefetch();
        let enabled = {
            let s = self.lock();
            s.settings.smart_prefetch && s.tools.yt_dlp.found
        };
        if !enabled || !resolve::is_video_page(url) {
            return;
        }
        let core = Arc::clone(self);
        let owned = url.to_string();
        let result = async move { core.resolve_now(&owned, None, None).await }
            .boxed()
            .shared();
        let task = tauri::async_runtime::spawn(result.clone().map(|_| ()));
        *self.prefetch_slot() = Some(Prefetch {
            url: url.to_string(),
            at: Instant::now(),
            result,
            task,
        });
    }

    /// Stops the background resolve. Dropping its future kills yt-dlp.
    pub fn cancel_prefetch(&self) {
        if let Some(p) = self.prefetch_slot().take() {
            p.task.abort();
        }
    }

    fn prefetch_slot(&self) -> MutexGuard<'_, Option<Prefetch>> {
        self.prefetch.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub async fn resolve_link(
        &self,
        url: &str,
        referer: Option<String>,
        user_agent: Option<String>,
    ) -> CmdResult<ResolveResult> {
        let url = resolve::validate_url(url)?;
        let referer = non_empty(referer);
        let user_agent = non_empty(user_agent);
        // The prefetch ran without headers, so it fits only a request without them.
        if referer.is_none() && user_agent.is_none() {
            let prefetched = self
                .prefetch_slot()
                .take_if(|p| p.url == url && p.at.elapsed() < RESOLVE_TTL)
                .map(|p| p.result);
            // A failed prefetch can be temporary. Then resolve again.
            if let Some(Ok(result)) = match prefetched {
                Some(f) => Some(f.await),
                None => None,
            } {
                return Ok(result);
            }
        }
        self.resolve_now(&url, referer, user_agent).await
    }

    async fn resolve_now(
        &self,
        url: &str,
        referer: Option<String>,
        user_agent: Option<String>,
    ) -> CmdResult<ResolveResult> {
        let url = url.to_string();
        let (yt_dlp, folder, english_titles) = {
            let s = self.lock();
            let yt = s
                .tools
                .yt_dlp
                .found
                .then(|| s.tools.yt_dlp.path.clone().map(PathBuf::from))
                .flatten();
            (
                yt,
                s.settings.download_folder.clone(),
                s.settings.english_titles,
            )
        };
        let mut resolved = resolve::resolve(
            &self.client,
            &url,
            referer.as_deref(),
            user_agent.as_deref(),
            yt_dlp,
            english_titles,
        )
        .await;
        resolved.result.free_space_bytes = free_space(Path::new(&folder));
        let mut cache = self.resolves.lock().unwrap_or_else(|p| p.into_inner());
        cache.retain(|_, c| c.at.elapsed() < RESOLVE_TTL);
        cache.insert(
            resolved.result.resolve_id.clone(),
            CachedResolve {
                at: Instant::now(),
                url: url.clone(),
                choices: resolved.choices,
            },
        );
        Ok(resolved.result)
    }

    pub fn add_download(self: &Arc<Self>, req: AddRequest) -> CmdResult<Download> {
        let (mut choice, source_url) = {
            let cache = self.resolves.lock().unwrap_or_else(|p| p.into_inner());
            let cached = cache
                .get(&req.resolve_id)
                .ok_or("The link result expired. Fetch the link again.")?;
            let choice = cached
                .choices
                .get(&(req.stream_id.clone(), req.quality_id.clone()))
                .cloned()
                .ok_or("Choose a stream and a quality.")?;
            (choice, cached.url.clone())
        };
        if let Some(subtitle_id) = &req.subtitle_id {
            let source = choice
                .subtitles
                .iter()
                .find(|(id, _)| id == subtitle_id)
                .map(|(_, s)| s.clone())
                .ok_or("This quality cannot have subtitles. Choose None or another quality.")?;
            choice.spec.subtitle = Some(source);
        }
        if req.compatible_format
            && let Some(convert) = choice.convert.take()
        {
            choice.spec.convert = Some(convert);
            choice.output_ext = Some("mp4".into());
        }
        let folder = req.folder.trim();
        if folder.is_empty() {
            return Err("Choose a folder.".into());
        }
        let mut name = util::sanitize_file_name(&req.file_name);
        if let Some(ext) = &choice.output_ext
            && !util::split_ext(&name)
                .1
                .eq_ignore_ascii_case(&format!(".{ext}"))
        {
            name = util::with_extension(&name, ext);
        }

        let view = {
            let mut s = self.lock();
            let taken: Vec<PathBuf> = s
                .entries
                .iter()
                .filter(|e| !e.removed)
                .map(|e| final_path(&e.stored.view))
                .collect();
            let path =
                util::unique_path(Path::new(folder), &name, &|p| taken.iter().any(|t| t == p));
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&name)
                .to_string();
            let sources = choice.spec.sources().len();
            let mut stored = StoredDownload {
                view: Download {
                    id: uuid::Uuid::new_v4().to_string(),
                    host: util::host_of(&source_url),
                    source_url,
                    file_name,
                    folder: folder.to_string(),
                    kind: choice.kind,
                    quality_label: choice.quality_label.clone(),
                    status: DownloadStatus::Queued,
                    total_bytes: None,
                    total_is_estimate: false,
                    done_bytes: 0,
                    speed: 0,
                    eta_seconds: None,
                    merge_progress: None,
                    detail: ProgressDetail::None,
                    error: None,
                    referer: non_empty(req.referer),
                    user_agent: non_empty(req.user_agent),
                    created_at: util::now_ms(),
                    finished_at: None,
                },
                spec: choice.spec,
                parts: vec![None; sources],
            };
            stored.refresh_view();
            let view = stored.view.clone();
            s.entries.push(Entry {
                stored,
                run: None,
                removed: false,
                dirty: true,
                meter: None,
            });
            s.save_pending = true;
            view
        };
        self.save_now();
        self.flush(false);
        self.schedule();
        Ok(view)
    }

    pub async fn add_links(self: &Arc<Self>, urls: Vec<String>) -> AddLinksResult {
        let mut added = Vec::new();
        let mut failed = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let settings = self.settings();
        let folder = settings.download_folder;
        for raw in urls {
            let raw = raw.trim().to_string();
            if raw.is_empty() || !seen.insert(raw.clone()) {
                continue;
            }
            let result = match self.resolve_link(&raw, None, None).await {
                Ok(r) => r,
                Err(message) => {
                    failed.push(FailedLink { url: raw, message });
                    continue;
                }
            };
            let Some(stream) = result.streams.first() else {
                let reason = result
                    .steps
                    .iter()
                    .rev()
                    .find(|s| {
                        matches!(
                            s.state,
                            ResolveStepState::Error | ResolveStepState::NotFound
                        )
                    })
                    .map(|s| s.message.clone())
                    .unwrap_or_else(|| "Siphon found no video or file.".into());
                failed.push(FailedLink {
                    url: raw,
                    message: reason,
                });
                continue;
            };
            let req = AddRequest {
                resolve_id: result.resolve_id.clone(),
                stream_id: stream.id.clone(),
                quality_id: stream.qualities[0].id.clone(),
                file_name: stream.suggested_file_name.clone(),
                folder: folder.clone(),
                referer: None,
                user_agent: None,
                subtitle_id: None,
                compatible_format: settings.compatible_format,
            };
            match self.add_download(req) {
                Ok(view) => added.push(view),
                Err(message) => failed.push(FailedLink { url: raw, message }),
            }
        }
        AddLinksResult { added, failed }
    }

    // ---- queue actions ----

    fn with_entry<R>(
        &self,
        id: &str,
        f: impl FnOnce(&mut Entry, &ToolsStatus) -> CmdResult<R>,
    ) -> CmdResult<R> {
        let mut s = self.lock();
        let State { entries, tools, .. } = &mut *s;
        let entry = entries
            .iter_mut()
            .find(|e| e.id() == id && !e.removed)
            .ok_or("This download is not in the list anymore.")?;
        let r = f(entry, tools)?;
        entry.dirty = true;
        s.save_pending = true;
        Ok(r)
    }

    fn after_change(self: &Arc<Self>) {
        self.save_now();
        self.flush(false);
        self.schedule();
    }

    pub fn pause(self: &Arc<Self>, id: &str) -> CmdResult<()> {
        self.with_entry(id, |e, _| {
            if let Some(run) = &mut e.run {
                run.stop = Some(StopReason::Pause);
                run.cancel.cancel();
                return Ok(());
            }
            match e.stored.view.status {
                DownloadStatus::Queued | DownloadStatus::NeedsFfmpeg => {
                    e.stored.view.status = DownloadStatus::Paused;
                    Ok(())
                }
                _ => Err("Only a waiting or running download can pause.".into()),
            }
        })?;
        self.after_change();
        Ok(())
    }

    pub fn resume(self: &Arc<Self>, id: &str) -> CmdResult<()> {
        self.with_entry(id, |e, _| match e.stored.view.status {
            DownloadStatus::Paused | DownloadStatus::NeedsFfmpeg => {
                e.stored.view.status = DownloadStatus::Queued;
                Ok(())
            }
            _ => Err("Only a paused download can resume.".into()),
        })?;
        self.after_change();
        Ok(())
    }

    pub fn retry(self: &Arc<Self>, id: &str) -> CmdResult<()> {
        self.with_entry(id, |e, _| match e.stored.view.status {
            DownloadStatus::Failed => {
                e.stored.view.status = DownloadStatus::Queued;
                e.stored.view.error = None;
                Ok(())
            }
            _ => Err("Only a failed download can retry.".into()),
        })?;
        self.after_change();
        Ok(())
    }

    /// Starts a waiting download now, also when all slots are busy.
    pub fn start_now(self: &Arc<Self>, id: &str) -> CmdResult<()> {
        let token = self.with_entry(id, |e, tools| {
            if e.run.is_some() {
                return Err("This download is already running.".into());
            }
            if !matches!(
                e.stored.view.status,
                DownloadStatus::Queued | DownloadStatus::Paused | DownloadStatus::NeedsFfmpeg
            ) {
                return Err("Only a waiting or paused download can start.".into());
            }
            Ok(Self::begin(e, tools))
        })?;
        if let Some(token) = token {
            self.spawn_job(id.to_string(), token);
        }
        self.save_now();
        self.flush(false);
        Ok(())
    }

    pub fn cancel(self: &Arc<Self>, id: &str) -> CmdResult<()> {
        let cleanup = self.with_entry(id, |e, _| {
            if let Some(run) = &mut e.run {
                run.stop = Some(StopReason::Cancel);
                run.cancel.cancel();
                return Ok(None);
            }
            if e.stored.view.status == DownloadStatus::Done {
                return Err("This download is complete. Use Remove instead.".into());
            }
            e.stored.view.status = DownloadStatus::Failed;
            e.stored.view.error = Some(cancelled_error());
            e.stored.parts.iter_mut().for_each(|p| *p = None);
            e.stored.refresh_view();
            Ok(Some(e.stored.view.clone()))
        })?;
        if let Some(view) = cleanup {
            remove_temp_files(&view);
        }
        self.after_change();
        Ok(())
    }

    pub fn remove(self: &Arc<Self>, id: &str, delete_file: bool) -> CmdResult<()> {
        let removed_view = {
            let mut s = self.lock();
            let pos = s
                .entries
                .iter()
                .position(|e| e.id() == id && !e.removed)
                .ok_or("This download is not in the list anymore.")?;
            let entry = &mut s.entries[pos];
            if let Some(run) = &mut entry.run {
                run.stop = Some(StopReason::Remove { delete_file });
                run.cancel.cancel();
                entry.removed = true;
                None
            } else {
                let e = s.entries.remove(pos);
                s.save_pending = true;
                Some(e.stored.view)
            }
        };
        if let Some(view) = removed_view {
            remove_temp_files(&view);
            if delete_file && view.status == DownloadStatus::Done {
                std::fs::remove_file(final_path(&view)).map_err(|e| {
                    format!("The download is removed, but Siphon could not delete the file: {e}.")
                })?;
            }
        }
        let _ = self
            .app
            .emit("download-removed", serde_json::json!({ "id": id }));
        self.after_change();
        Ok(())
    }

    pub fn update_options(
        self: &Arc<Self>,
        id: &str,
        referer: Option<String>,
        user_agent: Option<String>,
    ) -> CmdResult<()> {
        self.with_entry(id, |e, _| {
            e.stored.view.referer = non_empty(referer);
            e.stored.view.user_agent = non_empty(user_agent);
            Ok(())
        })?;
        self.after_change();
        Ok(())
    }

    pub fn file_path(&self, id: &str) -> CmdResult<(PathBuf, bool)> {
        let s = self.lock();
        let e = s
            .entries
            .iter()
            .find(|e| e.id() == id && !e.removed)
            .ok_or("This download is not in the list anymore.")?;
        let done = e.stored.view.status == DownloadStatus::Done;
        Ok(if done {
            (final_path(&e.stored.view), true)
        } else {
            (PathBuf::from(&e.stored.view.folder), false)
        })
    }

    // ---- scheduling and running ----

    /// Marks an entry as running. Returns None when the job needs ffmpeg and ffmpeg is missing.
    fn begin(e: &mut Entry, tools: &ToolsStatus) -> Option<CancellationToken> {
        if e.stored.spec.needs_ffmpeg() && !tools.ffmpeg.found {
            e.stored.view.status = DownloadStatus::NeedsFfmpeg;
            e.dirty = true;
            return None;
        }
        let token = CancellationToken::new();
        e.run = Some(Run {
            cancel: token.clone(),
            stop: None,
        });
        e.stored.view.status = DownloadStatus::Downloading;
        e.stored.view.error = None;
        e.meter = None;
        e.dirty = true;
        Some(token)
    }

    pub fn schedule(self: &Arc<Self>) {
        let to_start = {
            let mut s = self.lock();
            let State {
                entries,
                tools,
                settings,
                ..
            } = &mut *s;
            if tools.ffmpeg.found {
                for e in entries
                    .iter_mut()
                    .filter(|e| e.stored.view.status == DownloadStatus::NeedsFfmpeg)
                {
                    e.stored.view.status = DownloadStatus::Queued;
                    e.dirty = true;
                }
            }
            let running = entries.iter().filter(|e| e.run.is_some()).count();
            let mut free = (settings.max_downloads as usize).saturating_sub(running);
            let mut to_start = Vec::new();
            for e in entries.iter_mut() {
                if free == 0 {
                    break;
                }
                if e.removed || e.run.is_some() || e.stored.view.status != DownloadStatus::Queued {
                    continue;
                }
                if let Some(token) = Self::begin(e, tools) {
                    to_start.push((e.id().to_string(), token));
                    free -= 1;
                }
            }
            to_start
        };
        let started = !to_start.is_empty();
        for (id, token) in to_start {
            self.spawn_job(id, token);
        }
        if started {
            self.save_now();
        }
        self.flush(false);
    }

    fn spawn_job(self: &Arc<Self>, id: String, token: CancellationToken) {
        let core = self.clone();
        tauri::async_runtime::spawn(async move {
            let Some(ctx) = core.job_context(&id) else {
                return;
            };
            let result = core.execute(&id, &ctx, &token).await;
            core.finish(&id, result);
        });
    }

    fn job_context(&self, id: &str) -> Option<JobContext> {
        let s = self.lock();
        let e = s.entries.iter().find(|e| e.id() == id)?;
        Some(JobContext {
            spec: e.stored.spec.clone(),
            folder: PathBuf::from(&e.stored.view.folder),
            file_name: e.stored.view.file_name.clone(),
            referer: e.stored.view.referer.clone(),
            user_agent: e.stored.view.user_agent.clone(),
            connections: s.settings.connections_per_file,
            ffmpeg: s
                .tools
                .ffmpeg
                .found
                .then(|| s.tools.ffmpeg.path.clone().map(PathBuf::from))
                .flatten(),
        })
    }

    fn sink(self: &Arc<Self>, id: &str, idx: usize) -> Arc<dyn PartSink> {
        Arc::new(CoreSink {
            core: self.clone(),
            id: id.to_string(),
            idx,
        })
    }

    /// Returns the final file name. It changes when another file took the name meanwhile.
    async fn execute(
        self: &Arc<Self>,
        id: &str,
        ctx: &JobContext,
        cancel: &CancellationToken,
    ) -> Result<String, DlError> {
        tokio::fs::create_dir_all(&ctx.folder)
            .await
            .map_err(|e| DlError::io(&e))?;
        let final_path = ctx.folder.join(&ctx.file_name);
        let headers = |src: &MediaSource| {
            net::request_headers(
                &src.headers,
                ctx.referer.as_deref(),
                ctx.user_agent.as_deref(),
            )
        };
        let primary = &ctx.spec.primary;

        if !ctx.spec.needs_ffmpeg() {
            let part = part_path(&final_path);
            engine::http::download(
                &self.client,
                &primary.url,
                &headers(primary),
                &part,
                ctx.connections,
                self.sink(id, 0),
                cancel,
            )
            .await?;
            return self.place(&part, &final_path, id).await;
        }

        let work = {
            let s = self.lock();
            let e = s
                .entries
                .iter()
                .find(|e| e.id() == id)
                .ok_or_else(DlError::cancelled)?;
            work_dir(&e.stored.view)
        };
        let (video, audio) = match &ctx.spec.audio {
            Some(a) => {
                // Two sources share the connection budget.
                let half = (ctx.connections / 2).max(1);
                let (video_base, audio_base) = (work.join("video"), work.join("audio"));
                let video_task =
                    self.fetch_source(id, 0, primary, &video_base, headers(primary), half, cancel);
                let audio_task = self.fetch_source(id, 1, a, &audio_base, headers(a), half, cancel);
                let (v, a) = tokio::try_join!(video_task, audio_task)?;
                (v, Some(a))
            }
            None => (
                self.fetch_source(
                    id,
                    0,
                    primary,
                    &work.join("video"),
                    headers(primary),
                    ctx.connections,
                    cancel,
                )
                .await?,
                None,
            ),
        };

        let subtitle = match &ctx.spec.subtitle {
            Some(sub) => {
                let path = work.join(format!("subtitle.{}", sub.ext));
                let req_headers = net::request_headers(
                    &sub.headers,
                    ctx.referer.as_deref(),
                    ctx.user_agent.as_deref(),
                );
                fetch_subtitle(&self.client, sub, req_headers, &path, cancel).await?;
                Some((path, sub))
            }
            None => None,
        };

        let ffmpeg = ctx.ffmpeg.clone().ok_or_else(|| {
            DlError::fatal(
                "ffmpeg is missing.",
                Some("Set the ffmpeg path in Settings, then retry."),
            )
        })?;
        let recode = match &ctx.spec.convert {
            Some(c) => Some(tools::Recode {
                video: match c.video {
                    true => Some(tools::VideoEncode {
                        encoder: tools::h264_encoder(&ffmpeg, cancel).await?,
                        bitrate: h264_bitrate(c.source_bitrate, c.height),
                    }),
                    false => None,
                },
                audio: c.audio,
            }),
            None => None,
        };
        self.set_merging(
            id,
            match recode {
                Some(_) => DownloadStatus::Converting,
                None => DownloadStatus::Merging,
            },
        );
        let tmp = temp_output(&final_path);
        let args = ffmpeg_args(
            &video,
            audio.as_deref(),
            subtitle.as_ref().map(|(p, s)| (p.as_path(), *s)),
            recode.as_ref(),
            &tmp,
        );
        let progress_core = self.clone();
        let progress_id = id.to_string();
        let merged = tools::run_ffmpeg(&ffmpeg, &args, ctx.spec.duration, cancel, move |p| {
            progress_core.set_merge_progress(&progress_id, p)
        })
        .await;
        let checked = match merged {
            Ok(()) => tools::check_output(&ffmpeg, &tmp, cancel).await,
            Err(e) => Err(e),
        };
        if let Err(e) = checked {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(e);
        }
        let name = self.place(&tmp, &final_path, id).await?;
        let _ = tokio::fs::remove_dir_all(&work).await;
        remove_empty_dir(work.parent());
        Ok(name)
    }

    #[allow(clippy::too_many_arguments)]
    async fn fetch_source(
        self: &Arc<Self>,
        id: &str,
        idx: usize,
        src: &MediaSource,
        base: &Path,
        headers: reqwest::header::HeaderMap,
        connections: u32,
        cancel: &CancellationToken,
    ) -> Result<PathBuf, DlError> {
        match src.kind {
            SourceKind::Http => {
                let path = base.with_extension("part");
                engine::http::download(
                    &self.client,
                    &src.url,
                    &headers,
                    &path,
                    connections,
                    self.sink(id, idx),
                    cancel,
                )
                .await?;
                Ok(path)
            }
            SourceKind::Hls => {
                let out = engine::hls::download(
                    &self.client,
                    &src.url,
                    &headers,
                    base,
                    connections,
                    self.sink(id, idx),
                    cancel,
                )
                .await?;
                Ok(out.joined)
            }
        }
    }

    /// Moves a finished file to its final name, or to a free name when that one is taken.
    async fn place(&self, from: &Path, wanted: &Path, id: &str) -> Result<String, DlError> {
        let target = if wanted.exists() {
            let folder = wanted.parent().unwrap_or(Path::new("."));
            let name = wanted
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("download");
            let taken: Vec<PathBuf> = {
                let s = self.lock();
                s.entries
                    .iter()
                    .filter(|e| e.id() != id && !e.removed)
                    .map(|e| final_path(&e.stored.view))
                    .collect()
            };
            util::unique_path(folder, name, &|p| taken.iter().any(|t| t == p))
        } else {
            wanted.to_path_buf()
        };
        tokio::fs::rename(from, &target)
            .await
            .map_err(|e| DlError::io(&e))?;
        Ok(target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string())
    }

    /// `status` is `Merging`, or `Converting` when ffmpeg changes the format.
    fn set_merging(&self, id: &str, status: DownloadStatus) {
        let mut s = self.lock();
        if let Some(e) = s.entries.iter_mut().find(|e| e.id() == id) {
            e.stored.view.status = status;
            e.stored.view.speed = 0;
            e.stored.view.eta_seconds = None;
            e.stored.view.merge_progress = Some(0.0);
            e.dirty = true;
        }
    }

    fn set_merge_progress(&self, id: &str, p: f64) {
        let mut s = self.lock();
        if let Some(e) = s.entries.iter_mut().find(|e| e.id() == id) {
            e.stored.view.merge_progress = Some(p);
            e.dirty = true;
        }
    }

    fn finish(self: &Arc<Self>, id: &str, result: Result<String, DlError>) {
        let mut cleanup: Option<(Download, bool)> = None;
        {
            let mut s = self.lock();
            let Some(pos) = s.entries.iter().position(|e| e.id() == id) else {
                return;
            };
            let entry = &mut s.entries[pos];
            let stop = entry.run.take().and_then(|r| r.stop);
            entry.meter = None;
            let v = &mut entry.stored.view;
            v.speed = 0;
            v.eta_seconds = None;
            v.merge_progress = None;
            match stop {
                Some(StopReason::Pause) => v.status = DownloadStatus::Paused,
                Some(StopReason::Cancel) => {
                    v.status = DownloadStatus::Failed;
                    v.error = Some(cancelled_error());
                    cleanup = Some((v.clone(), false));
                    entry.stored.parts.iter_mut().for_each(|p| *p = None);
                }
                Some(StopReason::Remove { delete_file }) => {
                    let e = s.entries.remove(pos);
                    cleanup = Some((e.stored.view, delete_file));
                }
                None => match result {
                    Ok(name) => {
                        v.file_name = name;
                        v.status = DownloadStatus::Done;
                        v.finished_at = Some(util::now_ms());
                        v.error = None;
                    }
                    Err(e) if e.message == DlError::cancelled().message => {
                        v.status = DownloadStatus::Paused
                    }
                    Err(e) => {
                        v.status = DownloadStatus::Failed;
                        v.error = Some(e.into_view());
                    }
                },
            }
            if let Some(entry) = s.entries.get_mut(pos).filter(|e| e.id() == id) {
                entry.stored.refresh_view();
                entry.dirty = true;
            }
            s.save_pending = true;
        }
        if let Some((view, delete_final)) = cleanup {
            remove_temp_files(&view);
            if delete_final && view.status == DownloadStatus::Done {
                let _ = std::fs::remove_file(final_path(&view));
            }
        }
        self.after_change();
    }

    // ---- events and persistence ----

    /// Sends changed downloads to the UI. With `sample_speed`, also updates speed and time left.
    pub fn flush(&self, sample_speed: bool) {
        let mut changed = Vec::new();
        let mut snapshot = None;
        {
            let mut s = self.lock();
            let now = Instant::now();
            let mut running = false;
            for e in s.entries.iter_mut().filter(|e| !e.removed) {
                if e.dirty {
                    e.stored.refresh_view();
                }
                if sample_speed
                    && e.run.is_some()
                    && e.stored.view.status == DownloadStatus::Downloading
                {
                    running = true;
                    let done = e.stored.view.done_bytes;
                    let meter = e.meter.get_or_insert(SpeedMeter {
                        last_bytes: done,
                        last_at: now,
                        ema: 0.0,
                    });
                    let speed = meter.sample(done, now);
                    let eta = e
                        .stored
                        .view
                        .total_bytes
                        .filter(|_| speed > 0)
                        .map(|t| t.saturating_sub(done) / speed.max(1));
                    if speed != e.stored.view.speed || eta != e.stored.view.eta_seconds {
                        e.stored.view.speed = speed;
                        e.stored.view.eta_seconds = eta;
                        e.dirty = true;
                    }
                }
                if e.dirty {
                    e.dirty = false;
                    changed.push(e.stored.view.clone());
                }
            }
            if running {
                s.save_pending = true;
            }
            if s.save_pending && s.last_save.elapsed() >= SAVE_INTERVAL {
                s.save_pending = false;
                s.last_save = now;
                snapshot = Some(Self::snapshot(&s));
            }
        }
        for view in changed {
            let _ = self.app.emit("download-updated", view);
        }
        if let Some(list) = snapshot {
            self.write_downloads(&list);
        }
    }

    fn snapshot(s: &State) -> Vec<StoredDownload> {
        s.entries
            .iter()
            .filter(|e| !e.removed)
            .map(|e| e.stored.clone())
            .collect()
    }

    pub fn save_now(&self) {
        let list = {
            let mut s = self.lock();
            s.save_pending = false;
            s.last_save = Instant::now();
            Self::snapshot(&s)
        };
        self.write_downloads(&list);
    }

    fn write_downloads(&self, list: &[StoredDownload]) {
        if let Err(e) = store::save_downloads(&self.data_dir, list) {
            eprintln!("siphon: could not save the download list: {e}");
        }
    }

    pub fn start_ticker(self: &Arc<Self>) {
        let core = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            loop {
                tick.tick().await;
                core.flush(true);
            }
        });
    }
}

struct CoreSink {
    core: Arc<Core>,
    id: String,
    idx: usize,
}

impl PartSink for CoreSink {
    fn snapshot(&self) -> Option<PartState> {
        let s = self.core.lock();
        s.entries
            .iter()
            .find(|e| e.id() == self.id)
            .and_then(|e| e.stored.parts.get(self.idx).cloned().flatten())
    }

    fn update(&self, f: &mut dyn FnMut(&mut Option<PartState>)) {
        let mut s = self.core.lock();
        if let Some(e) = s.entries.iter_mut().find(|e| e.id() == self.id)
            && let Some(part) = e.stored.parts.get_mut(self.idx)
        {
            f(part);
            e.dirty = true;
        }
    }
}

fn remove_temp_files(view: &Download) {
    let _ = std::fs::remove_file(part_path(&final_path(view)));
    let _ = std::fs::remove_file(temp_output(&final_path(view)));
    let work = work_dir(view);
    let _ = std::fs::remove_dir_all(&work);
    remove_empty_dir(work.parent());
}

fn remove_empty_dir(dir: Option<&Path>) {
    if let Some(d) = dir {
        // Fails when the folder still has other downloads in it. That is intended.
        let _ = std::fs::remove_dir(d);
    }
}

/// Free space on the disk of `folder`, or of its nearest existing parent.
pub fn free_space(folder: &Path) -> Option<u64> {
    let mut p = Some(folder);
    while let Some(dir) = p {
        if dir.exists() {
            return fs4::available_space(dir).ok();
        }
        p = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_names_keep_the_extension() {
        assert_eq!(
            temp_output(Path::new("D:/v/clip.mp4")),
            PathBuf::from("D:/v/clip.siphon-tmp.mp4")
        );
        assert_eq!(
            part_path(Path::new("D:/v/a.zip")),
            PathBuf::from("D:/v/a.zip.part")
        );
    }

    #[test]
    fn speed_meter_smooths() {
        let t0 = Instant::now();
        let mut m = SpeedMeter {
            last_bytes: 0,
            last_at: t0,
            ema: 0.0,
        };
        assert_eq!(m.sample(1000, t0 + Duration::from_secs(1)), 1000);
        let s = m.sample(4000, t0 + Duration::from_secs(2));
        assert!(s > 1000 && s < 3000);
    }

    #[test]
    fn free_space_walks_up_to_an_existing_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_space(&dir.path().join("not/yet/created")).is_some());
    }

    async fn ffmpeg_make(ffmpeg: &Path, args: &[&str]) -> bool {
        tools::command(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(args)
            .output()
            .await
            .is_ok_and(|o| o.status.success())
    }

    /// Uses the real ffmpeg on this PC. Skips when ffmpeg is not installed.
    #[tokio::test]
    async fn hls_download_and_remux_with_real_ffmpeg() {
        use crate::engine::testing::MemorySink;
        use crate::test_server::{TestServer, TestServerOptions};

        let Ok(ffmpeg) = which::which("ffmpeg") else {
            eprintln!("ffmpeg not found: test skipped");
            return;
        };
        let src = tempfile::tempdir().unwrap();
        let playlist = src.path().join("index.m3u8");
        let seg_pattern = src.path().join("seg%d.ts");
        let made = ffmpeg_make(
            &ffmpeg,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=4:size=160x120:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=4",
                "-c:v",
                "libx264",
                "-c:a",
                "aac",
                "-shortest",
                "-f",
                "hls",
                "-hls_time",
                "1",
                "-hls_playlist_type",
                "vod",
                "-hls_segment_filename",
                seg_pattern.to_str().unwrap(),
                playlist.to_str().unwrap(),
            ],
        )
        .await;
        assert!(made, "ffmpeg could not make the test stream");

        let mut routes = Vec::new();
        let names: Vec<String> = std::fs::read_dir(src.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        let files: Vec<(String, Vec<u8>)> = names
            .iter()
            .map(|n| (format!("/{n}"), std::fs::read(src.path().join(n)).unwrap()))
            .collect();
        for (path, body) in &files {
            routes.push((path.as_str(), "application/octet-stream", body.clone()));
        }
        let server = TestServer::start_routes(routes, TestServerOptions::default()).await;

        let work = tempfile::tempdir().unwrap();
        let out = crate::engine::hls::download(
            &net::build_client(),
            &server.url("/index.m3u8"),
            &reqwest::header::HeaderMap::new(),
            work.path(),
            4,
            Arc::new(MemorySink::default()),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        let target = work.path().join("clip.siphon-tmp.mp4");
        let mut last = 0.0;
        tools::run_ffmpeg(
            &ffmpeg,
            &ffmpeg_args(&out.joined, None, None, None, &target),
            Some(4.0),
            &CancellationToken::new(),
            |p| last = p,
        )
        .await
        .unwrap();
        assert!(std::fs::metadata(&target).unwrap().len() > 1000);
        assert!(last > 0.5, "progress should reach the end, got {last}");
    }

    /// Uses the real ffmpeg on this PC. Skips when ffmpeg is not installed.
    #[tokio::test]
    async fn output_check_rejects_a_broken_video_track() {
        let Ok(ffmpeg) = which::which("ffmpeg") else {
            eprintln!("ffmpeg not found: test skipped");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clip.mp4");
        let made = ffmpeg_make(
            &ffmpeg,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=6:size=320x240:rate=25",
                "-c:v",
                "libx264",
                "-movflags",
                "+faststart",
                file.to_str().unwrap(),
            ],
        )
        .await;
        assert!(made, "ffmpeg could not make the test file");
        tools::check_output(&ffmpeg, &file, &CancellationToken::new())
            .await
            .expect("the intact file passes");

        // The index stays at the start (+faststart). Junk replaces most of the frame data.
        let mut bytes = std::fs::read(&file).unwrap();
        let (from, to) = (bytes.len() / 4, bytes.len() * 19 / 20);
        for (i, b) in bytes[from..to].iter_mut().enumerate() {
            *b = (i * 7 + 13) as u8;
        }
        std::fs::write(&file, bytes).unwrap();
        let err = tools::check_output(&ffmpeg, &file, &CancellationToken::new())
            .await
            .unwrap_err();
        assert!(err.hint.unwrap().contains("Update"));
    }

    #[test]
    fn subtitle_input_comes_before_the_output_options() {
        let sub = SubtitleSource {
            url: String::new(),
            headers: Vec::new(),
            ext: "srt".into(),
            language: Some("eng".into()),
            label: "English (auto)".into(),
        };
        let args = ffmpeg_args(
            Path::new("v.part"),
            Some(Path::new("a.part")),
            Some((Path::new("subtitle.srt"), &sub)),
            None,
            Path::new("out.mp4"),
        );
        let last_input = args.iter().rposition(|a| a == "-i").unwrap();
        let first_map = args.iter().position(|a| a == "-map").unwrap();
        assert_eq!(args[last_input + 1], "subtitle.srt");
        assert!(last_input < first_map);
        assert!(args.contains(&"2:s:0".to_string()));
        assert!(args.contains(&"language=eng".to_string()));
        assert!(args.contains(&"title=English (auto)".to_string()));
    }

    /// Uses the real ffmpeg on this PC. Skips when ffmpeg is not installed.
    #[tokio::test]
    async fn adds_a_subtitle_track_with_real_ffmpeg() {
        let Ok(ffmpeg) = which::which("ffmpeg") else {
            eprintln!("ffmpeg not found: test skipped");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("video.part");
        let made = ffmpeg_make(
            &ffmpeg,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=160x120:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=2",
                "-c:v",
                "libx264",
                "-c:a",
                "aac",
                "-shortest",
                "-f",
                "mp4",
                video.to_str().unwrap(),
            ],
        )
        .await;
        assert!(made, "ffmpeg could not make the test input");
        let srt = dir.path().join("subtitle.srt");
        std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:01,500\nHello\n\n").unwrap();
        let sub = SubtitleSource {
            url: String::new(),
            headers: Vec::new(),
            ext: "srt".into(),
            language: Some("eng".into()),
            label: "English".into(),
        };
        for ext in ["mp4", "mkv"] {
            let out = dir.path().join(format!("out.siphon-tmp.{ext}"));
            tools::run_ffmpeg(
                &ffmpeg,
                &ffmpeg_args(&video, None, Some((&srt, &sub)), None, &out),
                Some(2.0),
                &CancellationToken::new(),
                |_| {},
            )
            .await
            .unwrap();
            tools::check_output(&ffmpeg, &out, &CancellationToken::new())
                .await
                .expect("the video track stays intact");
            let probe = tools::command(&ffmpeg)
                .args(["-hide_banner", "-i", out.to_str().unwrap()])
                .output()
                .await
                .unwrap();
            let info = String::from_utf8_lossy(&probe.stderr);
            let codec = if ext == "mp4" { "mov_text" } else { "subrip" };
            assert!(
                info.contains("Video: h264")
                    && info.contains("Audio: aac")
                    && info.contains(&format!("(eng): Subtitle: {codec}")),
                "{ext} must have video, audio and the subtitle: {info}"
            );
        }
    }

    /// Uses the real ffmpeg on this PC. Skips when ffmpeg is not installed.
    #[tokio::test]
    async fn merges_separate_video_and_audio_with_real_ffmpeg() {
        let Ok(ffmpeg) = which::which("ffmpeg") else {
            eprintln!("ffmpeg not found: test skipped");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        // yt-dlp sources download as files without an extension, so the inputs have none.
        let video = dir.path().join("video.part");
        let audio = dir.path().join("audio.part");
        let v = ffmpeg_make(
            &ffmpeg,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=160x120:rate=10",
                "-c:v",
                "libx264",
                "-f",
                "mp4",
                video.to_str().unwrap(),
            ],
        )
        .await;
        let a = ffmpeg_make(
            &ffmpeg,
            &[
                "-f",
                "lavfi",
                "-i",
                "sine=duration=2",
                "-c:a",
                "aac",
                "-f",
                "mp4",
                audio.to_str().unwrap(),
            ],
        )
        .await;
        assert!(v && a, "ffmpeg could not make the test inputs");

        let out = dir.path().join("merged.siphon-tmp.mp4");
        tools::run_ffmpeg(
            &ffmpeg,
            &ffmpeg_args(&video, Some(&audio), None, None, &out),
            Some(2.0),
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap();
        tools::check_output(&ffmpeg, &out, &CancellationToken::new())
            .await
            .expect("a correct merge passes the output check");
        let probe = tools::command(&ffmpeg)
            .args(["-hide_banner", "-i", out.to_str().unwrap()])
            .output()
            .await
            .unwrap();
        let info = String::from_utf8_lossy(&probe.stderr);
        assert!(
            info.contains("Video: h264") && info.contains("Audio: aac"),
            "merged file must have both streams: {info}"
        );
    }

    #[test]
    fn h264_bitrate_doubles_the_source_within_limits() {
        // Instagram VP9 1078p at 931 kbps: twice is below the 1080p minimum.
        assert_eq!(h264_bitrate(Some(931_105), Some(1078)), 2_500_000);
        assert_eq!(h264_bitrate(Some(3_000_000), Some(1080)), 6_000_000);
        assert_eq!(h264_bitrate(Some(40_000_000), Some(2160)), 20_000_000);
        assert_eq!(h264_bitrate(None, Some(720)), 3_000_000);
        // A missing or zero height counts as 1080p.
        assert_eq!(h264_bitrate(None, Some(0)), 5_000_000);
        assert_eq!(h264_bitrate(Some(0), None), 5_000_000);
    }

    /// Uses the real ffmpeg on this PC. Skips when ffmpeg is not installed or has no VP9 encoder.
    #[tokio::test]
    async fn converts_vp9_and_opus_to_h264_and_aac_with_real_ffmpeg() {
        let Ok(ffmpeg) = which::which("ffmpeg") else {
            eprintln!("ffmpeg not found: test skipped");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("video.part");
        let audio = dir.path().join("audio.part");
        let v = ffmpeg_make(
            &ffmpeg,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=320x240:rate=10",
                "-c:v",
                "libvpx-vp9",
                "-f",
                "webm",
                video.to_str().unwrap(),
            ],
        )
        .await;
        let a = ffmpeg_make(
            &ffmpeg,
            &[
                "-f",
                "lavfi",
                "-i",
                "sine=duration=2",
                "-c:a",
                "libopus",
                "-f",
                "webm",
                audio.to_str().unwrap(),
            ],
        )
        .await;
        if !(v && a) {
            eprintln!("ffmpeg has no VP9 or Opus encoder: test skipped");
            return;
        }

        let cancel = CancellationToken::new();
        let encoder = tools::h264_encoder(&ffmpeg, &cancel)
            .await
            .expect("this ffmpeg can make H.264");
        let recode = tools::Recode {
            video: Some(tools::VideoEncode {
                encoder,
                bitrate: h264_bitrate(None, Some(240)),
            }),
            audio: true,
        };
        let out = dir.path().join("converted.siphon-tmp.mp4");
        tools::run_ffmpeg(
            &ffmpeg,
            &ffmpeg_args(&video, Some(&audio), None, Some(&recode), &out),
            Some(2.0),
            &cancel,
            |_| {},
        )
        .await
        .unwrap();
        tools::check_output(&ffmpeg, &out, &cancel)
            .await
            .expect("a converted file passes the output check");
        let probe = tools::command(&ffmpeg)
            .args(["-hide_banner", "-i", out.to_str().unwrap()])
            .output()
            .await
            .unwrap();
        let info = String::from_utf8_lossy(&probe.stderr);
        assert!(
            info.contains("Video: h264") && info.contains("(avc1") && info.contains("Audio: aac"),
            "the converted file must be H.264 + AAC in MP4 ({encoder}): {info}"
        );
    }
}
