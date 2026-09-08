//! 浏览器面板（Tauri 原生子 WebView 内嵌方案）。
//!
//! 用 `Window.add_child` 在主窗口内创建一块独立子 WebView（加载外部 URL 的真实
//! 浏览器渲染区域），等价于 codex 的 Electron WebContentsView：
//! - 画面：原生渲染，零投屏开销（替换此前 Chromium+CDP 抓帧轮询）
//! - 交互：WebView 本身就是可交互控件（点击/打字/滚动原生直通）
//! - 持久化：`data_directory` 指向 `~/.cyan/browser-data`，登录态跨会话保留
//! - agent 控制：导航/标题事件 + eval 求值（快照类能力）
//!
//! 生命周期：面板挂载 → attach（子 webview 创建并定位到面板容器区）；
//! 卸载/尺寸变化 → detach（移除子视图）。URL 栏导航与 agent 导航共享同一视图。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};
use tokio::sync::oneshot;
use tokio::sync::Mutex as AsyncMutex;

use crate::infra::computer::native_input;
use crate::infra::db::datasource::cyan_home;

/// 子 WebView label（固定：单浏览器视图，重建前先 detach 旧的）
const WEBVIEW_LABEL: &str = "cyan-browser-webview";

/// 自定义 User-Agent：WKWebView 默认 UA 是旧版 Safari 标识，
/// 会被 B 站等站点判为「浏览器版本过低」而拦截；伪装成桌面 Chrome 规避
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/139.0.0.0 Safari/537.36";

/// eval 结果回传 scheme：JS 把结果 base64url 编码后导航到
/// `cyan-eval://r/<id>/<payload>`，on_navigation 拦截（返回 false 阻止真实跳转）
const EVAL_SCHEME: &str = "cyan-eval";

/// eval 回传等待超时（页面跳转中等不到回调时兜底）
const EVAL_TIMEOUT: Duration = Duration::from_secs(10);

/// TEMP-TEST: 常驻点击探针 v2（initialization_script，每次导航 document-start 注入，
/// 跳转后仍有效）。capture 阶段监听 mousedown/mouseup/click/dragstart，事件序列滚动
/// 写进 document.title（经 on_document_title_changed 落日志；保留最近若干条以防 KVO
/// 合并丢中间态）；同时包装 window.open 探 JS 新窗口调用。用于定位「点击链接没反应」：
/// click 有日志但无 on_navigation/on_new_window = WKWebView 丢了 target=_blank 导航；
/// 只有 mousedown 没有 click = 点击序列未完整到达页面；全无 = 点击未到达内容。
const CLICK_PROBE_SCRIPT: &str = r#"(function(){
  if (window.__cyanProbe) return; window.__cyanProbe = true;
  var hist = [];
  function log(m) {
    hist.push(m); if (hist.length > 6) hist.shift();
    try { document.title = 'EVT ' + hist.join(' ; '); } catch (_) {}
  }
  ['mousedown', 'mouseup', 'click', 'dragstart'].forEach(function(t) {
    window.addEventListener(t, function(e) {
      var a = e.target && e.target.closest ? e.target.closest('a') : null;
      log(t + ' ' + (e.clientX | 0) + ',' + (e.clientY | 0) +
        (a ? ' a[t=' + (a.getAttribute('target') || 'self') + ']' : ''));
    }, true);
  });
  // bubble 阶段：页面 JS 处理完后看 defaultPrevented（判断导航是否被页面脚本拦下）
  window.addEventListener('click', function(e) {
    var a = e.target && e.target.closest ? e.target.closest('a') : null;
    if (a) log('bubble-click defaultPrevented=' + e.defaultPrevented +
      ' s_session=' + typeof window.s_session);
  }, false);
  // 未捕获 JS 异常（怀疑百度统计回调里抛错中断跳转）
  window.addEventListener('error', function(e) {
    log('JSERR ' + String(e.message).slice(0, 80) + ' @' +
      String(e.filename).split('/').pop() + ':' + e.lineno);
  }, true);
  var origOpen = window.open;
  window.open = function(u) {
    log('WINOPEN ' + String(u).slice(0, 60));
    return origOpen ? origOpen.apply(window, arguments) : null;
  };
  // target=_blank 兜底：capture 阶段优先于页面 JS（百度等站点会 preventDefault
  // 后用统计脚本回调跳转，该链路在 WKWebView 单视图里静默失败），直接转为当前页
  // 导航——与单视图浏览器「不弹新窗」的设计一致，也覆盖 WKWebView 不触发
  // createWebViewWithConfiguration 的情况
  window.addEventListener('click', function(e) {
    if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    var a = e.target && e.target.closest ? e.target.closest('a[href]') : null;
    if (!a) return;
    if ((a.getAttribute('target') || '').toLowerCase() !== '_blank') return;
    var href = a.href;
    if (!/^https?:\/\//.test(href)) return;
    log('BLANK->SELF ' + String(href).slice(0, 60));
    e.preventDefault();
    e.stopImmediatePropagation();
    location.href = href;
  }, true);
})();"#;

/// eval 结果回传桥。std Mutex：on_navigation 回调在 WebView 主线程同步执行，
/// 不能在里面 await / 持 async 锁（可能死锁），临界区只做 map 操作
#[derive(Default)]
struct EvalBridge {
    seq: u64,
    pending: HashMap<u64, oneshot::Sender<String>>,
}

/// 面板状态（App 共享）：当前子 webview 的归属窗口与位置
pub struct BrowserPanelState {
    inner: AsyncMutex<Option<PanelInner>>,
    /// attach/detach 串行化锁：Tauri 命令并发执行，两个 attach 可能同时通过
    /// 存在性检查后重复 add_child（label 全局唯一 → already exists），需互斥
    op_lock: AsyncMutex<()>,
    /// eval 结果回传注册表
    evals: Mutex<EvalBridge>,
    /// 页面加载事件序号（on_page_load 驱动）：agent 驱动层以此等待导航真实完成，
    /// 避免在加载途中 eval（回传通道的 location.href 赋值会取消进行中的跳转）
    load_started: AtomicU64,
    load_finished: AtomicU64,
}

struct PanelInner {
    /// 宿主窗口 label（attach 时记录）
    window_label: String,
    /// 子视图在窗口内的位置与尺寸（CSS px，attach 时前端实测传入）；
    /// 原生输入注入（CGEvent）换算屏幕坐标用
    rect: (f64, f64, f64, f64),
}

impl BrowserPanelState {
    /// 构造（未 attach）
    pub fn new() -> Self {
        Self {
            inner: AsyncMutex::new(None),
            op_lock: AsyncMutex::new(()),
            evals: Mutex::new(EvalBridge::default()),
            load_started: AtomicU64::new(0),
            load_finished: AtomicU64::new(0),
        }
    }

    /// 页面开始加载事件累计次数
    pub fn load_started_seq(&self) -> u64 {
        self.load_started.load(Ordering::Relaxed)
    }

    /// 页面加载完成事件累计次数
    pub fn load_finished_seq(&self) -> u64 {
        self.load_finished.load(Ordering::Relaxed)
    }

    /// 是否已 attach（子视图存在）
    pub async fn is_attached(&self) -> bool {
        self.inner.lock().await.is_some()
    }

    /// 持久化数据目录：`~/.cyan/browser-data`（WebView 的 cookie/localStorage 等）
    pub fn data_dir() -> PathBuf {
        cyan_home()
            .map(|h| h.join("browser-data"))
            .unwrap_or_else(|_| PathBuf::from(".cyan/browser-data"))
    }
}

impl Default for BrowserPanelState {
    fn default() -> Self {
        Self::new()
    }
}

/// `rect` 是面板内容区在窗口内的位置与尺寸（CSS px，前端实测后传）。
/// 已有视图时原地重定位（set_bounds，不重创建——保持页面状态与登录会话不丢）；
/// 首次/被销毁后才创建，创建后加载 `home`（默认主页，前端设置下发）。add_child 按
/// label 全局唯一，重复创建会报 already exists。
pub async fn attach(
    app: &tauri::AppHandle,
    window_label: &str,
    rect: (f64, f64, f64, f64),
    home: &str,
    state: &Arc<BrowserPanelState>,
) -> Result<(), String> {
    // 全程持锁：存在性检查与 add_child 必须原子，否则并发 attach 会重复创建
    let _op = state.op_lock.lock().await;
    eprintln!("[TEMP-TEST] attach rect={rect:?}");
    let window = app
        .get_window(window_label)
        .ok_or_else(|| format!("窗口不存在：{window_label}"))?;
    // 跨屏诊断：窗口位置 + 所在屏缩放因子（logical/physical 换算问题定位用）
    eprintln!(
        "[TEMP-TEST] window pos={:?} size={:?} scale={:?}",
        window.outer_position().ok(),
        window.inner_size().ok(),
        window.scale_factor().ok()
    );
    // 已挂载：原地调整边界（不重创建，页面状态/登录态保留）
    if let Some(webview) = find_webview(&window) {
        return reposition(&webview, rect, state, window_label).await;
    }
    // 视图在别的窗口（悬浮窗 ⇄ 主面板互迁）：reparent 过来再定位
    if let Some(webview) = app.webviews().into_values().find(|w| w.label() == WEBVIEW_LABEL) {
        webview
            .reparent(&window)
            .map_err(|e| format!("迁移浏览器视图失败：{e}"))?;
        return reposition(&webview, rect, state, window_label).await;
    }
    let data_dir = BrowserPanelState::data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|e| format!("创建浏览器数据目录失败：{e}"))?;

    // 事件回调需 'static：clone app handle / state 移入闭包
    let app_for_events = app.clone();
    let app_for_nav = app.clone();
    let app_for_newwin = app.clone();
    let state_for_nav = state.clone();
    let state_for_load = state.clone();
    let webview = tauri::webview::WebviewBuilder::new(
        WEBVIEW_LABEL,
        WebviewUrl::External("about:blank".parse().map_err(|_| "URL 解析失败")?),
    )
    .data_directory(data_dir)
    .user_agent(USER_AGENT)
    // TEMP-TEST: 常驻点击探针（跳转后仍有效，见常量注释）
    .initialization_script(CLICK_PROBE_SCRIPT)
    // eval 结果回传通道：拦截 cyan-eval:// 导航，解码后完成等待中的 oneshot
    .on_navigation(move |url| {
        if url.scheme() == EVAL_SCHEME {
            handle_eval_result(&state_for_nav, url);
            return false;
        }
        // 地址栏同步：主框架导航（用户点链接/agent 导航/站内跳转）都通知前端更新 URL 栏
        if matches!(url.scheme(), "http" | "https") {
            eprintln!("[TEMP-TEST] on_navigation: {url}");
            let _ = app_for_nav.emit("browser:url", json!({ "url": url.to_string() }));
        }
        true
    })
    // 页面加载事件序号：drive 层等待导航完成的原生信号（不碰 JS，不干扰加载）
    .on_page_load(move |_webview, payload| {
        match payload.event() {
            tauri::webview::PageLoadEvent::Started => {
                state_for_load.load_started.fetch_add(1, Ordering::Relaxed);
            }
            tauri::webview::PageLoadEvent::Finished => {
                state_for_load.load_finished.fetch_add(1, Ordering::Relaxed);
            }
        }
    })
    // 起始页：about:blank 由面板 URL 栏接管（用户输入或 agent 导航后加载真实页面）
    .on_document_title_changed(move |_webview, title| {
        eprintln!("[TEMP-TEST] 子视图标题：{title}");
        let _ = app_for_events.emit("browser:title", json!({ "title": title }));
    })
    // target=_blank / window.open：WKWebView 默认丢弃新窗口导航（表现为“点链接没反应”），
    // 改为在当前视图内打开（单视图浏览器，不弹新窗）。
    // 视图可能被 reparent 到悬浮窗，按 label 全局查找而不是记死宿主窗口
    .on_new_window(move |url, _features| {
        eprintln!("[TEMP-TEST] on_new_window: {url}");
        if matches!(url.scheme(), "http" | "https") {
            if let Some(wv) = app_for_newwin
                .webviews()
                .into_values()
                .find(|w| w.label() == WEBVIEW_LABEL)
            {
                let _ = wv.navigate(url);
            }
        }
        tauri::webview::NewWindowResponse::Deny
    });
    let child = match window.add_child(
        webview,
        LogicalPosition::new(rect.0, rect.1),
        LogicalSize::new(rect.2, rect.3),
    ) {
        Ok(w) => w,
        Err(e) => {
            // 自愈：残留 label（异常退出/外部关闭后注册表未清）导致 already exists 时，
            // 改走原地重定位而不是报错给前端
            if e.to_string().contains("already exists") {
                if let Some(webview) = find_webview(&window) {
                    return reposition(&webview, rect, state, window_label).await;
                }
            }
            return Err(format!("嵌入浏览器视图失败：{e}"));
        }
    };
    *state.inner.lock().await = Some(PanelInner {
        window_label: window_label.to_string(),
        rect,
    });
    // 新建视图加载默认主页（设置页可选；空串停留 about:blank）
    if !home.trim().is_empty() {
        let home = normalize_url(home);
        let _ = child.navigate(home.parse().map_err(|_| "主页 URL 解析失败")?);
    }
    Ok(())
}

/// 在窗口的子视图列表里按固定 label 查找浏览器视图
fn find_webview(window: &tauri::Window) -> Option<tauri::Webview> {
    window
        .webviews()
        .into_iter()
        .find(|w| w.label() == WEBVIEW_LABEL)
}

/// 已存在视图时原地重定位（不重创建，页面状态/登录态保留）
async fn reposition(
    webview: &tauri::Webview,
    rect: (f64, f64, f64, f64),
    state: &Arc<BrowserPanelState>,
    window_label: &str,
) -> Result<(), String> {
    webview
        .set_bounds(tauri::Rect {
            position: LogicalPosition::new(rect.0, rect.1).into(),
            size: LogicalSize::new(rect.2, rect.3).into(),
        })
        .map_err(|e| format!("调整浏览器视图位置失败：{e}"))?;
    *state.inner.lock().await = Some(PanelInner {
        window_label: window_label.to_string(),
        rect,
    });
    Ok(())
}

/// 移除子 WebView（面板卸载时调用；幂等）。
/// 同时兜底清理注册表里的残留 label（避免外部关闭后状态不同步导致 attach 报已存在）。
/// `caller_label`：调用方窗口 label；视图已被 reparent 到其他窗口（悬浮窗）时跳过——
/// 旧宿主面板卸载不应关掉已迁走的视图。
pub async fn detach(app: &tauri::AppHandle, state: &Arc<BrowserPanelState>, caller_label: Option<&str>) {
    // 与 attach 同一把锁：close 不能插在别的 attach 的检查与创建之间
    let _op = state.op_lock.lock().await;
    let mut guard = state.inner.lock().await;
    if let Some(inner) = guard.as_ref() {
        if caller_label.is_some_and(|c| c != inner.window_label) {
            return; // 视图已迁往其他窗口，调用方不再是宿主
        }
    }
    let inner = guard.take();
    // 无论内部状态有无记录，都按 label 清理（外部窗口关闭/异常退出后自愈）
    let labels: Vec<String> = match &inner {
        Some(i) => vec![i.window_label.clone()],
        None => vec!["main".to_string()],
    };
    for label in labels {
        let Some(window) = app.get_window(&label) else {
            continue;
        };
        let webviews = window.webviews();
        if let Some(webview) = webviews.iter().find(|w| w.label() == WEBVIEW_LABEL) {
            let _ = webview.close();
        }
    }
}

/// 悬浮窗 label（弹出为独立窗口；/browser-popout 路由加载浮动面板 UI）
pub const POPOUT_WINDOW_LABEL: &str = "browser-popout";

/// 弹出为悬浮窗：创建独立窗口并把浏览器子视图 reparent 过去（页面状态/登录态保留）。
/// 幂等：悬浮窗已存在则聚焦。悬浮窗关闭时子视图随之销毁，此处同步清状态防脏读。
pub async fn popout(app: &tauri::AppHandle, state: &Arc<BrowserPanelState>) -> Result<(), String> {
    let _op = state.op_lock.lock().await;
    // 幂等：已存在则聚焦
    if let Some(w) = app.get_window(POPOUT_WINDOW_LABEL) {
        let _ = w.set_focus();
        return Ok(());
    }
    let host_label = {
        let guard = state.inner.lock().await;
        guard
            .as_ref()
            .map(|i| i.window_label.clone())
            .ok_or_else(|| "浏览器面板未打开，无视图可弹出".to_string())?
    };
    let host_window = app
        .get_window(&host_label)
        .ok_or_else(|| "宿主窗口不存在".to_string())?;
    let Some(webview) = find_webview(&host_window) else {
        return Err("浏览器视图不存在".to_string());
    };
    let win = tauri::WebviewWindowBuilder::new(
        app,
        POPOUT_WINDOW_LABEL,
        WebviewUrl::App("index.html#/browser-popout".into()),
    )
    .title("cyan 浏览器")
    .inner_size(960.0, 720.0)
    .build()
    .map_err(|e| format!("创建悬浮窗失败：{e}"))?;
    // reparent 目标是 Window（与 WebviewWindow 同 label 的句柄）
    let win_handle = app
        .get_window(POPOUT_WINDOW_LABEL)
        .ok_or_else(|| "悬浮窗句柄不存在".to_string())?;
    webview
        .reparent(&win_handle)
        .map_err(|e| format!("迁移浏览器视图失败：{e}"))?;
    // 先铺满整个窗口（浮窗 UI 挂载后 attach 会带上工具栏偏移精确重定位）
    let size = win
        .inner_size()
        .map_err(|e| format!("获取悬浮窗尺寸失败：{e}"))?
        .to_logical::<f64>(win.scale_factor().unwrap_or(1.0));
    webview
        .set_bounds(tauri::Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(size.width, size.height).into(),
        })
        .map_err(|e| format!("调整浏览器视图位置失败：{e}"))?;
    *state.inner.lock().await = Some(PanelInner {
        window_label: POPOUT_WINDOW_LABEL.to_string(),
        rect: (0.0, 0.0, size.width, size.height),
    });
    // 悬浮窗销毁：子视图随窗口销毁，清状态避免 is_attached 脏读
    let state_for_close = state.clone();
    win.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            let state = state_for_close.clone();
            tauri::async_runtime::spawn(async move {
                let mut guard = state.inner.lock().await;
                if guard
                    .as_ref()
                    .is_some_and(|i| i.window_label == POPOUT_WINDOW_LABEL)
                {
                    *guard = None;
                }
            });
        }
    });
    Ok(())
}

/// 导航到 URL（面板 URL 栏或 agent 共用入口；幂等）
pub async fn navigate(
    app: &tauri::AppHandle,
    url: &str,
    state: &Arc<BrowserPanelState>,
) -> Result<(), String> {
    let url = normalize_url(url);
    let window_label = {
        let guard = state.inner.lock().await;
        guard
            .as_ref()
            .map(|inner| inner.window_label.clone())
            .unwrap_or_else(|| "main".to_string())
    };
    let Some(window) = app.get_window(&window_label) else {
        return Err("宿主窗口不存在".to_string());
    };
    let webviews = window.webviews();
    let Some(webview) = webviews.iter().find(|w| w.label() == WEBVIEW_LABEL) else {
        return Err("浏览器视图不存在".to_string());
    };
    webview
        .navigate(url.parse().map_err(|_| "URL 解析失败")?)
        .map_err(|e| format!("导航失败：{e}"))?;
    Ok(())
}

/// 在视图中执行 JS（agent 快照/信息采集用；返回 JSON 字符串）
pub async fn eval(app: &tauri::AppHandle, script: &str) -> Result<String, String> {
    let window = app
        .get_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    let Some(webview) = find_webview(&window) else {
        return Err("浏览器视图未挂载".to_string());
    };
    webview
        .eval(script)
        .map_err(|e| format!("执行脚本失败：{e}"))?;
    Ok(json!({ "ok": true }).to_string())
}

/// 确保面板已打开并完成 attach：未挂载时发事件让前端自动打开，轮询等待挂载。
/// agent 工具入口调用（用户无需先手动打开面板）。
pub async fn ensure_open(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
    wait: Duration,
) -> Result<(), String> {
    if state.is_attached().await {
        return Ok(());
    }
    let _ = app.emit("browser:open-panel", json!({}));
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if state.is_attached().await {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("浏览器面板自动打开超时；请手动点击左侧「浏览器」后重试".to_string());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// 在面板视图中执行 JS（fire-and-forget，无回传通道）。
/// 用于会触发页面跳转的动作（click/submit 等）：eval_value 的回传通道靠
/// `location.href` 赋值实现，会取消脚本自身发起的导航，所以动作脚本必须走这里。
pub async fn eval_fire(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
    script: &str,
) -> Result<(), String> {
    let window_label = {
        let guard = state.inner.lock().await;
        guard
            .as_ref()
            .map(|inner| inner.window_label.clone())
            .unwrap_or_else(|| "main".to_string())
    };
    let window = app
        .get_window(&window_label)
        .ok_or_else(|| "宿主窗口不存在".to_string())?;
    let Some(webview) = find_webview(&window) else {
        return Err("浏览器视图未挂载".to_string());
    };
    webview
        .eval(script)
        .map_err(|e| format!("执行脚本失败：{e}"))?;
    Ok(())
}

/// 在面板视图中执行 JS 并取回结果（agent 驱动层用）。
/// 结果经 `cyan-eval://r/<id>/<base64url>` 导航回传；`script` 为单个表达式
/// （IIFE 亦可），字符串结果原样返回，其他类型 JSON 序列化后返回。
pub async fn eval_value(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
    script: &str,
) -> Result<String, String> {
    let window_label = {
        let guard = state.inner.lock().await;
        guard
            .as_ref()
            .map(|inner| inner.window_label.clone())
            .unwrap_or_else(|| "main".to_string())
    };
    let window = app
        .get_window(&window_label)
        .ok_or_else(|| "宿主窗口不存在".to_string())?;
    let Some(webview) = find_webview(&window) else {
        return Err("浏览器视图未挂载".to_string());
    };
    let (id, rx) = {
        let mut bridge = state.evals.lock().map_err(|_| "eval 桥锁中毒")?;
        bridge.seq += 1;
        let id = bridge.seq;
        let (tx, rx) = oneshot::channel();
        bridge.pending.insert(id, tx);
        (id, rx)
    };
    // 结果 base64url 编码放 URL path（WKWebView 自定义 scheme 拿不到 POST body，只能走 URL）
    let wrapped = format!(
        "(function(){{var __r;try{{__r=({script});}}catch(e){{__r=JSON.stringify({{__cyanErr:String(e)}});}}\
         var __p;try{{__p=(typeof __r==='string')?__r:JSON.stringify(__r===undefined?null:__r);}}catch(e2){{__p='null';}}\
         location.href='{EVAL_SCHEME}://r/{id}/'+btoa(unescape(encodeURIComponent(__p))).replace(/\\+/g,'-').replace(/\\//g,'_').replace(/=+$/,'');}})()"
    );
    if let Err(e) = webview.eval(&wrapped) {
        if let Ok(mut bridge) = state.evals.lock() {
            bridge.pending.remove(&id);
        }
        return Err(format!("执行脚本失败：{e}"));
    }
    match tokio::time::timeout(EVAL_TIMEOUT, rx).await {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(_)) => Err("eval 结果通道被关闭".to_string()),
        Err(_) => {
            if let Ok(mut bridge) = state.evals.lock() {
                bridge.pending.remove(&id);
            }
            Err("eval 回传超时（页面可能正在跳转，请稍后重试）".to_string())
        }
    }
}

/// on_navigation 拦截处理：`cyan-eval://r/<id>/<base64url>` → 解码并完成 oneshot
fn handle_eval_result(state: &Arc<BrowserPanelState>, url: &tauri::Url) {
    let segs: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    if segs.len() != 2 {
        return;
    }
    let Ok(id) = segs[0].parse::<u64>() else {
        return;
    };
    let Some(bytes) = b64url_decode(segs[1]) else {
        return;
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if let Ok(mut bridge) = state.evals.lock() {
        if let Some(tx) = bridge.pending.remove(&id) {
            let _ = tx.send(text);
        }
    }
}

/// base64url 解码（无 padding；`-_` 替代 `+/`）
fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    const fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        let v = val(c)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// 当前 URL/标题（面板状态栏显示用；导航事件未覆盖到的兜底轮询）
pub async fn current_info(app: &tauri::AppHandle) -> Result<(String, String), String> {
    let window = app
        .get_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    let webviews = window.webviews();
    let Some(webview) = webviews.iter().find(|w| w.label() == WEBVIEW_LABEL) else {
        return Err("浏览器视图未挂载".to_string());
    };
    let url = webview.url().map(|u| u.to_string()).unwrap_or_default();
    let title = String::new(); // 标题经 on_document_title_changed 事件驱动，此处只取 URL
    Ok((url, title))
}

/// URL 规范化：无 scheme 时补 https://（前端命令与 agent 工具共用）
pub fn normalize_for_display(url: &str) -> String {
    normalize_url(url)
}

// ==================== 原生输入注入（macOS CGEvent，对等 Codex 的 CDP Input 域）====================
//
// JS 合成事件（el.click() 等）isTrusted=false，无法唤起 <select> 原生弹层这类系统 UI。
// 这里改用 CGEventPost 注入真实鼠标/键盘事件：事件从窗口系统进入，与真人操作无差别。
// 代价：真实光标会移动、窗口需在前台（点击前 set_focus 激活）。

/// 真实鼠标点击面板内某点（页面视口 CSS px）
pub async fn native_click(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let (window, rect) = host_window_and_rect(app, state).await?;
    // 坐标硬校验：越界点击会打到 app 自身 UI 或其他窗口，宁可报错
    if x < 0.0 || y < 0.0 || x > rect.2 || y > rect.3 {
        return Err(format!(
            "点击坐标 ({x:.0},{y:.0}) 超出面板范围（{:.0}×{:.0}），已取消",
            rect.2, rect.3
        ));
    }
    // 幽灵光标：真实点击落下前，先在页面里把光标图标飞到目标点（agent 操作可视化）
    if let Some(wv) = find_webview(&window) {
        let _ = wv.eval(&ghost_cursor_js(x, y));
    }
    tokio::time::sleep(Duration::from_millis(200)).await; // 等飞行动画就位
    // CGEvent 落在屏幕坐标处「最上层」的窗口，必须先确保本窗口在前台
    window.set_focus().map_err(|e| format!("激活窗口失败：{e}"))?;
    tokio::time::sleep(Duration::from_millis(120)).await;
    native_input::post_click(screen_point(&window, rect, x, y)?)
}

/// 幽灵光标脚本：固定定位的光标图标（品牌色箭头）飞到 (x,y) 并显示点击涟漪。
/// 纯可视化层：pointer-events:none、z-index 拉满，不干扰页面交互；
/// 页面跳转后随文档销毁，下次点击重建。数字坐标直接内插（无注入面）。
fn ghost_cursor_js(x: f64, y: f64) -> String {
    format!(
        r##"(function(x, y) {{
  var ID = '__cyan_ghost_cursor';
  var g = document.getElementById(ID);
  if (!g) {{
    g = document.createElement('div');
    g.id = ID;
    g.style.cssText = 'position:fixed;z-index:2147483647;pointer-events:none;left:0;top:0;transition:left .18s ease,top .18s ease,opacity .3s;filter:drop-shadow(0 1px 2px rgba(0,0,0,.35));';
    g.innerHTML = '<svg width="22" height="22" viewBox="0 0 24 24" fill="#00B39E" stroke="#fff" stroke-width="1.2"><path d="M5 3l14 7.5-6.2 1.6L9.5 18 5 3z"/></svg>';
    (document.body || document.documentElement).appendChild(g);
  }}
  g.style.opacity = '1';
  g.style.left = x + 'px';
  g.style.top = y + 'px';
  // 点击涟漪
  var r = document.createElement('div');
  r.style.cssText = 'position:fixed;z-index:2147483646;pointer-events:none;left:' + x + 'px;top:' + y + 'px;width:10px;height:10px;margin:-5px 0 0 -5px;border:2px solid #00B39E;border-radius:50%;opacity:.9;transition:all .45s ease-out;';
  (document.body || document.documentElement).appendChild(r);
  requestAnimationFrame(function() {{
    r.style.width = '40px'; r.style.height = '40px'; r.style.margin = '-20px 0 0 -20px'; r.style.opacity = '0';
  }});
  setTimeout(function() {{ r.remove(); }}, 500);
  // 闲置 3s 后光标淡出，下次动作再现身
  clearTimeout(window.__cyanGhostTimer);
  window.__cyanGhostTimer = setTimeout(function() {{ g.style.opacity = '0'; }}, 3000);
}})({x}, {y})"##,
        x = x,
        y = y
    )
}

/// 真实键盘输入文本（逐字符 Unicode 注入，支持中文等任意字符；焦点由调用方保证）
pub async fn native_type(state: &Arc<BrowserPanelState>, text: &str) -> Result<(), String> {
    if !state.is_attached().await {
        return Err("浏览器面板未挂载".to_string());
    }
    native_input::post_text(text)
}

/// 真实按下具名键（enter/tab/escape/backspace/left/right/up/down）
pub async fn native_key(state: &Arc<BrowserPanelState>, key: &str) -> Result<(), String> {
    if !state.is_attached().await {
        return Err("浏览器面板未挂载".to_string());
    }
    native_input::post_key(key)
}

/// 真实全选（Cmd+A；清空输入框的前置步骤）
pub async fn native_select_all(state: &Arc<BrowserPanelState>) -> Result<(), String> {
    if !state.is_attached().await {
        return Err("浏览器面板未挂载".to_string());
    }
    native_input::post_select_all()
}

/// 取宿主窗口与面板 rect（原生输入换算坐标用）
async fn host_window_and_rect(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
) -> Result<(tauri::Window, (f64, f64, f64, f64)), String> {
    let (window_label, rect) = {
        let guard = state.inner.lock().await;
        guard
            .as_ref()
            .map(|i| (i.window_label.clone(), i.rect))
            .ok_or_else(|| "浏览器面板未挂载".to_string())?
    };
    let window = app
        .get_window(&window_label)
        .ok_or_else(|| "宿主窗口不存在".to_string())?;
    Ok((window, rect))
}

/// 面板内 CSS 坐标 → 屏幕逻辑坐标（CGEvent 全局显示坐标，原点左上；tauri
/// inner_position 是物理像素，需除以缩放因子）。三平台公式相同，不做 cfg 门控
fn screen_point(
    window: &tauri::Window,
    rect: (f64, f64, f64, f64),
    x: f64,
    y: f64,
) -> Result<(f64, f64), String> {
    let scale = window
        .scale_factor()
        .map_err(|e| format!("获取屏幕缩放失败：{e}"))?;
    let pos = window
        .inner_position()
        .map_err(|e| format!("获取窗口位置失败：{e}"))?;
    Ok((
        pos.x as f64 / scale + rect.0 + x,
        pos.y as f64 / scale + rect.1 + y,
    ))
}

/// URL 规范化：无 scheme 时补 https://
fn normalize_url(url: &str) -> String {
    let u = url.trim();
    if u.starts_with("http://")
        || u.starts_with("https://")
        || u.starts_with("file://")
        || u.starts_with("about:")
        || u.starts_with("chrome://")
    {
        u.to_string()
    } else {
        format!("https://{u}")
    }
}
