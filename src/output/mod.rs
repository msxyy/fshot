use anyhow::Result;
use image::{Rgba, RgbaImage};
use std::path::PathBuf;

/// 把多块选区按虚拟桌面物理坐标拼成一张图；空白处保持透明。
pub fn merge_images(parts: &[(i32, i32, RgbaImage)]) -> RgbaImage {
    if parts.is_empty() {
        return RgbaImage::new(1, 1);
    }
    if parts.len() == 1 {
        return parts[0].2.clone();
    }

    let min_x = parts.iter().map(|(x, _, _)| *x as i64).min().unwrap_or(0);
    let min_y = parts.iter().map(|(_, y, _)| *y as i64).min().unwrap_or(0);
    let max_x = parts
        .iter()
        .map(|(x, _, img)| *x as i64 + img.width() as i64)
        .max()
        .unwrap_or(0);
    let max_y = parts
        .iter()
        .map(|(_, y, img)| *y as i64 + img.height() as i64)
        .max()
        .unwrap_or(0);

    let w = (max_x - min_x).max(1) as u64;
    let h = (max_y - min_y).max(1) as u64;
    // 防止异常选区导致超大内存分配；失败时退回第一块。
    if w > 32768 || h > 32768 || w.saturating_mul(h) > 200_000_000 {
        log::warn!("merged canvas {w}x{h} too large; falling back to first selection");
        return parts[0].2.clone();
    }

    let mut canvas = RgbaImage::from_pixel(w as u32, h as u32, Rgba([0, 0, 0, 0]));
    for (x, y, img) in parts {
        let dx = *x as i64 - min_x;
        let dy = *y as i64 - min_y;
        image::imageops::overlay(&mut canvas, img, dx, dy);
    }
    canvas
}

pub fn copy_to_clipboard(img: &RgbaImage) -> Result<()> {
    let mut cb = arboard::Clipboard::new()?;
    cb.set_image(arboard::ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: img.as_raw().clone().into(),
    })?;
    Ok(())
}

pub fn save_png(img: &RgbaImage, dir: &str) -> Result<PathBuf> {
    let dir_path = PathBuf::from(dir);
    std::fs::create_dir_all(&dir_path)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    // 毫秒级时间戳仍可能碰撞（连续保存），存在时追加序号
    let mut path = dir_path.join(format!("fshot-{ts}.png"));
    let mut seq = 1u32;
    while path.exists() {
        path = dir_path.join(format!("fshot-{ts}-{seq}.png"));
        seq += 1;
    }

    img.save(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_png_never_overwrites_by_name() {
        let dir = std::env::temp_dir().join(format!("fshot-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let img = RgbaImage::new(2, 2);

        let a = save_png(&img, dir.to_str().unwrap()).unwrap();
        let b = save_png(&img, dir.to_str().unwrap()).unwrap();
        assert_ne!(a, b);
        assert!(a.exists() && b.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_places_selections_at_virtual_desktop_coords() {
        let red = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        let blue = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 255, 255]));

        let merged = merge_images(&[(0, 0, red), (10, 0, blue)]);
        assert_eq!(merged.dimensions(), (12, 2));
        assert_eq!(merged.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
        assert_eq!(merged.get_pixel(10, 0), &Rgba([0, 0, 255, 255]));
        assert_eq!(merged.get_pixel(5, 0), &Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn merge_handles_negative_coordinates() {
        let a = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
        let b = RgbaImage::from_pixel(3, 4, Rgba([4, 5, 6, 255]));
        let merged = merge_images(&[(-5, 0, a), (0, 0, b)]);
        assert_eq!(merged.dimensions(), (8, 4));
        assert_eq!(merged.get_pixel(0, 0), &Rgba([1, 2, 3, 255]));
        assert_eq!(merged.get_pixel(5, 0), &Rgba([4, 5, 6, 255]));
    }

    #[test]
    fn merge_single_returns_original() {
        let img = RgbaImage::from_pixel(3, 5, Rgba([9, 8, 7, 255]));
        let merged = merge_images(&[(100, -2, img)]);
        assert_eq!(merged.dimensions(), (3, 5));
    }
}
