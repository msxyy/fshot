use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Result};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use winit::event_loop::EventLoopProxy;

use crate::event::UserEvent;

/// 热键对应的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    Capture,
    WindowCapture,
}

pub struct HotkeyManager {
    manager: GlobalHotKeyManager,
    bindings: Arc<Mutex<HashMap<u32, HotkeyAction>>>,
    capture: HotKey,
    window: HotKey,
}

impl HotkeyManager {
    pub fn new(
        capture_spec: &str,
        window_spec: &str,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self> {
        let capture = parse_hotkey(capture_spec)?;
        let window = parse_hotkey(window_spec)?;
        check_distinct(capture, window)?;

        let manager = GlobalHotKeyManager::new()?;
        manager.register_all(&[capture, window])?;

        let bindings = Arc::new(Mutex::new(HashMap::new()));
        set_binding(&bindings, capture.id, HotkeyAction::Capture);
        set_binding(&bindings, window.id, HotkeyAction::WindowCapture);
        spawn_listener(bindings.clone(), proxy)?;

        Ok(Self {
            manager,
            bindings,
            capture,
            window,
        })
    }

    /// 运行时重新绑定两个热键。先注册新热键，失败时保留旧绑定。
    pub fn rebind(&mut self, capture_spec: &str, window_spec: &str) -> Result<()> {
        let new_capture = parse_hotkey(capture_spec)?;
        let new_window = parse_hotkey(window_spec)?;
        check_distinct(new_capture, new_window)?;

        // HotKey::new 每次都会分配新 id；组合未变时必须复用旧 HotKey，
        // 否则会重复注册同一个组合而失败。
        let (capture, window) = plan_rebind(self.capture, self.window, new_capture, new_window);

        let old = [self.capture, self.window];
        let new = [capture, window];
        let old_ids: HashSet<u32> = old.iter().map(|h| h.id).collect();
        let new_ids: HashSet<u32> = new.iter().map(|h| h.id).collect();

        let mut registered = Vec::new();
        for hotkey in new {
            if old_ids.contains(&hotkey.id) {
                continue;
            }
            if let Err(e) = self.manager.register(hotkey) {
                for done in registered {
                    let _ = self.manager.unregister(done);
                }
                return Err(anyhow!(
                    "热键 {hotkey} 注册失败（可能已被其他程序占用）: {e}"
                ));
            }
            registered.push(hotkey);
        }

        for hotkey in old {
            if !new_ids.contains(&hotkey.id) {
                if let Err(e) = self.manager.unregister(hotkey) {
                    log::warn!("unregister {hotkey} failed: {e}");
                }
            }
        }

        if let Ok(mut b) = self.bindings.lock() {
            b.clear();
            b.insert(capture.id, HotkeyAction::Capture);
            b.insert(window.id, HotkeyAction::WindowCapture);
        }
        self.capture = capture;
        self.window = window;
        Ok(())
    }
}

fn set_binding(bindings: &Arc<Mutex<HashMap<u32, HotkeyAction>>>, id: u32, action: HotkeyAction) {
    if let Ok(mut b) = bindings.lock() {
        b.insert(id, action);
    }
}

fn check_distinct(capture: HotKey, window: HotKey) -> Result<()> {
    if same_combo(&capture, &window) {
        bail!("截图热键和窗口截图热键不能相同");
    }
    Ok(())
}

fn same_combo(a: &HotKey, b: &HotKey) -> bool {
    a.mods == b.mods && a.key == b.key
}

/// 组合不变时复用旧 HotKey（保留已注册的 id），支持两个热键互换。
fn plan_rebind(
    old_capture: HotKey,
    old_window: HotKey,
    new_capture: HotKey,
    new_window: HotKey,
) -> (HotKey, HotKey) {
    let capture = if same_combo(&new_capture, &old_capture) {
        old_capture
    } else if same_combo(&new_capture, &old_window) {
        old_window
    } else {
        new_capture
    };
    let window = if same_combo(&new_window, &old_window) {
        old_window
    } else if same_combo(&new_window, &old_capture) {
        old_capture
    } else {
        new_window
    };
    (capture, window)
}

fn spawn_listener(
    bindings: Arc<Mutex<HashMap<u32, HotkeyAction>>>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<()> {
    std::thread::Builder::new()
        .name("fshot-hotkey".into())
        .spawn(move || {
            let rx = GlobalHotKeyEvent::receiver();
            while let Ok(ev) = rx.recv() {
                if ev.state != HotKeyState::Pressed {
                    continue;
                }
                let action = bindings.lock().ok().and_then(|m| m.get(&ev.id).copied());
                if let Some(action) = action {
                    if proxy.send_event(UserEvent::Hotkey(action)).is_err() {
                        break;
                    }
                }
            }
        })?;
    Ok(())
}

fn parse_hotkey(spec: &str) -> Result<HotKey> {
    let (mods, code) =
        parse_spec(spec).ok_or_else(|| anyhow!("无效热键格式：{spec}（示例：Ctrl+Shift+S）"))?;
    Ok(HotKey::new(Some(mods), code))
}

fn parse_spec(spec: &str) -> Option<(Modifiers, Code)> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in spec.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "alt" => mods |= Modifiers::ALT,
            "super" | "win" | "meta" => mods |= Modifiers::SUPER,
            other => {
                let parsed = match other {
                    "space" => Some(Code::Space),
                    "tab" => Some(Code::Tab),
                    "enter" | "return" => Some(Code::Enter),
                    "escape" | "esc" => Some(Code::Escape),
                    "backspace" => Some(Code::Backspace),
                    "f1" => Some(Code::F1),
                    "f2" => Some(Code::F2),
                    "f3" => Some(Code::F3),
                    "f4" => Some(Code::F4),
                    "f5" => Some(Code::F5),
                    "f6" => Some(Code::F6),
                    "f7" => Some(Code::F7),
                    "f8" => Some(Code::F8),
                    "f9" => Some(Code::F9),
                    "f10" => Some(Code::F10),
                    "f11" => Some(Code::F11),
                    "f12" => Some(Code::F12),
                    _ => {
                        let s = other.to_string();
                        if s.len() == 1 {
                            let c = s.chars().next().unwrap().to_ascii_uppercase();
                            if c.is_ascii_alphabetic() {
                                Some(match c {
                                    'A' => Code::KeyA,
                                    'B' => Code::KeyB,
                                    'C' => Code::KeyC,
                                    'D' => Code::KeyD,
                                    'E' => Code::KeyE,
                                    'F' => Code::KeyF,
                                    'G' => Code::KeyG,
                                    'H' => Code::KeyH,
                                    'I' => Code::KeyI,
                                    'J' => Code::KeyJ,
                                    'K' => Code::KeyK,
                                    'L' => Code::KeyL,
                                    'M' => Code::KeyM,
                                    'N' => Code::KeyN,
                                    'O' => Code::KeyO,
                                    'P' => Code::KeyP,
                                    'Q' => Code::KeyQ,
                                    'R' => Code::KeyR,
                                    'S' => Code::KeyS,
                                    'T' => Code::KeyT,
                                    'U' => Code::KeyU,
                                    'V' => Code::KeyV,
                                    'W' => Code::KeyW,
                                    'X' => Code::KeyX,
                                    'Y' => Code::KeyY,
                                    'Z' => Code::KeyZ,
                                    _ => unreachable!(),
                                })
                            } else if c.is_ascii_digit() {
                                Some(match c {
                                    '0' => Code::Digit0,
                                    '1' => Code::Digit1,
                                    '2' => Code::Digit2,
                                    '3' => Code::Digit3,
                                    '4' => Code::Digit4,
                                    '5' => Code::Digit5,
                                    '6' => Code::Digit6,
                                    '7' => Code::Digit7,
                                    '8' => Code::Digit8,
                                    '9' => Code::Digit9,
                                    _ => unreachable!(),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                };
                if parsed.is_some() {
                    code = parsed;
                }
            }
        }
    }

    Some((mods, code?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_hotkeys() {
        assert!(parse_spec("Ctrl+Shift+S").is_some());
        assert!(parse_spec("alt+f4").is_some());
        assert!(parse_spec("Ctrl+Shift").is_none());
        assert!(parse_spec("Ctrl+Ctrl+S").is_some());
        assert!(parse_spec("").is_none());
    }

    #[test]
    fn duplicate_bindings_rejected() {
        let a = parse_hotkey("Ctrl+Shift+S").unwrap();
        let b = parse_hotkey("ctrl+shift+s").unwrap();
        assert!(check_distinct(a, b).is_err());
        let c = parse_hotkey("Ctrl+Shift+W").unwrap();
        assert!(check_distinct(a, c).is_ok());
    }

    #[test]
    fn rebind_keeps_ids_for_unchanged_and_swapped_combos() {
        let a = parse_hotkey("Ctrl+Shift+S").unwrap();
        let b = parse_hotkey("Ctrl+Shift+W").unwrap();
        let c = parse_hotkey("Ctrl+Alt+S").unwrap();

        let (capture, window) = plan_rebind(
            a,
            b,
            parse_hotkey("Ctrl+Shift+S").unwrap(),
            parse_hotkey("Ctrl+Shift+W").unwrap(),
        );
        assert_eq!((capture.id, window.id), (a.id, b.id));

        let (capture, window) = plan_rebind(
            a,
            b,
            parse_hotkey("Ctrl+Shift+W").unwrap(),
            parse_hotkey("Ctrl+Shift+S").unwrap(),
        );
        assert_eq!((capture.id, window.id), (b.id, a.id));

        let (capture, window) = plan_rebind(
            a,
            b,
            parse_hotkey("Ctrl+Alt+S").unwrap(),
            parse_hotkey("Ctrl+Shift+W").unwrap(),
        );
        assert_eq!(capture.id, c.id);
        assert_eq!(window.id, b.id);
    }
}
