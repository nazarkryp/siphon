//! Tauri commands. Names match src/api/index.ts.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::core::{Core, free_space as disk_free_space};
use crate::model::{
    AddLinksResult, AddRequest, AppState, Download, ResolveResult, Settings, ToolUpdates,
    ToolsStatus,
};
use crate::tools_install::ToolId;

type Core_<'a> = State<'a, Arc<Core>>;
type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub fn get_state(core: Core_<'_>) -> AppState {
    core.app_state()
}

#[tauri::command]
pub async fn resolve_link(
    core: Core_<'_>,
    url: String,
    referer: Option<String>,
    user_agent: Option<String>,
) -> CmdResult<ResolveResult> {
    core.resolve_link(&url, referer, user_agent).await
}

#[tauri::command]
pub fn add_download(core: Core_<'_>, request: AddRequest) -> CmdResult<Download> {
    core.add_download(request)
}

#[tauri::command]
pub async fn add_links(core: Core_<'_>, urls: Vec<String>) -> CmdResult<AddLinksResult> {
    Ok(core.add_links(urls).await)
}

#[tauri::command]
pub fn pause_download(core: Core_<'_>, id: String) -> CmdResult<()> {
    core.pause(&id)
}

#[tauri::command]
pub fn resume_download(core: Core_<'_>, id: String) -> CmdResult<()> {
    core.resume(&id)
}

#[tauri::command]
pub fn start_now(core: Core_<'_>, id: String) -> CmdResult<()> {
    core.start_now(&id)
}

#[tauri::command]
pub fn retry_download(core: Core_<'_>, id: String) -> CmdResult<()> {
    core.retry(&id)
}

#[tauri::command]
pub fn cancel_download(core: Core_<'_>, id: String) -> CmdResult<()> {
    core.cancel(&id)
}

#[tauri::command]
pub fn remove_download(core: Core_<'_>, id: String, delete_file: bool) -> CmdResult<()> {
    core.remove(&id, delete_file)
}

#[tauri::command]
pub fn update_options(
    core: Core_<'_>,
    id: String,
    referer: Option<String>,
    user_agent: Option<String>,
) -> CmdResult<()> {
    core.update_options(&id, referer, user_agent)
}

#[tauri::command]
pub async fn save_settings(core: Core_<'_>, settings: Settings) -> CmdResult<Settings> {
    core.save_settings(settings).await
}

#[tauri::command]
pub async fn detect_tools(core: Core_<'_>) -> CmdResult<ToolsStatus> {
    Ok(core.detect_tools().await)
}

#[tauri::command]
pub async fn check_tool_updates(core: Core_<'_>) -> CmdResult<ToolUpdates> {
    Ok(core.check_tool_updates().await)
}

#[tauri::command]
pub async fn install_tool(core: Core_<'_>, tool: ToolId) -> CmdResult<ToolsStatus> {
    core.install_tool(tool).await
}

#[tauri::command]
pub async fn pick_folder(core: Core_<'_>, start: Option<String>) -> CmdResult<Option<String>> {
    let app = core.app().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file().set_title("Choose the download folder");
        if let Some(dir) = start.map(PathBuf::from).filter(|p| p.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        dialog
            .blocking_pick_folder()
            .map(|p| p.into_path().map(|p| p.display().to_string()))
            .transpose()
    })
    .await
    .map_err(|e| format!("The folder dialog failed: {e}."))?
    .map_err(|e| format!("The chosen folder is not a local folder: {e}."))
}

#[tauri::command]
pub fn free_space(folder: String) -> Option<u64> {
    disk_free_space(&PathBuf::from(folder))
}

#[tauri::command]
pub fn open_file(core: Core_<'_>, id: String) -> CmdResult<()> {
    let (path, is_file) = core.file_path(&id)?;
    if !is_file || !path.exists() {
        return Err("The file is not on the disk anymore.".into());
    }
    core.app()
        .opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| format!("Windows could not open the file: {e}."))
}

#[tauri::command]
pub fn show_in_folder(core: Core_<'_>, id: String) -> CmdResult<()> {
    let (path, is_file) = core.file_path(&id)?;
    let result = if is_file && path.exists() {
        core.app().opener().reveal_item_in_dir(&path)
    } else {
        core.app()
            .opener()
            .open_path(path.display().to_string(), None::<&str>)
    };
    result.map_err(|e| format!("Siphon could not open the folder: {e}."))
}
