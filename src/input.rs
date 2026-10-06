use crate::backend::js_err;
use crate::hid_profile::{pressed, profile};
use crate::nav::Action;
use leptos::{ev, prelude::*};
use serde::Deserialize;
use std::{cell::RefCell, collections::HashMap, time::Duration};
use wasm_bindgen::prelude::*;
use web_sys::KeyboardEvent;

// Calibration knobs: real sticks rest off-center and pads report at their own pace.
const STICK_DEADZONE: f64 = 0.5;
const GAMEPAD_POLL: Duration = Duration::from_millis(16);

/// Arrows, Enter, Backspace. Ignored while typing in a text field.
pub fn keyboard(on: impl Fn(Action) + 'static) {
    let _ = window_event_listener(ev::keydown, move |e: KeyboardEvent| {
        if e.alt_key() || e.ctrl_key() || e.meta_key() {
            return;
        }
        let tag = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .map(|el| el.tag_name())
            .unwrap_or_default();
        if matches!(tag.as_str(), "INPUT" | "TEXTAREA" | "SELECT") {
            return;
        }
        let a = match e.key().as_str() {
            "ArrowLeft" => Action::Left,
            "ArrowRight" => Action::Right,
            "Enter" => Action::Confirm,
            "Backspace" => Action::Back,
            _ => return,
        };
        // A focused button handles Enter natively (click).
        if tag == "BUTTON" && a == Action::Confirm {
            return;
        }
        // Held Enter/Backspace must not repeat: it would launch games repeatedly.
        if e.repeat() && matches!(a, Action::Confirm | Action::Back) {
            e.prevent_default();
            return;
        }
        e.prevent_default();
        on(a);
    });
}

/// Standard-mapping gamepads: d-pad/left stick, A = Confirm, B = Back. Fires on press edge.
// ponytail: no hold-to-repeat; add if scrolling long lists by holding feels needed.
pub fn gamepad(on: impl Fn(Action) + 'static) {
    const ORDER: [Action; 4] = [Action::Left, Action::Right, Action::Confirm, Action::Back];
    let prev = RefCell::new([false; 4]);
    set_interval(
        move || {
            let now = read_gamepads();
            let was = prev.replace(now);
            for i in 0..4 {
                if now[i] && !was[i] {
                    on(ORDER[i]);
                }
            }
        },
        GAMEPAD_POLL,
    );
}

fn read_gamepads() -> [bool; 4] {
    let mut s = [false; 4];
    let Ok(pads) = web_sys::window().unwrap().navigator().get_gamepads() else {
        return s;
    };
    for p in pads.iter() {
        let Ok(p) = p.dyn_into::<web_sys::Gamepad>() else { continue };
        let buttons = p.buttons();
        let btn = |i: u32| {
            buttons
                .get(i)
                .dyn_into::<web_sys::GamepadButton>()
                .is_ok_and(|b| b.pressed())
        };
        let x = p.axes().get(0).as_f64().unwrap_or(0.0);
        s[0] |= btn(14) || x < -STICK_DEADZONE;
        s[1] |= btn(15) || x > STICK_DEADZONE;
        s[2] |= btn(0);
        s[3] |= btn(1);
    }
    s
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HidInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
}

#[wasm_bindgen(module = "/src/js/platform.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = hidOpen)]
    async fn hid_open(ask: bool, on_report: &JsValue) -> Result<JsValue, JsValue>;
}

/// Open Yuancon HID devices. `ask` shows the chooser (needs a click); otherwise reopens granted ones.
/// `on_raw` gets every report as hex, for building profiles.
pub async fn hid(ask: bool, on: impl Fn(Action) + 'static, on_raw: impl Fn(String) + 'static) -> Result<Option<HidInfo>, String> {
    let prev = RefCell::new(HashMap::<(u16, u16, u8), Vec<u8>>::new());
    let cb = Closure::<dyn Fn(u16, u16, u8, js_sys::Uint8Array)>::new(move |vid, pid, rid, data: js_sys::Uint8Array| {
        let now = data.to_vec();
        let mut prev = prev.borrow_mut();
        let last = prev.entry((vid, pid, rid)).or_default();
        if *last == now {
            return; // pads can stream identical reports at 1 kHz
        }
        let hex: Vec<String> = now.iter().map(|b| format!("{b:02x}")).collect();
        on_raw(format!("{vid:04x}:{pid:04x} #{rid}  {}", hex.join(" ")));
        for a in pressed(profile(vid, pid), rid, last, &now) {
            on(a);
        }
        *last = now;
    })
    .into_js_value();
    let v = hid_open(ask, &cb).await.map_err(js_err)?;
    Ok(serde_wasm_bindgen::from_value(v).ok())
}
