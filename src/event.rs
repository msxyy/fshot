use winit::window::WindowId;

#[derive(Debug, Clone)]
pub enum UserEvent {
    HotkeyTriggered,
    TrayAction(TrayAction),
    /// 在哪个覆盖层窗口里按下了确认键（多屏时取该窗口的选区）
    CaptureConfirmed { window: WindowId },
    CaptureCancelled,
    #[allow(dead_code)]
    PinClosed(WindowId),
}

#[derive(Debug, Clone, Copy)]
pub enum TrayAction {
    Capture,
    Settings,
    Quit,
}