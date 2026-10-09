use crate::backend::js_err;
use crate::hid_profile::{pressed, profile};
use crate::nav::Action;
use crate::controller_binding::ControllerBinding;
use leptos::{ev, prelude::*};
use serde::Deserialize;
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};
use wasm_bindgen::prelude::*;
use web_sys::KeyboardEvent;

// Calibration knobs: real sticks rest off-center and pads report at their own pace.
const STICK_DEADZONE: f64 = 0.5;
const GAMEPAD_POLL: Duration = Duration::from_millis(16);

/// Arrows, Enter, Backspace. Ignored while typing in a text field.
pub fn keyboard(on: impl Fn(Action) + 'static, vertical_home: impl Fn() -> bool + 'static) {
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
        let Some(a) = crate::nav::keyboard_action(&e.key(), vertical_home()) else { return; };
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
pub fn gamepad(on: impl Fn(Action) + 'static, on_devices: impl Fn(Vec<ControllerBinding>) + 'static, vertical_home: impl Fn() -> bool + 'static) {
    const ORDER: [Action; 4] = [Action::Left, Action::Right, Action::Confirm, Action::Back];
    let prev = RefCell::new([false; 4]);
    let devices = RefCell::new(Vec::new());
    set_interval(
        move || {
            let connected = gamepad_devices();
            if *devices.borrow() != connected {
                *devices.borrow_mut() = connected.clone();
                on_devices(connected);
            }
            let now = read_gamepads(vertical_home());
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

fn gamepad_devices() -> Vec<ControllerBinding> {
    let mut devices = Vec::new();
    if let Ok(pads) = window().navigator().get_gamepads() {
        for p in pads.iter().filter_map(|p| p.dyn_into::<web_sys::Gamepad>().ok()) {
            if !p.connected() { continue; }
            let binding = ControllerBinding { id: format!("gamepad:{}", p.id()), name: p.id() };
            if !devices.contains(&binding) { devices.push(binding); }
        }
    }
    devices
}

fn read_gamepads(vertical_home: bool) -> [bool; 4] {
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
        if vertical_home {
            let y = p.axes().get(1).as_f64().unwrap_or(0.0);
            s[0] |= btn(12) || y < -STICK_DEADZONE;
            s[1] |= btn(13) || y > STICK_DEADZONE;
        }
        s[2] |= btn(0);
        s[3] |= btn(1);
    }
    s
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HidInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
}

#[wasm_bindgen(module = "/src/js/platform.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = hidOpen)]
    async fn hid_open(ask: bool, on_report: &JsValue, on_action: &JsValue, on_device: &JsValue, on_status: &JsValue) -> Result<JsValue, JsValue>;
}

/// Desktop uses native input. Browser dev uses granted Yuancon WebHID devices.
/// `on_raw` gets every report as hex, for building profiles.
pub async fn hid(ask: bool, on: impl Fn(Action) + 'static, on_raw: impl Fn(String) + 'static,
    on_device: impl Fn(Vec<HidInfo>) + 'static, on_status: impl Fn(String) + 'static,
) -> Result<Option<HidInfo>, String> {
    let on = Rc::new(on);
    let on_native = on.clone();
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
    let action_cb = Closure::<dyn Fn(String)>::new(move |action: String| {
        let action = match action.as_str() {
            "left" => Action::Left, "right" => Action::Right,
            "back" => Action::Back, "confirm" => Action::Confirm,
            _ => return,
        };
        on_native(action);
    }).into_js_value();
    let device_cb = Closure::<dyn Fn(JsValue)>::new(move |device: JsValue| {
        // Browser WebHID reports all devices; native WinUSB currently reports one.
        let devices = serde_wasm_bindgen::from_value::<Vec<HidInfo>>(device.clone())
            .unwrap_or_else(|_| serde_wasm_bindgen::from_value::<Option<HidInfo>>(device).ok().flatten().into_iter().collect());
        on_device(devices);
    }).into_js_value();
    let status_cb = Closure::<dyn Fn(String)>::new(move |status: String| on_status(status)).into_js_value();
    let v = hid_open(ask, &cb, &action_cb, &device_cb, &status_cb).await.map_err(js_err)?;
    Ok(serde_wasm_bindgen::from_value(v).ok())
}
