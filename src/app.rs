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

    spawn_local(async move {
        match backend::load_games().await {
            Ok(g) => games.set(g),
            Err(e) => status.set(Some((format!("No se pudieron cargar los juegos: {e}"), true))),
        }
    });

    let on_action = Callback::new(move |a: Action| {
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
                let path = games.with_untracked(|g| g[i].path.clone());
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
                Tab::Home => view! { <Home games nav on_action/> }.into_any(),
                Tab::Settings => view! { <Settings games status hid_name hid_raw connect_hid/> }.into_any(),
            }}
        </main>
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

fn today() -> String {
    let opts = js_sys::Object::new();
    for (k, v) in [("weekday", "short"), ("day", "numeric"), ("month", "short")] {
        let _ = js_sys::Reflect::set(&opts, &k.into(), &v.into());
    }
    js_sys::Date::new_0().to_locale_date_string("es-CL", &opts).into()
}

#[component]
fn Clock() -> impl IntoView {
    let date = RwSignal::new(today());
    set_interval(move || date.set(today()), Duration::from_secs(30));
    view! { <time class="clock">{date}</time> }
}

#[component]
fn Home(games: RwSignal<Vec<Game>>, nav: RwSignal<Nav>, on_action: Callback<Action>) -> impl IntoView {
    view! {
        <section class="carousel">
            {move || {
                let list = games.get();
                if list.is_empty() {
                    return view! { <p class="empty">"Sin juegos. Agrégalos en Settings."</p> }.into_any();
                }
                list.into_iter()
                    .enumerate()
                    .map(|(i, g)| view! { <GameCard i g nav on_action/> })
                    .collect_view()
                    .into_any()
            }}
        </section>
    }
}

#[component]
fn GameCard(i: usize, g: Game, nav: RwSignal<Nav>, on_action: Callback<Action>) -> impl IntoView {
    let el = NodeRef::<html::Button>::new();
    let selected = Memo::new(move |_| nav.with(|n| n.zone == Zone::Content && n.card == i));
    Effect::new(move |_| {
        if let (true, Some(el)) = (selected.get(), el.get()) {
            let o = web_sys::ScrollIntoViewOptions::new();
            o.set_behavior(web_sys::ScrollBehavior::Auto);
            o.set_inline(web_sys::ScrollLogicalPosition::Center);
            el.scroll_into_view_with_scroll_into_view_options(&o);
        }
    });
    let initial = g.name.chars().next().unwrap_or('?').to_string();
    view! {
        <button
            class="card"
            tabindex="-1"
            node_ref=el
            class:selected=move || selected.get()
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
            <span class="name">{g.name}</span>
        </button>
    }
}

type Draft = RwSignal<Vec<(u32, RwSignal<Game>)>>;

#[component]
fn Settings(
    games: RwSignal<Vec<Game>>,
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
        if prev.is_none() {
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
