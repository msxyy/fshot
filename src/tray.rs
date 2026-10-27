use anyhow::Result;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use winit::event_loop::EventLoopProxy;

use crate::event::{TrayAction, UserEvent};
use crate::hotkey::HotkeyAction;

pub struct Tray {
    _icon: TrayIcon,
}

impl Tray {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let menu = Menu::new();
        let item_capture = MenuItem::new("截图", true, None);
        let item_window = MenuItem::new("窗口截图", true, None);
        let item_settings = MenuItem::new("设置…", true, None);
        let item_sep = PredefinedMenuItem::separator();
        let item_quit = MenuItem::new("退出", true, None);

        let id_capture = item_capture.id().clone();
        let id_window = item_window.id().clone();
        let id_settings = item_settings.id().clone();
        let id_quit = item_quit.id().clone();

        menu.append_items(&[
            &item_capture,
            &item_window,
            &item_settings,
            &item_sep,
            &item_quit,
        ])?;

        let icon = build_icon()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("fshot — 轻量截图")
            .with_icon(icon)
            .build()?;

        // 菜单事件（全局静态回调）
        let p_menu = proxy.clone();
        MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
            let action = if ev.id == id_capture {
                Some(UserEvent::TrayAction(TrayAction::Capture))
            } else if ev.id == id_window {
                Some(UserEvent::TrayAction(TrayAction::CaptureWindow))
            } else if ev.id == id_settings {
                Some(UserEvent::TrayAction(TrayAction::Settings))
            } else if ev.id == id_quit {
                Some(UserEvent::TrayAction(TrayAction::Quit))
            } else {
                None
            };
            if let Some(a) = action {
                let _ = p_menu.send_event(a);
            }
        }));

        // 托盘图标左键单击 → 直接截图
        let p_click = proxy;
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            if matches!(
                ev,
                TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                }
            ) {
                let _ = p_click.send_event(UserEvent::Hotkey(HotkeyAction::Capture));
            }
        }));

        Ok(Self { _icon: tray })
    }
}

/// 程序内生成一个简单的红点图标（免资源文件）
fn build_icon() -> Result<tray_icon::Icon> {
    let size = 32u32;
    let cx = size as f32 / 2.0;
    let cy = size as f32 / 2.0;
    let r = size as f32 * 0.40;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d <= r {
                rgba.extend_from_slice(&[230, 80, 80, 255]);
            } else if d <= r + 1.5 {
                // 抗锯齿边
                let a = ((r + 1.5 - d) / 1.5 * 255.0) as u8;
                rgba.extend_from_slice(&[230, 80, 80, a]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    Ok(tray_icon::Icon::from_rgba(rgba, size, size)?)
}
