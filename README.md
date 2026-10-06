# Rythm Fun Launcher

Switch-style game launcher (Tauri 2 + Leptos) for rhythm-game cabinets. It runs `.bat`/`.lnk` games and is navigable with four inputs.

## Development

- Browser: `trunk serve`, then open http://localhost:1420. Games are stored in `localStorage` and launching only logs to the console.
- Desktop: `cargo tauri dev` / `cargo tauri build`.

## Controls

- Keyboard: Left/Right move, Enter confirm, Backspace back.
- Gamepad: d-pad or stick to move, A confirm, B back.
- Yuancon controller via WebHID: press "Conectar controlador HID" in Settings. Raw reports are shown there so you can add bindings in `src/hid_profile.rs`.

## Data

Games are saved as `games.json` in the Tauri app config dir (e.g. `%APPDATA%\<app identifier>` on Windows).

## Sounds

UI sounds are generated with Foley (`@foleyjs/core`) (MIT), vendored in `src/vendor`.
