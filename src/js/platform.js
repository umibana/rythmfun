// Yuancon vendor id (tassa PID 0x10/0x11), same identities as yuancon.app.
// Filter by vendor only: the 0xFF71 collection yuancon.app uses carries config replies, not buttons.
const FILTERS = [{ vendorId: 0x5f73 }];

// ask=true shows the browser chooser (needs a click); ask=false reopens devices granted before.
export async function hidOpen(ask, onReport) {
  if (!("hid" in navigator)) throw new Error("WebHID no disponible en este navegador");
  const devices = ask
    ? await navigator.hid.requestDevice({ filters: FILTERS })
    : await navigator.hid.getDevices();
  if (devices.length === 0) return null;
  for (const d of devices) {
    if (!d.opened) await d.open();
    if (d.__rfl) continue; // already listening
    d.__rfl = true;
    d.addEventListener("inputreport", (e) =>
      onReport(d.vendorId, d.productId, e.reportId,
        new Uint8Array(e.data.buffer, e.data.byteOffset, e.data.byteLength)));
  }
  const d = devices[0];
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
