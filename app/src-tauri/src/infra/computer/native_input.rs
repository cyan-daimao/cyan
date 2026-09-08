//! OS 级真实输入注入（macOS CGEvent；非 macOS 返回不支持错误）。
//! 浏览器面板 real_* 动作与 ComputerAction 桌面操作共用本模块。

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
pub use fallback::*;

#[cfg(target_os = "macos")]
mod macos {
    use core_graphics::event::{
        CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGKeyCode, CGMouseButton,
        ScrollEventUnit,
    };
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;
    use std::time::Duration;

    fn source() -> Result<CGEventSource, String> {
        CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| "创建 CGEvent 事件源失败".to_string())
    }

    /// 具名键 → macOS 虚拟键码
    fn keycode(key: &str) -> Result<CGKeyCode, String> {
        match key.to_ascii_lowercase().as_str() {
            "enter" | "return" => Ok(36),
            "tab" => Ok(48),
            "backspace" | "delete" => Ok(51),
            "escape" | "esc" => Ok(53),
            "left" => Ok(123),
            "right" => Ok(124),
            "down" => Ok(125),
            "up" => Ok(126),
            other => Err(format!("不支持的按键：{other}（支持 enter/tab/escape/backspace/left/right/up/down）")),
        }
    }

    fn post(ev: CGEvent) {
        ev.post(CGEventTapLocation::Session);
    }

    /// 移动 + 指定键按下/抬起（down/up 间留小间隔，慢响应 UI 也能识别为完整点击）
    fn post_click_button(point: (f64, f64), button: CGMouseButton) -> Result<(), String> {
        let p = CGPoint::new(point.0, point.1);
        let src = source()?;
        let (down_ty, up_ty) = match button {
            CGMouseButton::Left => (CGEventType::LeftMouseDown, CGEventType::LeftMouseUp),
            _ => (CGEventType::RightMouseDown, CGEventType::RightMouseUp),
        };
        let mv = CGEvent::new_mouse_event(src.clone(), CGEventType::MouseMoved, p, button)
            .map_err(|_| "创建鼠标事件失败".to_string())?;
        post(mv);
        std::thread::sleep(Duration::from_millis(30));
        let down = CGEvent::new_mouse_event(src.clone(), down_ty, p, button)
            .map_err(|_| "创建鼠标事件失败".to_string())?;
        post(down);
        std::thread::sleep(Duration::from_millis(50));
        let up = CGEvent::new_mouse_event(src, up_ty, p, button)
            .map_err(|_| "创建鼠标事件失败".to_string())?;
        post(up);
        Ok(())
    }

    /// 移动 + 左键点击
    pub fn post_click(point: (f64, f64)) -> Result<(), String> {
        post_click_button(point, CGMouseButton::Left)
    }

    /// 移动 + 右键点击（呼出上下文菜单）
    pub fn post_right_click(point: (f64, f64)) -> Result<(), String> {
        post_click_button(point, CGMouseButton::Right)
    }

    /// 移动 + 左键双击
    pub fn post_double_click(point: (f64, f64)) -> Result<(), String> {
        post_click(point)?;
        std::thread::sleep(Duration::from_millis(80));
        post_click(point)
    }

    /// 滚轮滚动（行单位；dy>0 查看下方内容，dx>0 查看右方内容）
    pub fn post_scroll(dx: i32, dy: i32) -> Result<(), String> {
        let src = source()?;
        // CG 滚轮语义：wheel1>0 = 内容向下（查看上方），与“dy>0 查看下方”相反，取负
        let ev = CGEvent::new_scroll_event(src, ScrollEventUnit::LINE, 2, -dy, -dx, 0)
            .map_err(|_| "创建滚轮事件失败".to_string())?;
        post(ev);
        Ok(())
    }

    /// Unicode 文本注入：set_string 按字符走 IME 无关的文本通道（支持中文）
    pub fn post_text(text: &str) -> Result<(), String> {
        let src = source()?;
        for ch in text.chars() {
            let mut buf = [0u16; 2];
            let encoded = ch.encode_utf16(&mut buf);
            let down = CGEvent::new_keyboard_event(src.clone(), 0, true)
                .map_err(|_| "创建键盘事件失败".to_string())?;
            down.set_string_from_utf16_unchecked(encoded);
            post(down);
            let up = CGEvent::new_keyboard_event(src.clone(), 0, false)
                .map_err(|_| "创建键盘事件失败".to_string())?;
            post(up);
            std::thread::sleep(Duration::from_millis(8));
        }
        Ok(())
    }

    /// 具名键（down + up）
    pub fn post_key(key: &str) -> Result<(), String> {
        let code = keycode(key)?;
        let src = source()?;
        let down = CGEvent::new_keyboard_event(src.clone(), code, true)
            .map_err(|_| "创建键盘事件失败".to_string())?;
        post(down);
        std::thread::sleep(Duration::from_millis(20));
        let up = CGEvent::new_keyboard_event(src, code, false)
            .map_err(|_| "创建键盘事件失败".to_string())?;
        post(up);
        Ok(())
    }

    /// Cmd+A（全选）
    pub fn post_select_all() -> Result<(), String> {
        let src = source()?;
        let down = CGEvent::new_keyboard_event(src.clone(), 0, true)
            .map_err(|_| "创建键盘事件失败".to_string())?;
        down.set_flags(CGEventFlags::CGEventFlagCommand);
        post(down);
        std::thread::sleep(Duration::from_millis(20));
        let up = CGEvent::new_keyboard_event(src, 0, false)
            .map_err(|_| "创建键盘事件失败".to_string())?;
        up.set_flags(CGEventFlags::CGEventFlagCommand);
        post(up);
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
mod fallback {
    /// 移动 + 左键点击（非 macOS 暂不支持）
    pub fn post_click(_point: (f64, f64)) -> Result<(), String> {
        Err("原生输入注入目前仅支持 macOS".to_string())
    }

    pub fn post_right_click(_point: (f64, f64)) -> Result<(), String> {
        Err("原生输入注入目前仅支持 macOS".to_string())
    }

    pub fn post_double_click(_point: (f64, f64)) -> Result<(), String> {
        Err("原生输入注入目前仅支持 macOS".to_string())
    }

    pub fn post_scroll(_dx: i32, _dy: i32) -> Result<(), String> {
        Err("原生输入注入目前仅支持 macOS".to_string())
    }

    pub fn post_text(_text: &str) -> Result<(), String> {
        Err("原生输入注入目前仅支持 macOS".to_string())
    }

    pub fn post_key(_key: &str) -> Result<(), String> {
        Err("原生输入注入目前仅支持 macOS".to_string())
    }

    pub fn post_select_all() -> Result<(), String> {
        Err("原生输入注入目前仅支持 macOS".to_string())
    }
}
