import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createSync, fromHost, toHost, HOST } from '../../panel/js/sync.js';
import { createStore } from '../../panel/js/primitives.js';

const PARAMS = [
  { key: 'mix', label: 'Dry/Wet', type: 'float', default: 1 },
  { key: 'quality', label: 'Quality', type: 'popup', default: 1, options: ['SD', 'HI', 'FULL'] },
  { key: 'wave_rgb', label: 'Colour Bands', type: 'checkbox', default: false },
];
const defaults = () => Object.fromEntries(PARAMS.map((p) => [p.key, p.default]));
const tick = () => new Promise((r) => setTimeout(r, 5));

function fakeBridge({ hasEffect = true, values = {} } = {}) {
  const calls = [];
  return {
    calls,
    async call(name, args) {
      calls.push([name, args]);
      if (name === 'selection') return { sequence: 'Seq', clip: 'Clip', count: 1, hasEffect };
      if (name === 'read') return { values };
      return {};
    },
  };
}

function setup(opts = {}) {
  let clock = 1000;
  const store = createStore(defaults());
  const bridge = fakeBridge(opts);
  const seen = { selection: null, errors: [] };
  const sync = createSync({
    bridge, params: PARAMS, store, isHidden: () => false, now: () => clock,
    onSelection: (s) => { seen.selection = s; }, onError: (e) => seen.errors.push(e),
  });
  return { store, bridge, sync, seen, advance: (ms) => { clock += ms; } };
}

test('popups pass through as the 0-based index Premiere scripting uses', () => {
  assert.equal(fromHost(PARAMS[1], 2), 2);
  assert.equal(toHost(PARAMS[1], 2), 2);
  assert.equal(fromHost(PARAMS[2], 1), true);
  assert.equal(toHost(PARAMS[0], '0.5'), 0.5);
});

test('polling copies the clip values into the store without writing them back', async () => {
  const { store, bridge, sync, seen } = setup({ values: { 'Dry/Wet': 0.25, Quality: 2, 'Colour Bands': true } });
  await sync.pollOnce();
  await tick();
  assert.deepEqual([store.get('mix'), store.get('quality'), store.get('wave_rgb')], [0.25, 2, true]);
  assert.equal(seen.selection.clip, 'Clip');
  assert.ok(!bridge.calls.some(([n]) => n === 'write'), 'host values must not echo back');
});

test('without Raster CRT on the clip, nothing is read or written', async () => {
  const { store, bridge, sync } = setup({ hasEffect: false });
  await sync.pollOnce();
  store.set('mix', 0.5, 'knob');
  await tick();
  assert.deepEqual(bridge.calls.map(([n]) => n), ['selection']);
});

test('a panel edit is written to Premiere', async () => {
  const { store, bridge, sync } = setup();
  await sync.pollOnce();
  store.set('quality', 2, 'segmented');
  await tick();
  await sync.settled();
  assert.deepEqual(bridge.calls.find(([n]) => n === 'write'), ['write', { label: 'Quality', value: 2 }]);
});

test('a control just touched ignores Premiere until the panel edit settles', async () => {
  const { store, bridge, sync, advance } = setup({ values: { 'Dry/Wet': 1 } });
  await sync.pollOnce();
  store.set('mix', 0.4, 'knob');
  await sync.pollOnce();
  assert.equal(store.get('mix'), 0.4, 'stale host value must not undo the edit');
  advance(1000);
  bridge.calls.length = 0;
  await sync.pollOnce();
  assert.equal(store.get('mix'), 1, 'after the window, Premiere is the truth again');
});

test('reset writes every default in one host call', async () => {
  const { store, bridge, sync } = setup({ values: { 'Dry/Wet': 0.3 } });
  await sync.pollOnce();
  await sync.resetAll();
  const [, args] = bridge.calls.find(([n]) => n === 'writeMany');
  assert.deepEqual(args.values, [
    { label: 'Dry/Wet', value: 1 }, { label: 'Quality', value: 1 }, { label: 'Colour Bands', value: false },
  ]);
  assert.equal(store.get('mix'), 1);
});

test('host failures go to onError instead of throwing', async () => {
  const { sync, seen, bridge } = setup();
  bridge.call = async () => { throw Object.assign(new Error('x'), { code: 'BRIDGE' }); };
  await sync.pollOnce();
  assert.equal(seen.errors.at(-1).code, 'BRIDGE');
});

test('HOST origin is exported for callers that must not trigger writes', () => {
  assert.equal(HOST, 'host');
});

test('applying a look writes every control in one call with Preset last', async () => {
  const params = [...PARAMS, { key: 'preset', label: 'Preset', type: 'popup', default: 0 }];
  const store = createStore({ ...defaults(), preset: 0 });
  const bridge = fakeBridge();
  const sync = createSync({ bridge, params, store, isHidden: () => false, onSelection: () => {}, onError: () => {} });
  await sync.applyAll({ mix: 0.5, quality: 2, wave_rgb: true }, 3);
  const [, args] = bridge.calls.find(([n]) => n === 'writeMany');
  assert.deepEqual(args.values.at(-1), { label: 'Preset', value: 3 });
  assert.equal(args.values.length, 4);
  assert.equal(store.get('mix'), 0.5);
});

test('a failed write is reported and later writes still go out', async () => {
  const store = createStore(defaults());
  const errors = [];
  let fail = true;
  const calls = [];
  const bridge = { async call(name, args) {
    calls.push(name);
    if (name === 'selection') return { sequence: 'S', clip: 'C', count: 1, hasEffect: true };
    if (name === 'read') return { values: {} };
    if (name === 'write' && fail) { fail = false; throw Object.assign(new Error('x'), { code: 'KEYFRAMED' }); }
    return {};
  } };
  const sync = createSync({ bridge, params: PARAMS, store, isHidden: () => false, onSelection: () => {}, onError: (e) => e && errors.push(e) });
  await sync.pollOnce();
  store.set('mix', 0.2, 'knob');
  await tick(); await sync.settled().catch(() => {});
  store.set('mix', 0.3, 'knob');
  await tick(); await sync.settled().catch(() => {});
  assert.equal(calls.filter((n) => n === 'write').length, 2, 'the second write must still be sent');
  assert.equal(errors[0].code, 'KEYFRAMED');
});
