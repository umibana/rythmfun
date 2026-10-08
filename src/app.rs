use crate::backend::{self, Card, Cards, Display, Game};
use crate::input;
use crate::nav::{Action, Nav, Outcome, Tab, Zone};
use crate::sound;
use leptos::{prelude::*, task::spawn_local};
use std::time::Duration;
use web_sys::HtmlInputElement;

#[component]
pub fn App() -> impl IntoView {
    let games = RwSignal::new(Vec::<Game>::new());
    let nav = RwSignal::new(Nav::default());
    let status = RwSignal::new(None::<(String, bool)>); // (message, is_error)
    let hid_name = RwSignal::new(None::<String>);
    let hid_raw = RwSignal::new(String::new());
    let launching = RwSignal::new(None::<usize>);
    let playing = RwSignal::new(false);
    let wallpaper = RwSignal::new(None::<String>);
    spawn_local(async move {
        if let Ok(w) = backend::load_wallpaper().await {
            wallpaper.set(w);
        }
    });
    // Autosave stays off after a failed load so it cannot overwrite the real file.
    let loaded = RwSignal::new(false);

    spawn_local(async move {
        match backend::load_games().await {
            Ok(g) => {
                games.set(g);
                loaded.set(true);
            }
            Err(e) => status.set(Some((format!("No se pudieron cargar los juegos: {e}"), true))),
        }
    });

    let on_action = Callback::new(move |a: Action| {
        // Gamepad/HID keep arriving while a game runs; don't act on them (Confirm would relaunch).
        if !document().has_focus().unwrap_or(true) {
            return;
        }
        status.set(None);
        let n = games.with_untracked(Vec::len);
        let (next, out) = nav.get_untracked().step(a, n);
        nav.set(next);
        let pan = if n > 1 { next.card as f64 / (n - 1) as f64 * 2.0 - 1.0 } else { 0.0 };
        match out {
            Outcome::Moved => sound::play("tick", if next.zone == Zone::Bar { 0.0 } else { pan }),
            Outcome::Confirmed => sound::play("press", 0.0),
            Outcome::Back => sound::play("release", 0.0),
            Outcome::Launch(_) if playing.get_untracked() => {}
            Outcome::Launch(i) => {
                sound::play("whoosh", pan);
                launching.set(Some(i));
                set_timeout(move || launching.set(None), Duration::from_millis(450));
                let game = games.with_untracked(|g| g[i].clone());
                status.set(Some((format!("Iniciando {}…", game.name), false)));
                playing.set(true);
                spawn_local(async move {
                    // Resolves once the game has exited.
                    match backend::launch(&game).await {
                        Ok(()) => status.set(None),
                        Err(e) => status.set(Some((format!("{}: {e}", game.name), true))),
                    }
                    playing.set(false);
                });
            }
            Outcome::Nothing => {}
        }
    });

    input::keyboard(move |a| on_action.run(a));
    input::gamepad(move |a| on_action.run(a));

    let connect_hid = Callback::new(move |ask: bool| {
        spawn_local(async move {
            match input::hid(ask, move |a| on_action.run(a), move |s| hid_raw.set(s)).await {
                Ok(Some(i)) => hid_name.set(Some(format!("{} ({:04x}:{:04x})", i.name, i.vendor_id, i.product_id))),
                Ok(None) => {}
                // Silent on startup (browser without WebHID); shown when the user clicked.
                Err(e) if ask => status.set(Some((e, true))),
                Err(e) => leptos::logging::log!("HID: {e}"),
            }
        })
    });
    connect_hid.run(false);

    let tab = Memo::new(move |_| nav.get().tab);

    view! {
        <div
            class="wallpaper"
            class:custom=move || wallpaper.with(Option::is_some)
            style=move || wallpaper.get().map(|w| format!("background-image: url(\"{w}\")")).unwrap_or_default()
        ></div>
        <header class="bar" class:active=move || nav.get().zone == Zone::Bar>
            <nav>
                <TabButton tab=Tab::Home label="Home" nav/>
                <TabButton tab=Tab::Settings label="Settings" nav/>
            </nav>
            <Clock/>
        </header>
        {move || status.get().map(|(s, err)| view! {
            <p class="status" class:error=err role="status" on:click=move |_| status.set(None)>{s}</p>
        })}
        <main>
            {move || match tab.get() {
                Tab::Home => view! { <Home games nav on_action launching/> }.into_any(),
                Tab::Settings => view! { <Settings games loaded status wallpaper hid_name hid_raw connect_hid/> }.into_any(),
            }}
        </main>
        <Hints nav hid_name/>
    }
}

/// Bottom bar: what each input does right now.
#[component]
fn Hints(nav: RwSignal<Nav>, hid_name: RwSignal<Option<String>>) -> impl IntoView {
    let hints = move || match nav.with(|n| (n.zone, n.tab)) {
        (Zone::Bar, _) => vec![("← →", "Cambiar"), ("⏎", "Entrar")],
        (Zone::Content, Tab::Home) => vec![("← →", "Elegir"), ("⏎", "Jugar"), ("⌫", "Menú")],
        (Zone::Content, Tab::Settings) => vec![("⌫", "Menú")],
    };
    view! {
        <footer class="hints">
            <span class="device">{move || hid_name.get().unwrap_or_else(|| "Teclado o gamepad".into())}</span>
            <span class="keys">
                {move || hints().into_iter().map(|(k, label)| view! {
                    <span class="hint"><kbd>{k}</kbd>{label}</span>
                }).collect_view()}
            </span>
        </footer>
    }
}

#[component]
fn TabButton(tab: Tab, label: &'static str, nav: RwSignal<Nav>) -> impl IntoView {
    view! {
        <button
            class="tab"
            tabindex="-1"
            class:current=move || nav.get().tab == tab
            class:focused=move || nav.with(|n| n.tab == tab && n.zone == Zone::Bar)
            on:mousedown=|e| e.prevent_default()
            on:click=move |_| nav.update(|n| {
                n.tab = tab;
                n.zone = Zone::Content;
            })
        >
            {label}
        </button>
    }
}

fn now() -> (String, String) {
    let opts = |pairs: &[(&str, &str)]| {
        let o = js_sys::Object::new();
        for (k, v) in pairs {
            let _ = js_sys::Reflect::set(&o, &(*k).into(), &(*v).into());
        }
        o
    };
    let d = js_sys::Date::new_0();
    let time = d.to_locale_time_string_with_options("es-CL", &opts(&[("hour", "2-digit"), ("minute", "2-digit"), ("hourCycle", "h23")]));
    let date = d.to_locale_date_string("es-CL", &opts(&[("weekday", "short"), ("day", "numeric"), ("month", "short")]));
    (time.into(), date.into())
}

#[component]
fn Clock() -> impl IntoView {
    let t = RwSignal::new(now());
    set_interval(move || t.set(now()), Duration::from_secs(10));
    view! {
        <div class="clock">
            <time class="time">{move || t.get().0}</time>
            <time class="date">{move || t.get().1}</time>
        </div>
    }
}

#[component]
fn Home(games: RwSignal<Vec<Game>>, nav: RwSignal<Nav>, on_action: Callback<Action>, launching: RwSignal<Option<usize>>) -> impl IntoView {
    // The track slides so the selected cover always sits at screen center.
    let card = Memo::new(move |_| {
        let n = games.with(Vec::len);
        nav.with(|nav| nav.card.min(n.saturating_sub(1)))
    });
    let name = move || games.with(|g| g.get(card.get()).map(|g| g.name.clone()).unwrap_or_default());
    view! {
        <section class="home" class:resting=move || nav.get().zone == Zone::Bar>
            {move || {
                let list = games.get();
                if list.is_empty() {
                    return view! {
                        <p class="empty">"Todavía no hay juegos. Entra a Settings y agrega el primero."</p>
                    }.into_any();
                }
                view! {
                    <div class="track" style=move || format!("--i: {}", card.get())>
                        {list.into_iter()
                            .enumerate()
                            .map(|(i, g)| view! { <GameCard i g card nav on_action launching/> })
                            .collect_view()}
                    </div>
                }.into_any()
            }}
            // Re-created on every change so the fade-in replays.
            {move || {
                let n = name();
                view! { <p class="caption">{n}</p> }
            }}
        </section>
    }
}

#[component]
fn GameCard(i: usize, g: Game, card: Memo<usize>, nav: RwSignal<Nav>, on_action: Callback<Action>, launching: RwSignal<Option<usize>>) -> impl IntoView {
    let selected = Memo::new(move |_| card.get() == i);
    let initial = g.name.chars().next().unwrap_or('?').to_string();
    view! {
        <button
            class="card"
            tabindex="-1"
            aria-label=g.name.clone()
            class:selected=move || selected.get()
            class:launching=move || launching.get() == Some(i)
            on:mousedown=|e| e.prevent_default()
            on:click=move |_| {
                nav.update(|n| {
                    n.zone = Zone::Content;
                    n.card = i;
                });
                on_action.run(Action::Confirm);
            }
        >
            {if g.image.is_empty() {
                view! { <div class="art placeholder">{initial}</div> }.into_any()
            } else {
                view! { <img class="art" src=g.image alt="" draggable="false"/> }.into_any()
            }}
        </button>
    }
}

type Draft = RwSignal<Vec<(u32, RwSignal<Game>)>>;

#[component]
fn Settings(
    games: RwSignal<Vec<Game>>,
    loaded: RwSignal<bool>,
    status: RwSignal<Option<(String, bool)>>,
    wallpaper: RwSignal<Option<String>>,
    hid_name: RwSignal<Option<String>>,
    hid_raw: RwSignal<String>,
    connect_hid: Callback<bool>,
) -> impl IntoView {
    // Rows are keyed signals so typing re-renders one field, not the whole list (keeps focus).
    let next_id = StoredValue::new(0u32);
    let fresh = move |g: Game| {
        let id = next_id.get_value();
        next_id.set_value(id + 1);
        (id, RwSignal::new(g))
    };
    let draft: Draft = RwSignal::new(games.get_untracked().into_iter().map(fresh).collect());

    // ponytail: writes on every edit; debounce if images make this slow.
    Effect::new(move |prev: Option<()>| {
        let list: Vec<Game> = draft.with(|v| v.iter().map(|(_, g)| g.get()).collect());
        if prev.is_none() || !loaded.get_untracked() {
            return;
        }
        games.set(list.clone());
        spawn_local(async move {
            if let Err(e) = backend::save_games(&list).await {
                status.set(Some((format!("No se pudo guardar: {e}"), true)));
            }
        });
    });

    view! {
        <section class="settings">
            <h2>"Juegos"</h2>
            <For each=move || draft.get() key=|(id, _)| *id let((id, g))>
                <GameRow id g draft/>
            </For>
            <div class="actions">
                <button on:click=move |_| draft.update(|v| v.push(fresh(Game::default())))>"Agregar juego"</button>
            </div>

            <h2>"Tarjetas (Aime)"</h2>
            <CardManager status/>

            <h2>"Fondo de pantalla"</h2>
            <Wallpaper wallpaper status/>

            <h2>"Controlador"</h2>
            <p>{move || hid_name.get().unwrap_or_else(|| "Ningún dispositivo HID conectado".into())}</p>
            <button on:click=move |_| connect_hid.run(true)>"Conectar controlador HID"</button>
            <pre class="raw">{hid_raw}</pre>
        </section>
    }
}

#[component]
fn GameRow(id: u32, g: RwSignal<Game>, draft: Draft) -> impl IntoView {
    let shift = move |d: isize| {
        draft.update(|v| {
            let Some(i) = v.iter().position(|(k, _)| *k == id) else { return };
            let j = i as isize + d;
            if (0..v.len() as isize).contains(&j) {
                v.swap(i, j as usize);
            }
        })
    };
    let on_image = move |e: leptos::ev::Event| {
        let input: HtmlInputElement = event_target(&e);
        if let Some(file) = input.files().and_then(|l| l.get(0)) {
            spawn_local(async move {
                if let Ok(url) = backend::read_as_data_url(file).await {
                    g.update(|g| g.image = url);
                }
            });
        }
    };
    let browse = move |_| {
        spawn_local(async move {
            if let Some(p) = backend::pick_game_path().await {
                g.update(|g| g.path = p);
            }
        })
    };
    view! {
        <div class="row">
            <label class="thumb" title="Elegir imagen">
                {move || {
                    let img = g.with(|g| g.image.clone());
                    (!img.is_empty()).then(|| view! { <img src=img alt=""/> })
                }}
                <input type="file" accept="image/*" on:change=on_image/>
            </label>
            <input
                placeholder="Nombre"
                prop:value=move || g.with(|g| g.name.clone())
                on:input=move |e| g.update(|g| g.name = event_target_value(&e))
            />
            <input
                class="path"
                placeholder="C:\\juegos\\juego.bat"
                prop:value=move || g.with(|g| g.path.clone())
                on:input=move |e| g.update(|g| g.path = event_target_value(&e))
            />
            {backend::is_tauri().then(|| view! { <button on:click=browse>"Examinar…"</button> })}
            <button title="Subir" on:click=move |_| shift(-1)>"↑"</button>
            <button title="Bajar" on:click=move |_| shift(1)>"↓"</button>
            <button title="Quitar" on:click=move |_| draft.update(|v| v.retain(|(k, _)| *k != id))>"✕"</button>
        </div>
        <GameOptions g/>
    }
}

/// Per-game extras: scripts, card file and display mode. Empty / 0 = not used.
#[component]
fn GameOptions(g: RwSignal<Game>) -> impl IntoView {
    let text = move |label: &'static str, hint: &'static str, get: fn(&Game) -> &String, set: fn(&mut Game, String)| {
        view! {
            <label>
                <span>{label}</span>
                <input
                    placeholder=hint
                    prop:value=move || g.with(|g| get(g).clone())
                    on:input=move |e| g.update(|g| set(g, event_target_value(&e)))
                />
            </label>
        }
    };
    let num = move |label: &'static str, get: fn(&Display) -> u32, set: fn(&mut Display, u32)| {
        view! {
            <label>
                <span>{label}</span>
                <input
                    type="number"
                    min="0"
                    placeholder="actual"
                    prop:value=move || g.with(|g| match get(&g.display) { 0 => String::new(), n => n.to_string() })
                    on:input=move |e| g.update(|g| set(&mut g.display, event_target_value(&e).parse().unwrap_or(0)))
                />
            </label>
        }
    };
    view! {
        <details class="options">
            <summary>"Opciones"</summary>
            <div class="grid">
                {text("Script antes", r"C:\juegos\antes.bat", |g| &g.pre, |g, v| g.pre = v)}
                {text("Script después", r"C:\juegos\despues.bat", |g| &g.post, |g, v| g.post = v)}
                {text("Archivo de tarjeta", r"DEVICE\aime.txt (relativo al juego)", |g| &g.aime_path, |g, v| g.aime_path = v)}
            </div>
            <p class="note">"Pantalla principal mientras se juega. Vacío = no cambiar. Resolución en horizontal (ej. 1920×1080); la rotación la gira."</p>
            <div class="grid display">
                {num("Ancho", |d| d.width, |d, v| d.width = v)}
                {num("Alto", |d| d.height, |d, v| d.height = v)}
                {num("Hz", |d| d.hz, |d, v| d.hz = v)}
                <label>
                    <span>"Orientación"</span>
                    <select on:change=move |e| g.update(|g| g.display.rotation = event_target_value(&e).parse().ok())>
                        {[(None, "No cambiar"), (Some(0), "0° horizontal"), (Some(90), "90° vertical"), (Some(180), "180° horizontal invertida"), (Some(270), "270° vertical invertida")]
                            .into_iter()
                            .map(|(r, label)| view! {
                                <option
                                    value=r.map(|r: u32| r.to_string()).unwrap_or_default()
                                    selected=move || g.with(|g| g.display.rotation == r)
                                >{label}</option>
                            })
                            .collect_view()}
                    </select>
                </label>
            </div>
        </details>
    }
}

/// Virtual Aime cards. The active one is written to each game's card file right before it starts.
#[component]
fn CardManager(status: RwSignal<Option<(String, bool)>>) -> impl IntoView {
    let rows = RwSignal::new(Vec::<RwSignal<Card>>::new());
    let active = RwSignal::new(String::new());
    // Autosave stays off until the file loaded, so a failed read cannot wipe it.
    let loaded = RwSignal::new(false);
    spawn_local(async move {
        match backend::load_cards().await {
            Ok(c) => {
                active.set(c.active);
                rows.set(c.cards.into_iter().map(RwSignal::new).collect());
                loaded.set(true);
            }
            Err(e) => status.set(Some((format!("No se pudieron cargar las tarjetas: {e}"), true))),
        }
    });
    Effect::new(move |_| {
        if !loaded.get() {
            return;
        }
        let cards = Cards { active: active.get(), cards: rows.with(|v| v.iter().map(|c| c.get()).collect()) };
        spawn_local(async move {
            if let Err(e) = backend::save_cards(&cards).await {
                status.set(Some((format!("No se pudieron guardar las tarjetas: {e}"), true)));
            }
        });
    });
    let add = move |_| {
        let card = Card::random(format!("Tarjeta {}", rows.with(Vec::len) + 1));
        if active.with(String::is_empty) {
            active.set(card.id.clone());
        }
        rows.update(|v| v.push(RwSignal::new(card)));
    };

    view! {
        <For each=move || rows.get() key=|c| c.get_untracked().id let(c)>
            {
                let id = c.get_untracked().id;
                let id_active = id.clone();
                let id_remove = id.clone();
                view! {
                    <div class="row card-row" class:active=move || active.with(|a| *a == id)>
                        <input
                            type="radio"
                            name="active-card"
                            title="Usar esta tarjeta"
                            prop:checked={let id = id_active.clone(); move || active.with(|a| *a == id)}
                            on:change=move |_| active.set(id_active.clone())
                        />
                        <input
                            placeholder="Nombre"
                            prop:value=move || c.with(|c| c.name.clone())
                            on:input=move |e| c.update(|c| c.name = event_target_value(&e))
                        />
                        <input
                            class="path number"
                            class:invalid=move || !c.with(Card::is_valid)
                            inputmode="numeric"
                            maxlength="20"
                            placeholder="20 dígitos"
                            prop:value=move || c.with(|c| c.number.clone())
                            on:input=move |e| c.update(|c| c.number = event_target_value(&e).chars().filter(char::is_ascii_digit).collect())
                        />
                        <button title="Quitar" on:click=move |_| {
                            rows.update(|v| v.retain(|r| r.get_untracked().id != id_remove));
                            if active.get_untracked() == id_remove {
                                active.set(rows.with_untracked(|v| v.first().map(|r| r.get_untracked().id).unwrap_or_default()));
                            }
                        }>"✕"</button>
                    </div>
                }
            }
        </For>
        <div class="actions">
            <button on:click=add>"Agregar tarjeta"</button>
        </div>
    }
}

#[component]
fn Wallpaper(wallpaper: RwSignal<Option<String>>, status: RwSignal<Option<(String, bool)>>) -> impl IntoView {
    let set = move |w: Option<String>| {
        spawn_local(async move {
            match backend::save_wallpaper(w.as_deref()).await {
                Ok(()) => wallpaper.set(w),
                Err(e) => status.set(Some((format!("No se pudo guardar el fondo: {e}"), true))),
            }
        })
    };
    let on_pick = move |e: leptos::ev::Event| {
        let input: HtmlInputElement = event_target(&e);
        if let Some(file) = input.files().and_then(|l| l.get(0)) {
            spawn_local(async move {
                match backend::read_as_data_url(file).await {
                    Ok(url) => set(Some(url)),
                    Err(e) => status.set(Some((format!("No se pudo leer la imagen: {e}"), true))),
                }
            });
        }
    };
    view! {
        <div class="wallpaper-row">
            <div
                class="wallpaper-preview"
                style=move || wallpaper.get().map(|w| format!("background-image: url(\"{w}\")")).unwrap_or_default()
            ></div>
            <label class="button">
                "Elegir imagen"
                <input type="file" accept="image/*" on:change=on_pick/>
            </label>
            {move || wallpaper.with(Option::is_some).then(|| view! {
                <button on:click=move |_| set(None)>"Quitar fondo"</button>
            })}
        </div>
    }
}
