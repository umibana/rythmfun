use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/src/vendor/foley.js")]
extern "C" {
    #[wasm_bindgen(js_name = play)]
    fn foley_play(name: &str, opts: &JsValue) -> JsValue;
}

/// Play a Foley cue. `pan` is stereo placement, -1 (left) to 1 (right).
pub fn play(cue: &str, pan: f64) {
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"pan".into(), &pan.into());
    foley_play(cue, &opts);
}
