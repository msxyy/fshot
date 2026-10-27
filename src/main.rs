mod annotate;
mod app;
mod capture;
mod config;
mod event;
mod hotkey;
mod ocr;
mod output;
mod overlay;
mod pin;
mod tray;

use anyhow::Result;
use event::{TrayAction, UserEvent};
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_hal=warn,wgpu_core=warn,naga=warn"),
    )
    .init();

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();
    let open_settings = std::env::args().any(|arg| arg == "--settings");
    let mut app = app::App::new(proxy.clone());
    if open_settings {
        // 允许从命令行直接打开设置，也方便快捷方式/测试。
        let _ = proxy.send_event(UserEvent::TrayAction(TrayAction::Settings));
    }

    event_loop.run_app(&mut app)?;
    Ok(())
}
