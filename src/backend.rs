use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub name: String,
    pub path: String,
    pub image: String,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen(module = "/src/js/platform.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readAsDataUrl)]
    async fn read_as_data_url_js(file: web_sys::File) -> Result<JsValue, JsValue>;
}

const STORAGE_KEY: &str = "games";

pub fn is_tauri() -> bool {
    js_sys::Reflect::has(&web_sys::window().unwrap(), &"__TAURI__".into()).unwrap_or(false)
}

pub fn js_err(e: JsValue) -> String {
    e.as_string()
        .or_else(|| e.dyn_ref::<js_sys::Error>().map(|e| String::from(e.message())))
        .unwrap_or_else(|| format!("{e:?}"))
}

fn storage() -> web_sys::Storage {
    web_sys::window().unwrap().local_storage().unwrap().unwrap()
}

async fn call(cmd: &str, args: impl Serialize) -> Result<JsValue, String> {
    let args = serde_wasm_bindgen::to_value(&args).map_err(|e| e.to_string())?;
    invoke(cmd, args).await.map_err(js_err)
}

#[derive(Serialize)]
struct NoArgs {}

pub async fn load_games() -> Result<Vec<Game>, String> {
    if is_tauri() {
        let v = call("load_games", NoArgs {}).await?;
        serde_wasm_bindgen::from_value(v).map_err(|e| e.to_string())
    } else {
        let saved: Vec<Game> = storage()
            .get_item(STORAGE_KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Ok(if saved.is_empty() { demo_games() } else { saved })
    }
}

/// Browser dev only: sample games shown while the saved list is empty, so the UI has content.
fn demo_games() -> Vec<Game> {
    [
        ("Taiko no Tatsujin", "taiko"),
        ("osu!", "osu"),
        ("Sound Voltex", "sdvx"),
        ("Chunithm", "chunithm"),
        ("maimai", "maimai"),
        ("beatmania IIDX", "iidx"),
    ]
    .into_iter()
    .map(|(name, id)| Game {
        name: name.into(),
        path: format!(r"C:\juegos\{id}.bat"),
        image: format!("assets/demo/{id}.svg"),
    })
    .collect()
}

pub async fn save_games(games: &[Game]) -> Result<(), String> {
    if is_tauri() {
        #[derive(Serialize)]
        struct Args<'a> {
            games: &'a [Game],
        }
        call("save_games", Args { games }).await.map(|_| ())
    } else {
        let json = serde_json::to_string(games).map_err(|e| e.to_string())?;
        storage().set_item(STORAGE_KEY, &json).map_err(js_err)
    }
}

pub async fn launch(path: &str) -> Result<(), String> {
    if is_tauri() {
        #[derive(Serialize)]
        struct Args<'a> {
            path: &'a str,
        }
        call("launch", Args { path }).await.map(|_| ())
    } else {
        leptos::logging::log!("launch (browser dev, not executed): {path}");
        Ok(())
    }
}

/// Native file picker for the game script. Browser dev has none (path is typed).
pub async fn pick_game_path() -> Option<String> {
    let v = call("pick_game_path", NoArgs {}).await.ok()?;
    serde_wasm_bindgen::from_value(v).ok().flatten()
}

pub async fn read_as_data_url(file: web_sys::File) -> Result<String, String> {
    read_as_data_url_js(file).await.map_err(js_err)?.as_string().ok_or("imagen inválida".into())
}

const WALLPAPER_KEY: &str = "wallpaper";

pub async fn load_wallpaper() -> Result<Option<String>, String> {
    if is_tauri() {
        let v = call("load_wallpaper", NoArgs {}).await?;
        serde_wasm_bindgen::from_value(v).map_err(|e| e.to_string())
    } else {
        Ok(storage().get_item(WALLPAPER_KEY).ok().flatten())
    }
}

pub async fn save_wallpaper(wallpaper: Option<&str>) -> Result<(), String> {
    if is_tauri() {
        #[derive(Serialize)]
        struct Args<'a> {
            wallpaper: Option<&'a str>,
        }
        call("save_wallpaper", Args { wallpaper }).await.map(|_| ())
    } else {
        // ponytail: localStorage caps around 5 MB; big wallpapers only fail in browser dev, not in Tauri.
        match wallpaper {
            Some(w) => storage().set_item(WALLPAPER_KEY, w).map_err(|_| "imagen demasiado grande para el navegador".into()),
            None => storage().remove_item(WALLPAPER_KEY).map_err(js_err),
        }
    }
}
