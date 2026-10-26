use anyhow::Result;
use image::RgbaImage;
use std::path::PathBuf;

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
}