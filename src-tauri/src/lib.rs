use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

#[path = "../../src/controller_binding.rs"]
mod controller_binding;

#[cfg(windows)]
mod win;
#[cfg(windows)]
mod controller;
#[cfg(windows)]
mod controller_profile;
#[cfg(windows)]
mod controller_usb;
#[cfg(windows)]
mod controller_hid;

#[tauri::command]
fn controller_snapshot(app: AppHandle) -> serde_json::Value {
    #[cfg(windows)]
    { serde_json::to_value(app.state::<controller::Controller>().snapshot()).unwrap_or_default() }
    #[cfg(not(windows))]
    { let _ = app; serde_json::json!({ "device": null, "raw": [], "actions": [], "status": "Entrada nativa disponible en Windows" }) }
}

/// Display mode for a game. Zero / `None` keeps the current value.
/// `width`×`height` is the unrotated mode (e.g. 1920×1080); rotation is in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Display {
    width: u32,
    height: u32,
    hz: u32,
    rotation: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Game {
    name: String,
    path: String,
    image: String,
    /// Scripts run (and waited for) before / after the game.
    pre: String,
    post: String,
    /// Where segatools reads the card number (`[aime] aimePath`); relative to the game folder.
    aime_path: String,
    display: Display,
    controller: Option<controller_binding::ControllerBinding>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Card {
    id: String,
    name: String,
    number: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Cards {
    active: String,
    cards: Vec<Card>,
}

/// Launcher-wide switches, set from Settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    autostart: bool,
    always_on_top: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { autostart: true, always_on_top: true }
    }
}

fn read_json<T: DeserializeOwned + Default>(file: &Path) -> Result<T, String> {
    match std::fs::read_to_string(file) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| e.to_string()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.to_string()),
    }
}

/// Write to a temp file then rename, so a crash mid-write never truncates the file.
fn write_atomic(file: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, contents).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())
}

fn write_json<T: Serialize>(file: &Path, value: &T) -> Result<(), String> {
    write_atomic(file, serde_json::to_string_pretty(value).map_err(|e| e.to_string())?)
}

fn config_file(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(name))
}

/// Wallpaper is kept as a data URL in its own file so games.json stays small.
#[tauri::command]
fn load_wallpaper(app: AppHandle) -> Result<Option<String>, String> {
    match std::fs::read_to_string(config_file(&app, "wallpaper.txt")?) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn save_wallpaper(app: AppHandle, wallpaper: Option<String>) -> Result<(), String> {
    let file = config_file(&app, "wallpaper.txt")?;
    match wallpaper {
        Some(w) => std::fs::write(file, w).map_err(|e| e.to_string()),
        None => match std::fs::remove_file(file) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        },
    }
}

#[tauri::command]
fn load_games(app: AppHandle) -> Result<Vec<Game>, String> {
    read_json(&config_file(&app, "games.json")?)
}

#[tauri::command]
fn save_games(app: AppHandle, games: Vec<Game>) -> Result<(), String> {
    write_json(&config_file(&app, "games.json")?, &games)
}

#[tauri::command]
fn load_cards(app: AppHandle) -> Result<Cards, String> {
    read_json(&config_file(&app, "cards.json")?)
}

#[tauri::command]
fn save_cards(app: AppHandle, cards: Cards) -> Result<(), String> {
    write_json(&config_file(&app, "cards.json")?, &cards)
}

fn load_prefs_file(app: &AppHandle) -> Prefs {
    config_file(app, "prefs.json").and_then(|f| read_json(&f)).unwrap_or_default()
}

fn apply_prefs(app: &AppHandle, p: Prefs) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        w.set_always_on_top(p.always_on_top).map_err(|e| e.to_string())?;
    }
    // Release only: dev builds would register target/debug as the startup program.
    #[cfg(not(debug_assertions))]
    {
        use tauri_plugin_autostart::ManagerExt;
        let auto = app.autolaunch();
        if auto.is_enabled().unwrap_or(false) != p.autostart {
            let r = if p.autostart { auto.enable() } else { auto.disable() };
            r.map_err(|e| format!("Inicio con Windows: {e}"))?;
        }
    }
    Ok(())
}

#[tauri::command]
fn load_prefs(app: AppHandle) -> Prefs {
    load_prefs_file(&app)
}

#[tauri::command]
fn save_prefs(app: AppHandle, prefs: Prefs) -> Result<(), String> {
    write_json(&config_file(&app, "prefs.json")?, &prefs)?;
    apply_prefs(&app, prefs)
}

fn valid_card_number(n: &str) -> bool {
    n.len() == 20 && n.bytes().all(|b| b.is_ascii_digit())
}

/// Writes the active card to the game's aime.txt. No path or no active card: nothing to do.
fn write_card(app: &AppHandle, game: &Game) -> Result<(), String> {
    if game.aime_path.is_empty() {
        return Ok(());
    }
    let cards: Cards = read_json(&config_file(app, "cards.json")?)?;
    let Some(card) = cards.cards.iter().find(|c| c.id == cards.active) else { return Ok(()) };
    if !valid_card_number(&card.number) {
        return Err(format!("La tarjeta «{}» no tiene 20 dígitos", card.name));
    }
    // `join` keeps an absolute aime_path as is.
    let file = Path::new(&game.path).parent().unwrap_or(Path::new("")).join(&game.aime_path);
    write_atomic(&file, format!("{}\n", card.number)).map_err(|e| format!("No se pudo escribir {}: {e}", file.display()))
}

/// Mode to request, given the current one (`w`, `h`, orientation 0..=3 as in DEVMODE) and the game's wish.
/// DEVMODE wants width/height as seen after rotation, so odd orientations swap them.
#[cfg_attr(not(windows), allow(dead_code))] // used by win.rs and tests
fn target_mode((cw, ch, co): (u32, u32, u32), d: &Display) -> (u32, u32, u32) {
    let native = if co % 2 == 1 { (ch, cw) } else { (cw, ch) };
    let (w, h) = if d.width > 0 && d.height > 0 { (d.width, d.height) } else { native };
    let o = d.rotation.map_or(co, |r| r / 90 % 4);
    if o % 2 == 1 { (h, w, o) } else { (w, h, o) }
}

fn run_script(path: &str, when: &str) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }
    #[cfg(windows)]
    let r = win::run_script(path);
    #[cfg(not(windows))]
    let r = std::process::Command::new("sh").arg(path).status().map_err(|e| e.to_string()).and_then(|s| {
        if s.success() { Ok(()) } else { Err(format!("terminó con {s}")) }
    });
    r.map_err(|e| format!("Script {when}: {e}"))
}

#[cfg(windows)]
fn run_game(app: &AppHandle, game: &Game) -> Result<(), String> {
    let _display = win::apply_display(&game.display, &config_file(app, "display-changed")?)?;
    win::run_and_wait(&game.path, &app.state::<controller::Controller>().stop_requested)
} // _display restores the mode here, even if the game failed to start.

/// Dev on macOS/Linux: just open it, no waiting or display changes.
#[cfg(not(windows))]
fn run_game(app: &AppHandle, game: &Game) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_path(&game.path, None::<&str>).map_err(|e| e.to_string())
}

/// card → pre script → display mode → game (until its whole process tree exits) → restore display → post script.
fn play(app: &AppHandle, game: &Game) -> Result<(), String> {
    write_card(app, game)?;
    run_script(&game.pre, "antes")?;
    let played = run_game(app, game);
    let post = run_script(&game.post, "después");
    played.and(post)
}

static PLAYING: AtomicBool = AtomicBool::new(false);

/// Resolves when the game has exited, so the UI knows the launcher is back.
#[tauri::command]
async fn launch(app: AppHandle, window: WebviewWindow, game: Game) -> Result<(), String> {
    if PLAYING.swap(true, Ordering::SeqCst) {
        return Err("Ya hay un juego en curso".into());
    }
    let _ = window.set_always_on_top(false);
    let res = tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let controller = app.state::<controller::Controller>();
        #[cfg(windows)]
        let _suspension = controller.suspend()?;
        play(&app, &game)
    }).await.map_err(|e| e.to_string()).and_then(|r| r);
    let _ = window.set_always_on_top(load_prefs_file(window.app_handle()).always_on_top);
    let _ = window.set_focus();
    PLAYING.store(false, Ordering::SeqCst);
    res
}

#[tauri::command]
async fn pick_game_path(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .add_filter("Juego", &["bat", "lnk", "exe"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(|app| {
            #[cfg(windows)]
            app.manage(controller::Controller::new()?);
            // A crash mid-game leaves the game's display mode on; undo it.
            #[cfg(windows)]
            win::restore_display(&config_file(app.handle(), "display-changed")?);
            if let Err(e) = apply_prefs(app.handle(), load_prefs_file(app.handle())) {
                eprintln!("prefs: {e}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            controller_snapshot,
            load_games,
            save_games,
            load_cards,
            save_cards,
            load_prefs,
            save_prefs,
            load_wallpaper,
            save_wallpaper,
            launch,
            pick_game_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_empty_and_round_trip_works() {
        let dir = std::env::temp_dir().join(format!("rfl-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("games.json");
        let _ = std::fs::remove_file(&file);

        assert_eq!(read_json::<Vec<Game>>(&file).unwrap(), vec![]);

        let games = vec![Game { name: "osu".into(), path: r"C:\games\osu.lnk".into(), image: "data:image/png;base64,AA==".into(), ..Game::default() }];
        write_json(&file, &games).unwrap();
        assert_eq!(read_json::<Vec<Game>>(&file).unwrap(), games);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn old_games_json_still_loads() {
        let g: Vec<Game> = serde_json::from_str(r#"[{"name":"a","path":"b","image":""}]"#).unwrap();
        assert_eq!(g[0].display, Display::default());
        assert_eq!(g[0].controller, None);
    }

    #[test]
    fn controller_association_survives_game_save_and_reload() {
        let games: Vec<Game> = serde_json::from_str(r#"[{"name":"Chunithm","path":"launch.bat","controller":{"id":"hid:0e8f:1231","name":"TASOLLER PLUS"}}]"#).unwrap();
        let saved = serde_json::to_string(&games).unwrap();
        let reloaded: Vec<Game> = serde_json::from_str(&saved).unwrap();
        assert_eq!(reloaded, games);
        assert_eq!(reloaded[0].controller.as_ref().unwrap().id, "hid:0e8f:1231");
    }

    #[test]
    fn card_number_needs_20_digits() {
        assert!(valid_card_number("01234567890123456789"));
        assert!(!valid_card_number("0123456789012345678"));
        assert!(!valid_card_number("0123456789012345678x"));
    }

    #[test]
    fn target_mode_swaps_for_portrait() {
        let keep = Display::default();
        let portrait = Display { rotation: Some(90), ..keep };
        let res = Display { width: 1280, height: 720, ..keep };
        // landscape 1920x1080 → portrait keeps the panel mode, swapped for DEVMODE
        assert_eq!(target_mode((1920, 1080, 0), &portrait), (1080, 1920, 1));
        // already portrait, only resolution changes: stays portrait
        assert_eq!(target_mode((1080, 1920, 1), &res), (720, 1280, 1));
        // back to landscape from portrait
        assert_eq!(target_mode((1080, 1920, 3), &Display { rotation: Some(0), ..keep }), (1920, 1080, 0));
        assert_eq!(target_mode((1920, 1080, 0), &keep), (1920, 1080, 0));
    }
}
