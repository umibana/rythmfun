use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Game {
    name: String,
    path: String,
    image: String,
}

fn read_games(file: &Path) -> Result<Vec<Game>, String> {
    match std::fs::read_to_string(file) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| e.to_string()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.to_string()),
    }
}

/// Write to a temp file then rename, so a crash mid-write never truncates the list.
fn write_games(file: &Path, games: &[Game]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(games).map_err(|e| e.to_string())?;
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())
}

fn games_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("games.json"))
}

#[tauri::command]
fn load_games(app: AppHandle) -> Result<Vec<Game>, String> {
    read_games(&games_file(&app)?)
}

#[tauri::command]
fn save_games(app: AppHandle, games: Vec<Game>) -> Result<(), String> {
    write_games(&games_file(&app)?, &games)
}

/// Runs a .bat/.lnk/.exe from its own folder (game scripts usually use relative paths).
#[cfg(windows)]
#[tauri::command]
fn launch(path: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let dir = Path::new(&path).parent().ok_or("ruta inválida")?;
    // ponytail: paths containing `"` or `&` break cmd quoting; use ShellExecuteW if that ever matters.
    std::process::Command::new("cmd")
        .raw_arg(format!("/C start \"\" \"{path}\""))
        .current_dir(dir)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
#[tauri::command]
fn launch(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
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
        .invoke_handler(tauri::generate_handler![load_games, save_games, launch, pick_game_path])
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

        assert_eq!(read_games(&file).unwrap(), vec![]);

        let games = vec![Game { name: "osu".into(), path: r"C:\games\osu.lnk".into(), image: "data:image/png;base64,AA==".into() }];
        write_games(&file, &games).unwrap();
        assert_eq!(read_games(&file).unwrap(), games);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
