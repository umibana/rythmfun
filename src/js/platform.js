// Yuancon vendor id (tassa PID 0x10/0x11), same identities as yuancon.app.
// Filter by vendor only: the 0xFF71 collection yuancon.app uses carries config replies, not buttons.
const FILTERS = [{ vendorId: 0x5f73 }];

let report = null; // latest callback; reports from replugged devices go here too
let deviceChanged = null;
let statusChanged = null;
let nativeCallbacks = null;
let nativeTimer = null;
let nativeBusy = false;
let nativeDevice = null;
const hidDevices = new Map();
const notifyDevices = () => deviceChanged?.([...hidDevices.values()]);

async function pollNative() {
  if (nativeBusy) return nativeDevice;
  nativeBusy = true;
  try {
    const snapshot = await window.__TAURI__.core.invoke("controller_snapshot");
    nativeDevice = snapshot.device;
    nativeCallbacks.device?.(snapshot.device);
    nativeCallbacks.status?.(snapshot.status);
    if (snapshot.device && snapshot.raw.length) {
      nativeCallbacks.report(snapshot.device.vendorId, snapshot.device.productId, 0,
        new Uint8Array(snapshot.raw));
    }
    for (const action of snapshot.actions) nativeCallbacks.action?.(action);
    return snapshot.device;
  } catch (error) {
    nativeDevice = null;
    nativeCallbacks.device?.(null);
    nativeCallbacks.status?.(`Error de controlador: ${error}`);
    throw error;
  } finally {
    nativeBusy = false;
  }
}

async function attach(d) {
  if (!d.opened) await d.open();
  hidDevices.set(d, { vendorId: d.vendorId, productId: d.productId, name: d.productName || "HID" });
  if (d.__rfl) { notifyDevices(); return; } // already listening
  d.__rfl = true;
  d.addEventListener("inputreport", (e) =>
    report?.(d.vendorId, d.productId, e.reportId,
      new Uint8Array(e.data.buffer, e.data.byteOffset, e.data.byteLength)));
  notifyDevices();
}

let watching = false;

// ask=true shows the browser chooser (needs a click); ask=false reopens devices granted before.
export async function hidOpen(ask, onReport, onAction, onDevice, onStatus) {
  // Desktop: native WinUSB does not require a browser chooser or device-mode change.
  if (globalThis.window?.__TAURI__?.core?.invoke) {
    nativeCallbacks = { report: onReport, action: onAction, device: onDevice, status: onStatus };
    if (nativeTimer === null) {
      nativeTimer = setInterval(() => pollNative().catch(() => {}), 32);
    }
    return pollNative();
  }
  if (!("hid" in navigator)) throw new Error("WebHID no disponible en este navegador");
  report = onReport;
  deviceChanged = onDevice;
  statusChanged = onStatus;
  if (!watching) {
    watching = true;
    navigator.hid.addEventListener("connect", (e) => {
      if (e.device.vendorId === 0x5f73) attach(e.device).catch(err => statusChanged?.(String(err)));
    });
    navigator.hid.addEventListener("disconnect", (e) => {
      if (e.device.vendorId === 0x5f73) {
        hidDevices.delete(e.device);
        notifyDevices();
        statusChanged?.("Controlador desconectado");
      }
    });
  }
  const devices = ask
    ? await navigator.hid.requestDevice({ filters: FILTERS })
    : await navigator.hid.getDevices();
  const supported = devices.filter(d => d.vendorId === 0x5f73);
  if (supported.length === 0) { notifyDevices(); return null; }
  const opened = [];
  let error;
  for (const d of supported) {
    try { await attach(d); opened.push(d); } catch (err) { error = err; }
  }
  if (!opened.length) throw error;
  const d = opened[0];
  return { vendorId: d.vendorId, productId: d.productId, name: d.productName || "HID" };
}

export function readAsDataUrl(file) {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result);
    r.onerror = () => reject(r.error);
    r.readAsDataURL(file);
  });
}
