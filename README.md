# Rythm Fun

Game launcher (Tauri 2 + Leptos) for rhythm-game cabinets. It runs `.bat`/`.lnk` games and is navigable with four inputs.

## Development

- Browser: `trunk serve`, then open http://localhost:1420. Games are stored in `localStorage` and launching only logs to the console.
- Desktop: `cargo tauri dev` / `cargo tauri build`.
- Standalone executable: `trunk build --release`, then `cargo build -p rythmfunlauncher --features custom-protocol`. This embeds the interface and needs no local server.

## Controls

- TASOLLER slider: move left/right, confirm, and exit menus.
- FN1 + FN2: close the game.

## Launching (Windows)

Pressing a game runs, in order:

1. Writes the active card to the game's card file
2. Runs the "before" script and waits for it.
3. Changes the primary display (resolution, Hz, rotation) if the game has one set. The change is not saved to the registry, so a reboot always undoes it; a crash is undone on the next launcher start.
4. Starts the `.bat`/`.lnk`/`.exe` inside a Job Object and waits until every process it spawned exits.
5. Restores the display, then runs the "after" script.

The launcher stays always-on-top except while a game runs. Release builds register themselves to start with Windows.
Processes that elevate through UAC leave the Job Object (the launcher would think the game ended): run the launcher as admin if games need it.

## Data

Games are saved as `games.json` and cards as `cards.json` in the Tauri app config dir (e.g. `%APPDATA%\<app identifier>` on Windows).

Settings lets you choose separate portrait (ideally 9:16) and landscape (16:9) wallpapers. The background follows the window orientation automatically and uses the original wallpaper as a fallback when that orientation has no image. Each image can be removed independently. Native images are stored in `wallpaper-portrait.txt`, `wallpaper-landscape.txt`, and the existing `wallpaper.txt`; browser development uses corresponding localStorage keys. Wallpapers fill the screen with a centered crop and a subtle 2px blur; game cards have 95% opacity.

## Sounds

UI sounds are generated with Foley (`@foleyjs/core`) (MIT), vendored in `src/vendor`.
