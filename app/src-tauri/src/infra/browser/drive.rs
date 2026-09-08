//! Agent 浏览器驱动层：以应用内嵌面板 WebView 为执行体的高层 API。
//!
//! 替代此前的 headless Chromium + CDP 方案——agent 与用户共享同一可视视图
//! （用户实时看到并可接管 agent 的操作）。JS 采集逻辑沿用旧 page.rs（引擎无关），
//! 结果经 `panel::eval_value` 的自定义 scheme 导航通道回传。

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::panel::{self, BrowserPanelState};

/// 等待面板自动打开的最长时间
const OPEN_WAIT: Duration = Duration::from_secs(6);

/// 页面信息（URL + 标题）
struct PageInfo {
    url: String,
    title: String,
}

/// 导航到 URL（自动打开浏览器面板），等待加载稳定；返回页面信息
pub async fn navigate(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
    url: &str,
) -> Result<String, String> {
    panel::ensure_open(app, state, OPEN_WAIT).await?;
    // 等原生加载事件（on_page_load Finished），期间不 eval——
    // eval 回传通道的 location.href 赋值会取消进行中的页面加载
    let gen = state.load_finished_seq();
    panel::navigate(app, url, state).await?;
    wait_load_finished(state, gen, 8).await;
    // 兜底再等 readyState（个别场景 Finished 早于心智上的“可用”，短等无害）
    wait_ready(app, state, 3).await;
    let info = page_info(app, state).await.unwrap_or(PageInfo {
        url: panel::normalize_for_display(url),
        title: String::new(),
    });
    Ok(format!(
        "已打开：{}\n标题：{}（画面实时显示在应用浏览器面板中，用户可见、可与你共同操作同一浏览器）\n如需页面内容请调用 BrowserSnapshot。",
        info.url, info.title
    ))
}

/// 等 load_finished 序号越过 gen（导航真实完成；原生事件，不碰 JS）
async fn wait_load_finished(state: &Arc<BrowserPanelState>, gen: u64, max_secs: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(max_secs);
    loop {
        if state.load_finished_seq() > gen {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            return; // 超时放行：慢页不阻塞 agent
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// 有进行中的页面加载时等其收尾（snapshot/action 前调用，
/// 避免 eval 回传通道的 location.href 赋值打断加载中的页面）
async fn settle(state: &Arc<BrowserPanelState>, max_secs: u64) {
    let started = state.load_started_seq();
    let finished = state.load_finished_seq();
    if started > finished {
        wait_load_finished(state, finished, max_secs).await;
    }
}

/// 页面快照：注入编号标记 → 收集可交互元素 + 正文摘要。
/// 输出为文本：`[k] <tag> 文本/占位` 行列表，模型用 BrowserAction(k) 操作对应元素。
pub async fn snapshot(app: &tauri::AppHandle, state: &Arc<BrowserPanelState>) -> Result<String, String> {
    panel::ensure_open(app, state, OPEN_WAIT).await?;
    settle(state, 5).await;
    // 注入编号：给可交互元素打 data-cyan-k 属性（可点击/可输入/链接）
    eval(app, state, NUMBER_JS).await?;
    // 收集：编号 + 标签 + 可见文本/占位/aria + href
    let raw = eval(app, state, COLLECT_JS).await?;
    let items: Vec<Value> =
        serde_json::from_str(&raw).map_err(|e| format!("快照解析失败：{e}"))?;
    let info = page_info(app, state).await.unwrap_or(PageInfo {
        url: String::new(),
        title: String::new(),
    });
    let mut lines = vec![format!("页面：{}", info.url), format!("标题：{}", info.title)];
    if items.is_empty() {
        lines.push("（无可交互元素）".into());
    } else {
        lines.push(format!("可交互元素（BrowserAction 用 k 定位，共 {} 个）：", items.len()));
        for it in &items {
            let k = it["k"].as_str().unwrap_or_default();
            let tag = it["tag"].as_str().unwrap_or_default();
            let text = it["text"].as_str().unwrap_or_default();
            let href = it["href"].as_str().unwrap_or_default();
            let ty = it["type"].as_str().unwrap_or_default();
            let mut line = format!("[{k}] <{tag}");
            if !ty.is_empty() {
                line.push_str(&format!(" type={ty}"));
            }
            line.push('>');
            if !text.is_empty() {
                line.push_str(&format!(" {text}"));
            }
            if !href.is_empty() {
                line.push_str(&format!(" → {href}"));
            }
            lines.push(line);
        }
    }
    // 正文摘要（前 1500 字符）：帮助模型判断页面语境
    let body = eval(
        app,
        state,
        "document.body ? document.body.innerText.slice(0, 1500) : ''",
    )
    .await
    .unwrap_or_default();
    if !body.trim().is_empty() {
        lines.push("—— 正文摘要 ——".into());
        lines.push(body.trim().to_string());
    }
    Ok(lines.join("\n"))
}

/// 对元素执行动作：click / type / clear_and_type / press_enter / scroll_into
pub async fn action(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
    k: &str,
    action_kind: &str,
    text: Option<&str>,
) -> Result<String, String> {
    panel::ensure_open(app, state, OPEN_WAIT).await?;
    settle(state, 5).await;
    if !k.chars().all(|c| c.is_ascii_digit()) {
        return Err("k 必须是 BrowserSnapshot 返回的编号".into());
    }
    let started_gen = state.load_started_seq();
    let finished_gen = state.load_finished_seq();
    let selector = format!("[data-cyan-k=\"{k}\"]");
    // 元素存在性检查（real_key 不针对元素，跳过）
    if action_kind != "real_key" {
        let exists = eval(
            app,
            state,
            &format!("!!document.querySelector({})", json!(selector)),
        )
        .await?;
        if exists.trim() != "true" {
            return Err(format!(
                "元素 [{k}] 不存在，页面可能已变化，请先 BrowserSnapshot 重新获取"
            ));
        }
    }
    // 注意：动作脚本走 eval_fire（无回传通道）——eval_value 的回传靠 location.href
    // 赋值，会取消 click/submit 发起的页面跳转
    match action_kind {
        "click" => {
            fire(app, state, &format!(
                "(function(){{var el=document.querySelector({}); el.click(); return true;}})()",
                json!(selector)
            ))
            .await?;
        }
        // 原生动作（CGEvent 注入真实鼠标/键盘，isTrusted=true）：
        // JS 合成事件无法唤起 <select> 原生弹层等系统 UI，real_* 走 OS 级输入。
        // 代价：真实光标移动、窗口被激活到前台
        "real_click" => {
            let (x, y) = element_center(app, state, &selector).await?;
            panel::native_click(app, state, x, y).await?;
        }
        "real_type" => {
            let text = text.ok_or_else(|| "real_type 动作需要 text 参数".to_string())?;
            let (x, y) = element_center(app, state, &selector).await?;
            panel::native_click(app, state, x, y).await?; // 点击聚焦
            tokio::time::sleep(Duration::from_millis(120)).await;
            panel::native_select_all(state).await?; // 清空已有内容
            panel::native_key(state, "backspace").await?;
            panel::native_type(state, text).await?;
        }
        "real_key" => {
            let key = text.ok_or_else(|| {
                "real_key 动作需要 text 参数（enter/tab/escape/backspace/left/right/up/down）"
                    .to_string()
            })?;
            panel::native_key(state, key).await?;
        }
        "type" | "clear_and_type" => {
            let text = text.ok_or_else(|| "type 动作需要 text 参数".to_string())?;
            let clear = if action_kind == "clear_and_type" { "el.value='';" } else { "" };
            fire(app, state, &format!(
                "(function(){{var el=document.querySelector({}); {clear} el.value={}; el.dispatchEvent(new Event('input',{{bubbles:true}})); el.dispatchEvent(new Event('change',{{bubbles:true}})); return true;}})()",
                json!(selector),
                json!(text)
            ))
            .await?;
        }
        "press_enter" => {
            fire(app, state, &format!(
                "(function(){{var el=document.querySelector({}); el.dispatchEvent(new KeyboardEvent('keydown',{{key:'Enter',code:'Enter',keyCode:13,which:13,bubbles:true}})); var form=el.closest('form'); if(form) form.requestSubmit ? form.requestSubmit() : form.submit(); else el.click(); return true;}})()",
                json!(selector)
            ))
            .await?;
        }
        "scroll_into" => {
            fire(app, state, &format!(
                "(function(){{var el=document.querySelector({}); el.scrollIntoView({{behavior:'instant',block:'center'}}); return true;}})()",
                json!(selector)
            ))
            .await?;
        }
        other => {
            return Err(format!(
                "未知动作：{other}（支持 click/real_click/real_type/real_key/type/clear_and_type/press_enter/scroll_into）"
            ))
        }
    }
    // 动作可能触发跳转（click/submit）：给导航一个发起窗口，
    // 有新的 Started 才等其 Finished（type 等不跳转的动作零额外等待）
    tokio::time::sleep(Duration::from_millis(300)).await;
    if state.load_started_seq() > started_gen {
        wait_load_finished(state, finished_gen, 5).await;
    }
    wait_ready(app, state, 2).await;
    let info = page_info(app, state).await.unwrap_or(PageInfo {
        url: String::new(),
        title: String::new(),
    });
    Ok(format!("已执行 {action_kind} → [{k}]；当前页面：{}", info.url))
}

/// eval_value 简写
async fn eval(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
    script: &str,
) -> Result<String, String> {
    panel::eval_value(app, state, script).await
}

/// 元素滚动到可视区中央并返回中心点（页面视口 CSS px，供原生点击换算坐标）
async fn element_center(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
    selector: &str,
) -> Result<(f64, f64), String> {
    let raw = eval(
        app,
        state,
        &format!(
            "(function(){{var el=document.querySelector({}); el.scrollIntoView({{behavior:'instant',block:'center',inline:'center'}}); var r=el.getBoundingClientRect(); return JSON.stringify({{x:r.left+r.width/2, y:r.top+r.height/2, vw:window.innerWidth, vh:window.innerHeight}});}})()",
            json!(selector)
        ),
    )
    .await?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("元素坐标解析失败：{e}"))?;
    let x = v["x"].as_f64().ok_or("元素坐标缺失")?;
    let y = v["y"].as_f64().ok_or("元素坐标缺失")?;
    // 视口校验：中心点仍出屏（横向滚动未跟上等）时不乱点，交回错误
    let vw = v["vw"].as_f64().unwrap_or(0.0);
    let vh = v["vh"].as_f64().unwrap_or(0.0);
    if x < 0.0 || y < 0.0 || (vw > 0.0 && x > vw) || (vh > 0.0 && y > vh) {
        return Err(format!(
            "元素中心点 ({x:.0},{y:.0}) 不在视口内（{vw:.0}×{vh:.0}），请滚动页面后重新快照"
        ));
    }
    Ok((x, y))
}

/// eval_fire 简写（动作脚本：无回传通道，不干扰页面跳转）
async fn fire(
    app: &tauri::AppHandle,
    state: &Arc<BrowserPanelState>,
    script: &str,
) -> Result<(), String> {
    panel::eval_fire(app, state, script).await
}

/// 页面信息（URL + 标题）
async fn page_info(app: &tauri::AppHandle, state: &Arc<BrowserPanelState>) -> Result<PageInfo, String> {
    let raw = eval(
        app,
        state,
        r#"JSON.stringify({url: location.href, title: document.title})"#,
    )
    .await?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("页面信息解析失败：{e}"))?;
    Ok(PageInfo {
        url: v["url"].as_str().unwrap_or_default().to_string(),
        title: v["title"].as_str().unwrap_or_default().to_string(),
    })
}

/// 等 document.readyState === complete（或超时继续，慢页不阻塞 agent）
async fn wait_ready(app: &tauri::AppHandle, state: &Arc<BrowserPanelState>, max_secs: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(max_secs);
    loop {
        if let Ok(state_str) = eval(app, state, "document.readyState").await {
            if state_str.trim() == "complete" {
                return;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return; // 超时放行：交互式页面可能永远不是 complete
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// 注入编号：给可见可交互元素打 data-cyan-k 属性
const NUMBER_JS: &str = r#"(function () {
  var k = 0;
  var sel = 'a[href], button, input, select, textarea, [onclick], [role="button"], [role="link"], [role="tab"]';
  document.querySelectorAll(sel).forEach(function (el) {
    if (el.offsetParent !== null || el.getClientRects().length > 0) {
      k += 1;
      el.setAttribute('data-cyan-k', String(k));
    }
  });
  window.__cyanK = k;
  return k;
})()"#;

/// 收集：编号 + 标签 + 可见文本/占位/aria + href
const COLLECT_JS: &str = r#"(function () {
  var out = [];
  document.querySelectorAll('[data-cyan-k]').forEach(function (el) {
    var k = el.getAttribute('data-cyan-k');
    var tag = el.tagName.toLowerCase();
    var text = (el.innerText || el.getAttribute('placeholder') || el.getAttribute('aria-label') || '').trim().replace(/\s+/g, ' ').slice(0, 80);
    var href = tag === 'a' ? (el.getAttribute('href') || '') : '';
    var type = tag === 'input' ? (el.getAttribute('type') || 'text') : '';
    out.push({k: k, tag: tag, text: text, href: href, type: type});
  });
  return JSON.stringify(out);
})()"#;
