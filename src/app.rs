use crate::backend::{self, Game};
use crate::input;
use crate::nav::{Action, Nav, Outcome, Tab, Zone};
use crate::sound;
use leptos::{html, prelude::*, task::spawn_local};
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
            Outcome::Launch(i) => {
                sound::play("whoosh", pan);
                launching.set(Some(i));
                set_timeout(move || launching.set(None), Duration::from_millis(450));
                let (name, path) = games.with_untracked(|g| (g[i].name.clone(), g[i].path.clone()));
                status.set(Some((format!("Iniciando {name}…"), false)));
                spawn_local(async move {
                    if let Err(e) = backend::launch(&path).await {
                        status.set(Some((format!("No se pudo lanzar: {e}"), true)));
                    }
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
                Tab::Settings => view! { <Settings games loaded status hid_name hid_raw connect_hid/> }.into_any(),
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
    let title = move || {
        let card = nav.with(|n| n.card);
        games.with(|g| g.get(card.min(g.len().saturating_sub(1))).map(|g| g.name.clone()))
    };
    view! {
        <section class="home" class:resting=move || nav.get().zone == Zone::Bar>
            <h1 class="title">{title}</h1>
            <div class="carousel">
                {move || {
                    let list = games.get();
                    if list.is_empty() {
                        return view! {
                            <p class="empty">"Todavía no hay juegos. Entra a Settings y agrega el primero."</p>
                        }.into_any();
                    }
                    list.into_iter()
                        .enumerate()
                        .map(|(i, g)| view! { <GameCard i g nav on_action launching/> })
                        .collect_view()
                        .into_any()
                }}
            </div>
        </section>
    }
}

#[component]
fn GameCard(i: usize, g: Game, nav: RwSignal<Nav>, on_action: Callback<Action>, launching: RwSignal<Option<usize>>) -> impl IntoView {
    let el = NodeRef::<html::Button>::new();
    let selected = Memo::new(move |_| nav.with(|n| n.zone == Zone::Content && n.card == i));
    Effect::new(move |_| {
        if let (true, Some(el)) = (selected.get(), el.get()) {
            let o = web_sys::ScrollIntoViewOptions::new();
            o.set_behavior(web_sys::ScrollBehavior::Auto);
            o.set_inline(web_sys::ScrollLogicalPosition::Nearest);
            el.scroll_into_view_with_scroll_into_view_options(&o);
        }
    });
    let initial = g.name.chars().next().unwrap_or('?').to_string();
    view! {
        <button
            class="card"
            tabindex="-1"
            node_ref=el
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
                view! { <img class="art" src=g.image alt=""/> }.into_any()
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
    }
}
