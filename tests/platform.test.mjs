import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('../src/js/platform.js', import.meta.url), 'utf8');
const fresh = () => import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}#${Math.random()}`);

test('native controller opens without WebHID and dispatches its snapshot', async () => {
  const originalInterval = globalThis.setInterval;
  const originalWindow = globalThis.window;
  const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  let poll;
  const device = { vendorId: 0x0e8f, productId: 0x1231, name: 'TASOLLER PLUS' };
  let snapshot = { device, raw: [68, 66, 84, 0, ...Array(32).fill(0)], actions: ['left'], status: 'Conectado' };
  const actions = [], devices = [], reports = [], statuses = [];
  globalThis.setInterval = fn => { poll = fn; return 1; };
  globalThis.window = { __TAURI__: { core: { invoke: async command => {
    assert.equal(command, 'controller_snapshot');
    return snapshot;
  } } } };
  Object.defineProperty(globalThis, 'navigator', { value: {}, configurable: true });
  try {
    const platform = await fresh();
    assert.deepEqual(await platform.hidOpen(false, (...r) => reports.push(r), a => actions.push(a),
      d => devices.push(d), s => statuses.push(s)), device);
    assert.deepEqual(actions, ['left']);
    assert.equal(reports[0][0], 0x0e8f);
    assert.deepEqual([...reports[0][3]], snapshot.raw);
    assert.equal(typeof poll, 'function');
    snapshot = { device: null, raw: [], actions: [], status: 'Desconectado' };
    await poll();
    assert.equal(devices.at(-1), null);
    assert.equal(statuses.at(-1), 'Desconectado');
  } finally {
    globalThis.setInterval = originalInterval;
    globalThis.window = originalWindow;
    if (originalNavigator) Object.defineProperty(globalThis, 'navigator', originalNavigator);
    else delete globalThis.navigator;
  }
});

test('WebHID reports an open failure instead of a false connection', async () => {
  const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  const originalWindow = globalThis.window;
  globalThis.window = {};
  const device = { vendorId: 0x5f73, productId: 0x10, productName: 'Yuancon',
    opened: false, open: async () => { throw new Error('ocupado'); } };
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { hid: {
    addEventListener() {}, getDevices: async () => [device], requestDevice: async () => [device],
  } } });
  try {
    const platform = await fresh();
    await assert.rejects(platform.hidOpen(true, () => {}, () => {}, () => {}, () => {}), /ocupado/);
  } finally {
    globalThis.window = originalWindow;
    if (originalNavigator) Object.defineProperty(globalThis, 'navigator', originalNavigator);
    else delete globalThis.navigator;
  }
});

test('WebHID preserves other controllers on disconnect and restores reconnected identities', async () => {
  const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  const originalWindow = globalThis.window;
  const listeners = {}, snapshots = [];
  const makeDevice = productId => ({ vendorId: 0x5f73, productId, productName: `Yuancon ${productId}`,
    opened: true, addEventListener() {} });
  const first = makeDevice(0x10), second = makeDevice(0x11);
  globalThis.window = {};
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { hid: {
    addEventListener(event, fn) { listeners[event] = fn; }, getDevices: async () => [first, second],
  } } });
  try {
    const platform = await fresh();
    await platform.hidOpen(false, () => {}, () => {}, d => snapshots.push(d), () => {});
    assert.deepEqual(snapshots.at(-1).map(d => d.productId), [0x10, 0x11]);
    listeners.disconnect({ device: first });
    assert.deepEqual(snapshots.at(-1).map(d => d.productId), [0x11]);
    listeners.connect({ device: first });
    assert.deepEqual(snapshots.at(-1).map(d => d.productId).sort(), [0x10, 0x11]);
  } finally {
    globalThis.window = originalWindow;
    if (originalNavigator) Object.defineProperty(globalThis, 'navigator', originalNavigator);
    else delete globalThis.navigator;
  }
});
