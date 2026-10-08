# Rythm Fun Launcher

Switch-style game launcher (Tauri 2 + Leptos) for rhythm-game cabinets. It runs `.bat`/`.lnk` games and is navigable with four inputs.

## Development

- Browser: `trunk serve`, then open http://localhost:1420. Games are stored in `localStorage` and launching only logs to the console.
- Desktop: `cargo tauri dev` / `cargo tauri build`.

## Controls

- Keyboard: Left/Right move, Enter confirm, Backspace back.
- Gamepad: d-pad or stick to move, A confirm, B back.
- TASOLLER PLUS on Windows: automatic native WinUSB connection in WINUSB mode
  (VID `0e8f`, PID `1231`). Divide the panel into four equal quarters from left
  to right: **Left / Right / Back / Confirm**. Both rows belong to their quarter.
  Each touch fires once; release before pressing again. Multiple active quarters
  are ignored until released. AIR is unassigned. During a launched game,
  **FN1 + FN2 together** stop its process tree and return to the launcher.
  Release both buttons before pressing again; a held combination at game start
  is ignored until released. No driver or mode changes are made. Navigation
  releases WinUSB during games; the independent MI_02 HID diagnostic stream
  monitors FN1/FN2 using the same stream commands as TASOLLER Options V2.8.
  WinUSB reconnects afterward. Start with the panel released.
  The footer follows the physical left-to-right order. Menu LEDs identify the
  quarters in **yellow / orange / purple / pink**, brightening on touch with
  dark dividers between zones. Lighting is released to the game and restored
  when returning to the menu. LED write failures do not disable navigation.
- Yuancon controller via WebHID: press "Conectar controlador" in Settings. Raw reports are shown there so you can add bindings in `src/hid_profile.rs`.
  This WebHID path is available in browser development; the native desktop adapter
  currently supports TASOLLER PLUS. Yuancon's profile remains unconfigured.

TASOLLER PLUS packet layout reference:
[chuniio-rs](https://gitea.tendokyu.moe/beerpsi/chuniio-rs/src/branch/trunk/src/backends/tasoller_plus.rs).
Navigation uses the game IO pressure scale with threshold 20, separate from the
raw capacitance sensitivity shown by TASOLLER Options.

Controller checks: `node --test tests/platform.test.mjs`,
`cargo test -p rythmfunlauncher controller_profile`, and `cargo check --target wasm32-unknown-unknown -p rythmfunlauncher-ui`.

In each game's **Opciones → Control asociado**, choose TASOLLER PLUS or a
connected HID/gamepad. The association is saved with the game, including when
the controller is disconnected. Home places games associated with connected
controllers first, preserving the manual order within each group. If a connected
controller has associated games, launching a game outside that group asks for
confirmation. Left/right select Cancel/Open; Confirm accepts and Back cancels.
Priority affects only Home, so disconnecting the control restores the manual order.

## Launching (Windows)

Pressing a game runs, in order:

1. Writes the active Aime card to the game's card file (`[aime] aimePath` in its `segatools.ini`, relative to the game folder).
2. Runs the "before" script and waits for it.
3. Changes the primary display (resolution, Hz, rotation) if the game has one set. The change is not saved to the registry, so a reboot always undoes it; a crash is undone on the next launcher start.
4. Starts the `.bat`/`.lnk`/`.exe` inside a Job Object and waits until every process it spawned exits.
5. Restores the display, then runs the "after" script.

The launcher stays always-on-top except while a game runs. Release builds register themselves to start with Windows.
Processes that elevate through UAC leave the Job Object (the launcher would think the game ended): run the launcher as admin if games need it.

## Data

Games are saved as `games.json` and cards as `cards.json` in the Tauri app config dir (e.g. `%APPDATA%\<app identifier>` on Windows).

## Sounds

UI sounds are generated with Foley (`@foleyjs/core`) (MIT), vendored in `src/vendor`.
