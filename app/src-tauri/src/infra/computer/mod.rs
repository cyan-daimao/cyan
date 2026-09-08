//! 桌面级 computer-use：主屏截图（视觉快照）+ 全局输入注入。
//! 与浏览器面板方案平行：面板操作网页，本模块操作屏幕上任意应用。
//! 坐标约定：模型在缩放后的截图坐标系给 (x, y)，本模块按最近一次截图的
//! 映射上下文换算为屏幕逻辑点（CGEvent 全局坐标，主屏左上角原点）。

pub mod native_input;

use std::sync::Mutex;

/// 模型坐标系最长边（超过则等比缩小，控制图片 token 开销）
const MAX_MODEL_EDGE: u32 = 1568;

/// 最近一次截图的坐标映射上下文（ComputerSnapshot 更新，ComputerAction 消费）
struct CaptureCtx {
    /// 模型坐标系尺寸（缩放后）
    model_w: u32,
    model_h: u32,
    /// 主屏逻辑尺寸（CGEvent 坐标空间，点）
    logical_w: f64,
    logical_h: f64,
}

static LAST_CAPTURE: Mutex<Option<CaptureCtx>> = Mutex::new(None);

/// 一次截图的产物
pub struct Snapshot {
    /// 缩放后 PNG（模型坐标系）
    pub png: Vec<u8>,
    /// 模型坐标系宽
    pub model_w: u32,
    /// 模型坐标系高
    pub model_h: u32,
    /// 主屏物理像素尺寸
    pub phys_w: u32,
    pub phys_h: u32,
    /// 主屏逻辑尺寸
    pub logical_w: f64,
    pub logical_h: f64,
}

/// 主屏逻辑尺寸（点）。macOS 取 CGDisplay 主屏 bounds；非 macOS 不支持
#[cfg(target_os = "macos")]
fn main_display_logical_size() -> Result<(f64, f64), String> {
    let display = core_graphics::display::CGDisplay::main();
    let bounds = display.bounds();
    Ok((bounds.size.width, bounds.size.height))
}

#[cfg(not(target_os = "macos"))]
fn main_display_logical_size() -> Result<(f64, f64), String> {
    Err("桌面截屏目前仅支持 macOS".to_string())
}

/// 截取主屏原始 PNG（物理像素）。macOS 走系统 screencapture（自带权限引导，
/// 授权归因到本应用）；仅主屏（-D 1），避免多屏拼接坐标系歧义
#[cfg(target_os = "macos")]
async fn capture_main_screen() -> Result<Vec<u8>, String> {
    let tmp = std::env::temp_dir().join(format!("cyan-screen-{}.png", std::process::id()));
    let out = tokio::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-t", "png", "-D", "1"])
        .arg(&tmp)
        .output()
        .await
        .map_err(|e| format!("启动 screencapture 失败：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "screencapture 失败：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let png = std::fs::read(&tmp).map_err(|e| format!("读取截图失败：{e}"))?;
    let _ = std::fs::remove_file(&tmp);
    if png.is_empty() {
        return Err("截图为空（可能未授予屏幕录制权限：系统设置 → 隐私与安全性 → 屏幕录制）".into());
    }
    Ok(png)
}

#[cfg(not(target_os = "macos"))]
async fn capture_main_screen() -> Result<Vec<u8>, String> {
    Err("桌面截屏目前仅支持 macOS".to_string())
}

/// 截取主屏并缩放到模型坐标系；更新坐标映射上下文。
/// 返回 Snapshot；调用方负责 base64 编码与落盘
pub async fn snapshot() -> Result<Snapshot, String> {
    let raw = capture_main_screen().await?;
    let (logical_w, logical_h) = main_display_logical_size()?;
    let img = image::load_from_memory(&raw).map_err(|e| format!("解析截图失败：{e}"))?;
    let (phys_w, phys_h) = (img.width(), img.height());
    // 等比缩小到最长边 MAX_MODEL_EDGE（小屏不放大）
    let scale = (MAX_MODEL_EDGE as f64 / phys_w.max(phys_h) as f64).min(1.0);
    let model_w = ((phys_w as f64) * scale).round().max(1.0) as u32;
    let model_h = ((phys_h as f64) * scale).round().max(1.0) as u32;
    let resized = if scale < 1.0 {
        image::imageops::resize(&img, model_w, model_h, image::imageops::FilterType::Triangle)
    } else {
        img.to_rgba8()
    };
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(resized)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("编码截图失败：{e}"))?;
    *LAST_CAPTURE.lock().map_err(|_| "截图上下文锁中毒")? = Some(CaptureCtx {
        model_w,
        model_h,
        logical_w,
        logical_h,
    });
    Ok(Snapshot {
        png,
        model_w,
        model_h,
        phys_w,
        phys_h,
        logical_w,
        logical_h,
    })
}

/// 模型坐标系 (x, y) → 屏幕逻辑点（CGEvent 全局坐标）。
/// 无截图上下文时返回引导错误；越界硬校验（防模型幻坐标打到不可预期位置）
pub fn model_to_screen(x: f64, y: f64) -> Result<(f64, f64), String> {
    let guard = LAST_CAPTURE.lock().map_err(|_| "截图上下文锁中毒")?;
    let ctx = guard
        .as_ref()
        .ok_or_else(|| "尚未截图：请先调用 ComputerSnapshot 获取屏幕画面与坐标系".to_string())?;
    if x < 0.0 || y < 0.0 || x > ctx.model_w as f64 || y > ctx.model_h as f64 {
        return Err(format!(
            "坐标越界：({x}, {y}) 不在截图坐标系 0..{} × 0..{} 内",
            ctx.model_w, ctx.model_h
        ));
    }
    Ok((
        x * ctx.logical_w / ctx.model_w as f64,
        y * ctx.logical_h / ctx.model_h as f64,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_without_snapshot_is_guided_error() {
        // 测试环境无法截图（无窗口服务器依赖），LAST_CAPTURE 必为空
        let err = model_to_screen(10.0, 10.0).unwrap_err();
        assert!(err.contains("ComputerSnapshot"));
    }
}
