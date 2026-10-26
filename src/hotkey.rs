use anyhow::Result;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use winit::event_loop::EventLoopProxy;

use crate::event::UserEvent;

pub struct HotkeyManager {
    // 必须持有，drop 即注销热键
    _manager: GlobalHotKeyManager,
}

impl HotkeyManager {
    pub fn new(spec: &str, proxy: EventLoopProxy<UserEvent>) -> Result<Self> {
        let (mods, code) = parse_spec(spec)
            .ok_or_else(|| anyhow::anyhow!("invalid hotkey spec: {spec}"))?;

        let manager = GlobalHotKeyManager::new()?;
        let hotkey = HotKey::new(Some(mods), code);
        manager.register(hotkey)?;

        std::thread::Builder::new()
            .name("fshot-hotkey".into())
            .spawn(move || {
                let rx = GlobalHotKeyEvent::receiver();
                while rx.recv().is_ok() {
                    if proxy.send_event(UserEvent::HotkeyTriggered).is_err() {
                        break;
                    }
                }
            })?;

        Ok(Self { _manager: manager })
    }
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
                                    'A' => Code::KeyA, 'B' => Code::KeyB, 'C' => Code::KeyC,
                                    'D' => Code::KeyD, 'E' => Code::KeyE, 'F' => Code::KeyF,
                                    'G' => Code::KeyG, 'H' => Code::KeyH, 'I' => Code::KeyI,
                                    'J' => Code::KeyJ, 'K' => Code::KeyK, 'L' => Code::KeyL,
                                    'M' => Code::KeyM, 'N' => Code::KeyN, 'O' => Code::KeyO,
                                    'P' => Code::KeyP, 'Q' => Code::KeyQ, 'R' => Code::KeyR,
                                    'S' => Code::KeyS, 'T' => Code::KeyT, 'U' => Code::KeyU,
                                    'V' => Code::KeyV, 'W' => Code::KeyW, 'X' => Code::KeyX,
                                    'Y' => Code::KeyY, 'Z' => Code::KeyZ,
                                    _ => unreachable!(),
                                })
                            } else if c.is_ascii_digit() {
                                Some(match c {
                                    '0' => Code::Digit0, '1' => Code::Digit1, '2' => Code::Digit2,
                                    '3' => Code::Digit3, '4' => Code::Digit4, '5' => Code::Digit5,
                                    '6' => Code::Digit6, '7' => Code::Digit7, '8' => Code::Digit8,
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