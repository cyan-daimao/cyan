//! 浏览器面板命令：主窗口内嵌入子 WebView（原生渲染，与 codex WebContentsView 对等）。
//!
//! 面板挂载 → `browser_attach`（创建子 WebView 定位到容器区）；卸载 → `browser_detach`。
//! 导航/交互原生直通（WebView 即真实浏览器控件）；agent 经 `browser_navigate` /
//! `browser_eval` 控制同一视图。登录态经 `~/.cyan/browser-data` 持久化。

use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;
use tauri::Manager;

use crate::infra::browser::panel::{self, BrowserPanelState};

/// 挂载子 WebView 到主窗口指定区域（面板打开时调用）。
/// `rect` 为容器在窗口内的位置与尺寸（CSS px，前端实测）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAttachCmd {
    /// 宿主窗口 label（当前固定 main）
    pub window_label: String,
    /// 容器矩形：x/y/宽/高
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 默认主页（设置页偏好；仅新建视图时加载，重定位不影响当前页面）
    #[serde(default)]
    pub home_url: String,
}

#[tauri::command]
pub async fn browser_attach(
    app: tauri::AppHandle,
    panel_state: tauri::State<'_, Arc<BrowserPanelState>>,
    cmd: BrowserAttachCmd,
) -> Result<serde_json::Value, String> {
    panel::attach(
        &app,
        &cmd.window_label,
        (cmd.x, cmd.y, cmd.width, cmd.height),
        &cmd.home_url,
        &panel_state,
    )
    .await?;
    Ok(json!({ "attached": true }))
}

/// 移除子 WebView（面板关闭时调用；幂等）。
/// `window_label`：调用方窗口（前端 getCurrentWindow）；视图已迁到悬浮窗时跳过关闭
#[tauri::command]
pub async fn browser_detach(
    app: tauri::AppHandle,
    panel_state: tauri::State<'_, Arc<BrowserPanelState>>,
    window_label: Option<String>,
) -> Result<serde_json::Value, String> {
    panel::detach(&app, &panel_state, window_label.as_deref()).await;
    Ok(json!({ "attached": false }))
}

/// 弹出为悬浮窗：创建 /browser-popout 独立窗口并 reparent 浏览器视图（幂等）
#[tauri::command]
pub async fn browser_popout(
    app: tauri::AppHandle,
    panel_state: tauri::State<'_, Arc<BrowserPanelState>>,
) -> Result<serde_json::Value, String> {
    panel::popout(&app, &panel_state).await?;
    Ok(json!({ "ok": true }))
}

/// 导航（面板 URL 栏或 agent 共用入口）
#[tauri::command]
pub async fn browser_navigate(
    app: tauri::AppHandle,
    panel_state: tauri::State<'_, Arc<BrowserPanelState>>,
    url: String,
) -> Result<serde_json::Value, String> {
    panel::navigate(&app, &url, &panel_state).await?;
    Ok(json!({ "ok": true, "url": panel::normalize_for_display(&url) }))
}

/// 在视图中执行 JS（agent 信息采集）
#[tauri::command]
pub async fn browser_eval(
    app: tauri::AppHandle,
    script: String,
) -> Result<serde_json::Value, String> {
    panel::eval(&app, &script).await?;
    Ok(json!({ "ok": true }))
}

/// 当前 URL/标题（面板状态栏显示用）
#[tauri::command]
pub async fn browser_current(
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let (url, title) = panel::current_info(&app).await?;
    Ok(json!({ "url": url, "title": title }))
}

/// 查询子 WebView 挂载状态（侧栏指示/诊断用）
#[tauri::command]
pub async fn browser_panel_status(
    panel_state: tauri::State<'_, Arc<BrowserPanelState>>,
) -> Result<serde_json::Value, String> {
    Ok(json!({ "attached": panel_state.is_attached().await }))
}

/// TEMP-TEST: 打开 example.com 并输出链接坐标（供合成点击实验）
#[tauri::command]
pub async fn __test_click(
    app: tauri::AppHandle,
    panel_state: tauri::State<'_, Arc<BrowserPanelState>>,
) -> Result<serde_json::Value, String> {
    eprintln!("[TEMP-TEST] __test_click 开始");
    use crate::infra::browser::{drive, panel};
    let panel = panel_state.inner().clone();
    let nav = drive::navigate(&app, &panel, "https://example.com").await;
    eprintln!("[TEMP-TEST] navigate: {nav:?}");
    // 链接在子视图内的 CSS 坐标
    let rect = panel::eval_value(
        &app,
        &panel,
        r#"JSON.stringify(document.querySelector('a').getBoundingClientRect())"#,
    )
    .await;
    eprintln!("[TEMP-TEST] 链接 rect：{rect:?}");
    Ok(json!({ "ok": true }))
}

/// TEMP-TEST: 在主视图与子视图安装 click 探针（标题回显落点）
#[tauri::command]
pub async fn __test_probe(
    app: tauri::AppHandle,
    panel_state: tauri::State<'_, Arc<BrowserPanelState>>,
) -> Result<serde_json::Value, String> {
    eprintln!("[TEMP-TEST] __test_probe 安装探针");
    // 主视图：document.title 会反映到窗口标题（CGWindowList 可读出）
    if let Some(window) = app.get_window("main") {
        if let Some(main_wv) = window.webviews().into_iter().find(|w| w.label() == "main") {
            let _ = main_wv.eval(
                "document.addEventListener('click', function(e){ document.title = 'MAIN ' + e.clientX + ',' + e.clientY; })",
            );
        }
    }
    // 子视图：标题变更走 browser:title 事件（日志可见）
    let panel = panel_state.inner().clone();
    let _ = crate::infra::browser::panel::eval_fire(
        &app,
        &panel,
        "document.addEventListener('click', function(e){ document.title = 'CHILD ' + e.clientX + ',' + e.clientY; })",
    )
    .await;
    Ok(json!({ "ok": true }))
}
