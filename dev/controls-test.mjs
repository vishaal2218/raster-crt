// Live check of every panel control, the way a person uses them: real mouse drags on each
// knob and fader and clicks on each switch, sent into the panel inside Premiere through its
// debug port (8793). After each one it reads the clip's Raster CRT back from Premiere and
// checks the value arrived. Needs a clip with Raster CRT selected in the test project.
//   node dev/controls-test.mjs [--only=knobs]
import fs from 'node:fs';

const PORT = 8793;
const SETTLE_MS = 900;
const KNOB_DRAG_PX = 50;
const FADER_AT = 0.7;
const { params } = JSON.parse(fs.readFileSync(new URL('../panel/params.json', import.meta.url), 'utf8'));
const byKey = new Map(params.map((p) => [p.key, p]));
const onlyKnobs = process.argv.includes('--only=knobs');
const onlyModule = (process.argv.find((a) => a.startsWith('--module=')) || '').slice('--module='.length);

const targets = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
const ws = new WebSocket((targets.find((t) => t.type === 'page') ?? targets[0]).webSocketDebuggerUrl);
await new Promise((resolve) => ws.addEventListener('open', resolve));
let nextId = 1;
const pending = new Map();
ws.addEventListener('message', (m) => {
  const msg = JSON.parse(m.data);
  if (pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  }
});
const send = (method, params = {}) => new Promise((resolve) => {
  const id = nextId++;
  pending.set(id, resolve);
  ws.send(JSON.stringify({ id, method, params }));
});
const evaluate = async (expression) => {
  const r = (await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })).result;
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails).slice(0, 400));
  return r.result.value;
};
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const mouse = (type, x, y) => send('Input.dispatchMouseEvent', { type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1 });

async function drag(from, to, steps = 6) {
  await mouse('mousePressed', from.x, from.y);
  for (let i = 1; i <= steps; i++) {
    await mouse('mouseMoved', from.x + ((to.x - from.x) * i) / steps, from.y + ((to.y - from.y) * i) / steps);
    await wait(30);
  }
  await mouse('mouseReleased', to.x, to.y);
}

const close = (a, b) => (typeof a === 'boolean' || typeof b === 'boolean' ? Boolean(a) === Boolean(b) : Math.abs(Number(a) - Number(b)) <= 1e-3 * Math.max(1, Math.abs(Number(b))));
const toHost = (p, v) => (p.type === 'checkbox' ? Boolean(v) : Number(v));

// Setup: defaults everywhere, every module and the GLOBAL section open.
console.log(await evaluate(`(async () => {
  const { sync } = window.rasterCrt;
  await sync.resetAll(); await sync.settled();
  document.querySelectorAll('.module-expand[aria-expanded="false"]').forEach((b) => b.click());
  if (document.getElementById('service').getAttribute('aria-expanded') !== 'true') document.getElementById('service').click();
  return 'reset and opened ' + document.querySelectorAll('.module').length + ' modules';
})()`));
await wait(1500);

const selector = onlyKnobs ? '.knob' : '.knob, .fader-track, .segmented [role=radio], .toggle input, select.select';
const scopes = onlyModule ? onlyModule.split(',').map((m) => `.module[aria-label="${m}"] `) : [''];
const scoped = scopes.flatMap((scope) => selector.split(', ').map((part) => scope + part)).join(', ');
const count = await evaluate(`window.__controls = [...document.querySelectorAll(${JSON.stringify(scoped)})];
  window.__log = [];
  if (!window.__watching) {
    window.__watching = true;
    window.rasterCrt.store.subscribe((k, v, o, s) => window.__log.push(k + '=' + (typeof v === 'number' ? +v.toFixed(3) : v) + ' from ' + (o === 'host' ? 'Premiere' : o?.className || o?.tagName || o) + ' (' + s + ')'));
    ['mousedown', 'mouseup'].forEach((n) => document.addEventListener(n, (e) => window.__log.push(n + ' on ' + (e.target.closest('[aria-label]')?.getAttribute('aria-label') || e.target.className) + ' buttons=' + e.buttons), true));
  }
  window.__controls.length`);
const results = [];
for (let i = 0; i < count; i++) {
  const info = await evaluate(`(() => {
    const el = window.__controls[${i}];
    el.scrollIntoView({ block: 'center' });
    window.__before = window.rasterCrt.store.snapshot();
    window.__log = [];
    const r = el.getBoundingClientRect();
    const kind = el.matches('.knob') ? 'knob' : el.matches('.fader-track') ? 'fader' : el.matches('select') ? 'select' : el.matches('input') ? 'toggle' : 'segment';
    const where = el.closest('.module')?.getAttribute('aria-label') || (el.closest('.global') ? 'Global' : '');
    const name = el.getAttribute('aria-label') || el.closest('.field, .toggle')?.querySelector('.field-label, span:last-child')?.textContent || el.textContent;
    return { kind, where, name, x: r.left + r.width / 2, y: r.top + r.height / 2, left: r.left, width: r.width, checked: el.getAttribute?.('aria-checked'), atMax: el.getAttribute('aria-valuenow') === el.getAttribute('aria-valuemax') };
  })()`);
  if (info.kind === 'segment' && info.checked === 'true') continue; // clicking the chosen option changes nothing
  // a knob already at its top (Dry/Wet starts at 100%) is turned down instead
  if (info.kind === 'knob') await drag({ x: info.x, y: info.y }, { x: info.x, y: info.y + (info.atMax ? KNOB_DRAG_PX : -KNOB_DRAG_PX) });
  else if (info.kind === 'fader') await drag({ x: info.left + info.width * (info.atMax ? FADER_AT : 0.3), y: info.y }, { x: info.left + info.width * (info.atMax ? 0.3 : FADER_AT), y: info.y });
  else if (info.kind === 'select') await evaluate(`(() => { const s = window.__controls[${i}]; s.value = String((Number(s.value) + 1) % s.options.length); s.dispatchEvent(new Event('change')); })()`);
  else { await mouse('mousePressed', info.x, info.y); await mouse('mouseReleased', info.x, info.y); }
  await wait(SETTLE_MS);
  const check = await evaluate(`(async () => {
    const { store, sync, bridge } = window.rasterCrt;
    const after = store.snapshot();
    const changed = Object.keys(after).filter((k) => after[k] !== window.__before[k]);
    await sync.settled();
    const host = await bridge.call('read');
    return { changed: changed.map((k) => [k, after[k]]), host: host.values, log: window.__log.slice() };
  })()`);
  const label = `${info.where ? info.where + ' › ' : ''}${info.name.trim()} (${info.kind})`;
  if (!check.changed.length) {
    results.push({ label, ok: false, why: 'the panel value did not move' });
    continue;
  }
  const problems = check.changed.flatMap(([key, value]) => {
    const p = byKey.get(key);
    if (!p || key === 'preset') return [];
    const host = check.host[p.label];
    return close(host, toHost(p, value)) ? [] : [`${p.label}: panel ${value}, Premiere ${host}`];
  });
  if (problems.length) console.log('  changes during ' + label + ':\n    ' + check.log.join('\n    '));
  results.push({ label, ok: problems.length === 0, why: problems.join('; ') || check.changed.map(([k, v]) => `${k}=${typeof v === 'number' ? +v.toFixed(3) : v}`).join(', ') });
}

await evaluate(`window.rasterCrt.sync.resetAll().then(() => 'reset')`);
const failed = results.filter((r) => !r.ok);
results.forEach((r) => console.log(`${r.ok ? 'PASS' : 'FAIL'}  ${r.label}  ${r.why}`));
console.log(`\n${results.length - failed.length} of ${results.length} controls reached Premiere`);
ws.close();
process.exit(failed.length ? 1 : 0);
