# Rythm Fun Launcher — Design

Date: 2026-10-05

## Goal

Windows game launcher with a Nintendo Switch–style home screen. Big game cards in a
horizontal scroll, top bar with **Home** / **Settings** and the current date (top right).
Selecting a game runs its `.bat` or `.lnk`. Navigable with exactly four inputs from
keyboard, standard gamepads, or HID rhythm controllers (Yuancon). UI plays sounds.

Dev loop runs in a normal browser (`trunk serve`); production is a Tauri app on Windows.

## Stack

Existing scaffold: Tauri 2 + Leptos 0.8 (CSR) + Trunk. No framework changes.

## Architecture

### Backend (`src-tauri/`)

Tauri commands:

| Command | Behavior |
|---|---|
| `load_games() -> Vec<Game>` | Reads `games.json` from the app config dir. Missing file → empty list. |
| `save_games(games: Vec<Game>)` | Writes `games.json` to the app config dir. |
| `launch(path: String)` | `tauri-plugin-opener` `open_path` (already installed). Windows runs `.bat` / `.lnk` with its default handler. |

`Game { name: String, path: String, image: String }`. `image` is a data URL.
`ponytail:` data URL keeps us off the asset protocol; ceiling = big JSON with large images,
switch to asset protocol + file path if that hurts.

New dependency: `tauri-plugin-dialog` (file picker for `.bat`/`.lnk` and images).

### Frontend (`src/`)

- `App`: top bar (Home, Settings, date top-right) + active view.
- `Home`: horizontal carousel of big cards, 3–4 visible, focused card scrolls into view.
- `Settings`: games editor (add / edit / remove / reorder by mouse; file pickers) +
  controller section (connect HID button, detected device, raw report bytes).
- Backend bridge: if `window.__TAURI__` exists call Tauri commands; otherwise (browser dev)
  games live in `localStorage` and `launch` logs to console. File pickers in browser mode
  use `<input type="file">` (image → data URL; path is typed by hand).

## Input

Exactly four actions:

```rust
enum Action { Left, Right, Confirm, Back }
```

UI only consumes `Action`. Sources feed one shared signal:

1. **Keyboard:** `ArrowLeft`, `ArrowRight`, `Enter`, `Backspace`.
2. **Gamepad API:** polling via `requestAnimationFrame`, standard mapping
   (d-pad left/right + left stick X with deadzone, A = Confirm, B = Back). Edge-triggered.
3. **WebHID:** "Connect controller" button in Settings → `navigator.hid.requestDevice`
   with Yuancon's filters. Profile table keyed by `VID:PID`:
   - Yuancon tassa: VID `0x5F73`, PID `0x0010` / `0x0011`, vendor collection
     `usagePage 0xFF71`, `usage 0x61` (taken from yuancon.app).
   - Profile = `fn(&[u8]) -> Vec<Action>`. Tassa input layout is unknown, so the first
     version shows raw report bytes in Settings; the mapping is filled in once captured
     on real hardware.

### Navigation model (4 inputs)

- Focus zone is either **nav bar** or **content**.
- Content/Home: Left/Right move between cards, Confirm launches, Back → nav bar.
- Nav bar: Left/Right switch Home ↔ Settings, Confirm → content, Back → no-op.
- Settings content is mouse/keyboard-driven (editing paths needs it); Back → nav bar.

## Sound

Sounds from snd.dev (kit SND01 "sine"), bundled as audio files under `public/sounds/`
(offline app — no CDN). Played with `HtmlAudioElement`, no JS library.

| Event | Sound |
|---|---|
| Left/Right move | tap / select |
| Confirm | button |
| Back | back / cancel-style tap |
| Launch game | transition |

License: free for commercial/non-commercial, attribution appreciated, no redistribution of
raw assets → keep repo private or document source; add attribution in Settings.

## Testing

- Rust unit tests: HID profile parsing (bytes → actions, once tassa layout known) and
  `Game` JSON round-trip.
- UI verified manually in browser via `trunk serve`.

## Known risks (deferred until Windows + real devices available)

- WebHID inside WebView2 may lack a device chooser / permission flow. Fallback: read HID
  in Rust with `hidapi` and emit `Action` events to the frontend; UI unchanged.

## Out of scope

Themes, button remapping UI, more tabs, game metadata, autostart, kiosk/fullscreen mode.
