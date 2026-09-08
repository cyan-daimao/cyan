//! 原生浏览器模块：主窗口内嵌子 WebView（WKWebView），agent 与用户共享同一可视视图。
//!
//! 设计（替代此前的 headless Chromium + CDP 方案）：
//! - `panel`：子 WebView 生命周期（attach/detach/navigate/eval），
//!   原生渲染零投屏开销；持久数据目录 `~/.cyan/browser-data` 保留登录态；
//!   eval 结果经 `cyan-eval://` 自定义 scheme 导航通道回传
//! - `drive`：agent 工具高层 API（导航/快照/点击/输入），JS 采集逻辑引擎无关
//! - `page`：通用工具函数（base64 解码、截图目录），供 MCP image 落盘复用

pub mod drive;
pub mod page;
pub mod panel;
