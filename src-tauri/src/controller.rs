use crate::controller_profile::{tasoller_zones, tasoller_lights, hid_fn_buttons, Action, ZoneEdges, StopCombo, Fn1Tap};
use crate::controller_hid::TasollerHid;
use crate::controller_usb::TasollerUsb;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo { vendor_id: u16, product_id: u16, name: String }

#[derive(Serialize)]
pub struct Snapshot {
    device: Option<DeviceInfo>,
    raw: Vec<u8>,
    actions: Vec<&'static str>,
    status: String,
}

#[derive(Default)]
struct Shared {
    device: Option<DeviceInfo>,
    raw: Vec<u8>,
    actions: VecDeque<(Action, Instant)>,
    status: String,
}

impl Shared {
    fn reset(&mut self, status: impl Into<String>) {
        self.device = None;
        self.raw.clear();
        self.actions.clear();
        self.status = status.into();
    }
}

enum Command { Pause(mpsc::Sender<()>), Resume, Stop }

pub struct Controller {
    pub stop_requested: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
    commands: mpsc::Sender<Command>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Controller {
    pub fn new() -> std::io::Result<Self> {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (commands, receiver) = mpsc::channel();
        let state = shared.clone();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let stop = stop_requested.clone();
        let worker = thread::Builder::new().name("controller-input".into()).spawn(move || run(state, receiver, stop))?;
        Ok(Self { shared, commands, worker: Mutex::new(Some(worker)), stop_requested })
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let actions = s.actions.drain(..).filter(|(_, time)| time.elapsed() < Duration::from_millis(250)).map(|(a, _)| match a {
            Action::Left => "left", Action::Right => "right", Action::Back => "back", Action::Confirm => "confirm",
        }).collect();
        Snapshot { device: s.device.clone(), raw: s.raw.clone(), actions, status: s.status.clone() }
    }

    /// Wait until the reader has closed its handles before letting a game start.
    pub fn suspend(&self) -> Result<Suspension<'_>, String> {
        let (ack, receiver) = mpsc::channel();
        self.commands.send(Command::Pause(ack)).map_err(|_| "El lector de controles no responde")?;
        if receiver.recv_timeout(Duration::from_secs(2)).is_err() {
            let _ = self.commands.send(Command::Resume);
            return Err("No se pudo liberar el controlador para el juego".into());
        }
        Ok(Suspension(self))
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.get_mut().unwrap_or_else(|e| e.into_inner()).take() { let _ = worker.join(); }
    }
}

pub struct Suspension<'a>(&'a Controller);
impl Drop for Suspension<'_> {
    fn drop(&mut self) { let _ = self.0.commands.send(Command::Resume); }
}

fn run(shared: Arc<Mutex<Shared>>, commands: mpsc::Receiver<Command>, stop_requested: Arc<AtomicBool>) {
    let mut device = None;
    let mut edges = ZoneEdges::default();
    let mut paused = false;
    let mut retry = Instant::now();
    let mut last_valid = Instant::now();
    let mut buttons = None;
    let mut combo = StopCombo::default();
    let mut fn1 = Fn1Tap::default();
    let mut last_lights = None;
    let mut led_retry = Instant::now();
    let mut led_error = None;
    loop {
        let command = if (paused && buttons.is_none()) || (!paused && device.is_none()) {
            match commands.recv_timeout(Duration::from_millis(100)) {
                Ok(c) => Some(c), Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match commands.try_recv() {
                Ok(c) => Some(c), Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        };
        match command {
            Some(Command::Stop) => break,
            Some(Command::Pause(ack)) => {
                device = None; // Drop and close handles BEFORE acknowledging.
                edges = ZoneEdges::default();
                paused = true;
                retry = Instant::now();
                combo = StopCombo::default();
                fn1 = Fn1Tap::default();
                stop_requested.store(false, Ordering::SeqCst);
                shared.lock().unwrap_or_else(|e| e.into_inner()).reset("Controlador cedido al juego");
                let _ = ack.send(());
            }
            Some(Command::Resume) => {
                buttons = None;
                paused = false;
                retry = Instant::now();
                edges = ZoneEdges::default();
            }
            None => {}
        }
        if paused {
            if buttons.is_none() && Instant::now() >= retry {
                retry = Instant::now() + Duration::from_secs(1);
                match TasollerHid::open() {
                    Ok(Some(hid)) => { buttons = Some(hid); combo = StopCombo::default(); fn1 = Fn1Tap::default(); last_valid = Instant::now(); }
                    Ok(None) => shared.lock().unwrap_or_else(|e| e.into_inner()).status = "Juego activo · TASOLLER desconectado".into(),
                    Err(e) => shared.lock().unwrap_or_else(|e| e.into_inner()).status = format!("Juego activo · no se puede leer FN1 + FN2: {e}"),
                }
            }
            if let Some(hid) = buttons.as_ref() {
                match hid.read() {
                    Ok(Some(packet)) => {
                        if let Some(fn_buttons) = hid_fn_buttons(&packet) {
                            last_valid = Instant::now();
                            if combo.update(fn_buttons) { stop_requested.store(true, Ordering::SeqCst); }
                            if fn1.update(fn_buttons) { crate::win::request_track_skip(); }
                            shared.lock().unwrap_or_else(|e| e.into_inner()).status = "Juego activo · FN1 + FN2 para cerrar".into();
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        buttons = None;
                        combo = StopCombo::default();
                        fn1 = Fn1Tap::default();
                        shared.lock().unwrap_or_else(|e| e.into_inner()).status = format!("Lectura FN desconectada: {e}");
                    }
                }
            }
            if buttons.is_some() && last_valid.elapsed() > Duration::from_secs(1) {
                buttons = None;
                combo = StopCombo::default();
                fn1 = Fn1Tap::default();
            }
            continue;
        }
        if device.is_none() {
            if Instant::now() < retry { continue; }
            retry = Instant::now() + Duration::from_secs(1);
            match TasollerUsb::open() {
                Ok(Some(usb)) => {
                    device = Some(usb);
                    last_lights = None;
                    led_error = None;
                    led_retry = Instant::now();
                    edges = ZoneEdges::default();
                    last_valid = Instant::now();
                    shared.lock().unwrap_or_else(|e| e.into_inner()).reset("Esperando entradas de TASOLLER PLUS…");
                }
                Ok(None) => shared.lock().unwrap_or_else(|e| e.into_inner()).reset("Ningún TASOLLER PLUS conectado"),
                Err(e) => shared.lock().unwrap_or_else(|e| e.into_inner()).reset(format!("No se pudo abrir TASOLLER PLUS: {e}")),
            }
        }
        let Some(usb) = device.as_ref() else { continue; };
        match usb.read() {
            Ok(Some(packet)) => {
                if let Some(zones) = tasoller_zones(&packet) {
                    last_valid = Instant::now();
                    let lights = tasoller_lights(zones);
                    if last_lights != Some(lights) && Instant::now() >= led_retry {
                        match usb.write_lights(&lights) {
                            Ok(()) => { last_lights = Some(lights); led_error = None; }
                            Err(e) => { led_error = Some(e); led_retry = Instant::now() + Duration::from_secs(1); }
                        }
                    }
                    let action = edges.update(zones);
                    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    if s.device.is_none() { s.device = Some(DeviceInfo { vendor_id: 0x0e8f, product_id: 0x1231, name: "TASOLLER PLUS".into() }); }
                    s.raw = packet;
                    s.status = "Conectado · izquierda | derecha | volver | confirmar".into();
                    if let Some(e) = &led_error { s.status.push_str(&format!(" · LED: {e}")); }
                    if let Some(action) = action {
                        if s.actions.len() == 32 { s.actions.pop_front(); }
                        s.actions.push_back((action, Instant::now()));
                    }
                } else {
                    edges = ZoneEdges::default();
                    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    s.reset("Formato de entrada TASOLLER PLUS no reconocido");
                    s.raw = packet;
                }
            }
            Ok(None) => {}
            Err(e) => {
                device = None;
                edges = ZoneEdges::default();
                shared.lock().unwrap_or_else(|e| e.into_inner()).reset(format!("TASOLLER PLUS desconectado: {e}"));
            }
        }
        if device.is_some() && last_valid.elapsed() > Duration::from_secs(1) {
            device = None;
            edges = ZoneEdges::default();
            shared.lock().unwrap_or_else(|e| e.into_inner()).reset("TASOLLER PLUS no entrega entradas válidas");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_drains_actions_and_discards_old_input() {
        let mut shared = Shared::default();
        shared.actions.push_back((Action::Confirm, Instant::now() - Duration::from_secs(1)));
        shared.actions.push_back((Action::Left, Instant::now()));
        let (commands, _receiver) = mpsc::channel();
        let controller = Controller {
            shared: Arc::new(Mutex::new(shared)), commands, worker: Mutex::new(None),
            stop_requested: Arc::new(AtomicBool::new(false)),
        };
        assert_eq!(controller.snapshot().actions, vec!["left"]);
        assert!(controller.snapshot().actions.is_empty());
    }

    #[test]
    fn disconnect_or_pause_clears_input_and_identity() {
        let mut shared = Shared::default();
        shared.device = Some(DeviceInfo { vendor_id: 0x0e8f, product_id: 0x1231, name: "TASOLLER PLUS".into() });
        shared.raw = vec![1; 36];
        shared.actions.push_back((Action::Confirm, Instant::now()));
        shared.reset("Desconectado");
        assert!(shared.device.is_none());
        assert!(shared.raw.is_empty());
        assert!(shared.actions.is_empty());
        assert_eq!(shared.status, "Desconectado");
    }
}
