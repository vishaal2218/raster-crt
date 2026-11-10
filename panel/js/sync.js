// Keeps the panel's store and the selected clip's Raster CRT controls in step.
// Premiere sends no events when effect values change, so the panel polls while visible.
// A control the user just touched ignores Premiere for a moment, so knobs never jump back.

export const HOST = 'host';
const POLL_MS = 500;
const LOCAL_WINS_MS = 800;
const DRAG_WRITE_MS = 100;
// Premiere's scripting reports popups from 0 (checked live on 26.5: Quality = HI reads 1).
const POPUP_HOST_BASE = 0;

export function fromHost(param, raw) {
  if (param.type === 'popup') return Number(raw) - POPUP_HOST_BASE;
  if (param.type === 'checkbox') return Boolean(raw);
  return Number(raw);
}

export function toHost(param, value) {
  if (param.type === 'popup') return Number(value) + POPUP_HOST_BASE;
  if (param.type === 'checkbox') return Boolean(value);
  return Number(value);
}

/**
 * @param {object} o
 * @param {{call: Function}} o.bridge      from bridge.js
 * @param {object[]} o.params              params.json entries
 * @param {object} o.store                 from primitives.createStore
 * @param {(sel: object) => void} o.onSelection
 * @param {(err: Error|null) => void} o.onError
 * @param {(read: {values: object, keyframed: string[]}) => void} [o.onRead]  after each read
 * @param {() => boolean} [o.isHidden]
 * @param {() => number} [o.now]
 */
export function createSync({ bridge, params, store, onSelection, onError, onRead = () => {}, isHidden = () => document.hidden, now = Date.now }) {
  const byKey = new Map(params.map((p) => [p.key, p]));
  const localUntil = new Map();
  const pending = new Map();
  let chain = Promise.resolve();
  let flushTimer = null;
  let pollTimer = null;
  let hasEffect = false;

  function applyHost(values) {
    for (const p of params) {
      if (!(p.label in values) || (localUntil.get(p.key) ?? 0) > now()) continue;
      const v = fromHost(p, values[p.label]);
      if (store.get(p.key) !== v) store.set(p.key, v, HOST, 'host');
    }
  }

  async function pollOnce() {
    try {
      const sel = await bridge.call('selection');
      hasEffect = Boolean(sel.hasEffect);
      onSelection(sel);
      if (hasEffect) {
        const read = await bridge.call('read');
        applyHost(read.values);
        onRead(read);
      }
      onError(null);
    } catch (err) {
      onError(err);
    }
  }

  async function loop() {
    if (!isHidden()) await pollOnce();
    pollTimer = setTimeout(loop, POLL_MS);
  }

  async function flush() {
    const batch = [...pending.entries()];
    pending.clear();
    for (const [key, value] of batch) {
      const p = byKey.get(key);
      try {
        await bridge.call('write', { label: p.label, value: toHost(p, value) });
      } catch (err) {
        onError(err);
      }
    }
  }

  // Host writes run one after another. A failed one is reported and the queue carries on.
  function enqueue(task) {
    const run = chain.then(task);
    chain = run.catch((err) => onError(err));
    return run;
  }

  function schedule(delay) {
    if (flushTimer) return;
    flushTimer = setTimeout(() => {
      flushTimer = null;
      enqueue(flush);
    }, delay);
  }

  store.subscribe((key, value, origin, source) => {
    if (origin === HOST || !hasEffect || !byKey.has(key)) return;
    localUntil.set(key, now() + LOCAL_WINS_MS);
    pending.set(key, value);
    schedule(source === 'drag' ? DRAG_WRITE_MS : 0);
  });

  /**
   * Sets every control to `values` (by key) in one host call, Preset last: if the effect
   * applies a factory look when Preset changes, it lands on these same values.
   */
  function applyAll(values, presetIndex = 0) {
    const until = now() + LOCAL_WINS_MS;
    const writes = [];
    let presetWrite = null;
    for (const p of params) {
      const v = p.key === 'preset' ? presetIndex : values[p.key];
      if (v === undefined) continue;
      localUntil.set(p.key, until);
      store.set(p.key, v, HOST, 'apply');
      const write = { label: p.label, value: toHost(p, v) };
      if (p.key === 'preset') presetWrite = write;
      else writes.push(write);
    }
    if (presetWrite) writes.push(presetWrite);
    return enqueue(() => bridge.call('writeMany', { values: writes }));
  }

  /** Puts every control back to its default in one host call. */
  function resetAll() {
    return applyAll(Object.fromEntries(params.map((p) => [p.key, p.default])), 0);
  }

  return {
    start: loop,
    stop: () => clearTimeout(pollTimer),
    pollOnce,
    applyAll,
    resetAll,
    settled: () => chain,
  };
}
