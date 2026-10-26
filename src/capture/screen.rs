use anyhow::{anyhow, Result};
use image::RgbaImage;

pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: RgbaImage,
    pub monitor_x: i32,
    pub monitor_y: i32,
    /// 保留给 M6 混合 DPI 处理
    #[allow(dead_code)]
    pub scale: f32,
}

/// 抓取所有显示器，按系统枚举顺序返回
pub fn capture_all() -> Result<Vec<CapturedFrame>> {
    use xcap::Monitor;
    let monitors = Monitor::all()?;
    let mut out = Vec::new();
    for m in monitors {
        let pixels = m.capture_image()?;
        let width = pixels.width();
        let height = pixels.height();
        let monitor_x = m.x()?;
        let monitor_y = m.y()?;
        let scale = m.scale_factor()?;
        out.push(CapturedFrame {
            width,
            height,
            pixels,
            monitor_x,
            monitor_y,
            scale,
        });
    }
    if out.is_empty() {
        return Err(anyhow!("no monitor"));
    }
    Ok(out)
}