use anyhow::{anyhow, Context, Result};
use xcap::Window;

use crate::capture::screen::CapturedFrame;

/// 截取当前前台窗口。热键触发时焦点仍在目标窗口上；
/// 托盘菜单触发时若菜单抢走焦点，则退回 z 序最上层的非本进程窗口。
pub fn capture_focused() -> Result<CapturedFrame> {
    let own_pid = std::process::id();
    let windows = Window::all().context("枚举窗口失败")?;

    let mut candidates = Vec::new();
    for w in windows {
        if w.pid().map(|pid| pid == own_pid).unwrap_or(false) {
            continue;
        }
        if w.is_minimized().unwrap_or(false) {
            continue;
        }
        let width = w.width().unwrap_or(0);
        let height = w.height().unwrap_or(0);
        if width < 2 || height < 2 {
            continue;
        }
        // 桌面/任务栏等 shell 窗口通常没有标题；窗口截图不应命中它们。
        if w.title().unwrap_or_default().trim().is_empty() {
            continue;
        }
        candidates.push(w);
    }

    let target = candidates
        .iter()
        .find(|w| w.is_focused().unwrap_or(false))
        .or_else(|| candidates.iter().max_by_key(|w| w.z().unwrap_or(i32::MIN)))
        .ok_or_else(|| anyhow!("没有可截取的窗口"))?;

    let title = target.title().unwrap_or_default();
    let app = target.app_name().unwrap_or_default();
    let x = target.x().unwrap_or(0);
    let y = target.y().unwrap_or(0);
    let pixels = target
        .capture_image()
        .with_context(|| format!("截取窗口失败：{app} / {title}"))?;

    let width = pixels.width();
    let height = pixels.height();
    if width < 2 || height < 2 {
        return Err(anyhow!("窗口图像为空：{app} / {title}"));
    }
    log::info!("window capture: {app} / {title} at ({x}, {y}) {width}x{height}");

    Ok(CapturedFrame {
        width,
        height,
        pixels,
        monitor_x: x,
        monitor_y: y,
        scale: 1.0,
    })
}
