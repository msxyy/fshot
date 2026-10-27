use winit::window::WindowId;

use crate::config::SettingsDraft;
use crate::hotkey::HotkeyAction;

#[derive(Debug, Clone)]
pub enum UserEvent {
    /// 全局热键触发（截图 / 窗口截图）
    Hotkey(HotkeyAction),
    TrayAction(TrayAction),
    /// 在哪个覆盖层窗口里按下了确认键（多屏时合并所有选区）
    CaptureConfirmed {
        window: WindowId,
    },
    CaptureCancelled,
    /// 设置窗口请求应用草稿
    ApplySettings(SettingsDraft),
    #[allow(dead_code)]
    PinClosed(WindowId),
}

#[derive(Debug, Clone, Copy)]
pub enum TrayAction {
    Capture,
    CaptureWindow,
    Settings,
    Quit,
}
