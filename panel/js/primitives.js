// Raster CRT panel primitives. Plain ES module, no dependencies.
// Each factory returns a DOM element. Elements expose setValue(v) for updates coming
// from Premiere; that path never fires onChange, so panel and host can't echo forever.

const KNOB_SWEEP_DEG = 270;
const DRAG_PX_FULL_RANGE = 200;
const FINE_FACTOR = 0.1;
// Scrolling: a wheel or trackpad moves a knob half as fast as dragging; one event never
// jumps more than WHEEL_MAX_PX, and the value is written for good once scrolling pauses.
const WHEEL_PX_FULL_RANGE = 400;
const WHEEL_LINE_PX = 16;
const WHEEL_MAX_PX = 40;
const WHEEL_COMMIT_MS = 250;
const KNOB_HINT = 'Drag up or right to turn up, down or left to turn down. Scroll works too; hold Shift for fine steps; double-click to reset.';
const SVG_NS = 'http://www.w3.org/2000/svg';

export const MODULE_ICONS = {
  colour: 'palette', bend: 'zap', damage: 'triangle-alert', noise: 'sparkles', waves: 'waves',
  screen: 'monitor', capture: 'camera', convergence: 'crosshair', glass: 'aperture',
};

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const rangeOf = (p) => p.max - p.min;
// clamp to the param range and drop float noise (0.7000000000000001 -> 0.7)
const VALUE_DECIMALS = 1e4;
const settle = (p, v) => Math.round(clamp(v, p.min, p.max) * VALUE_DECIMALS) / VALUE_DECIMALS;
const isBipolar = (p) => p.min < 0 && p.max > 0;

export function unitOf(param) {
  if (param.type === 'angle') return '°';
  const match = /\(([^)]+)\)$/.exec(param.label);
  return match ? match[1] : '';
}

export function plainLabel(param) {
  return param.label.replace(/\s*\([^)]+\)$/, '');
}

export function formatValue(param, value) {
  const unit = unitOf(param);
  if (unit === '1/x s') return `1/${Math.round(value)} s`;
  const range = rangeOf(param);
  const digits = range <= 2.5 ? 2 : range <= 50 ? 1 : 0;
  const text = value.toFixed(digits);
  if (unit === '°') return `${text}°`;
  return unit ? `${text} ${unit}` : text;
}

function stepFor(param, event) {
  if (event.key === 'PageUp' || event.key === 'PageDown') return rangeOf(param) * 0.1;
  return rangeOf(param) * (event.shiftKey ? 0.001 : 0.01);
}

// Shared slider keys (knob and fader). Returns the new value, or null for other keys.
function valueForKey(param, value, event) {
  switch (event.key) {
    case 'ArrowUp': case 'ArrowRight': case 'PageUp': return value + stepFor(param, event);
    case 'ArrowDown': case 'ArrowLeft': case 'PageDown': return value - stepFor(param, event);
    case 'Home': return param.min;
    case 'End': return param.max;
    case 'Backspace': case 'Delete': return param.default;
    default: return null;
  }
}

function setSliderAria(el, param, value) {
  el.setAttribute('role', 'slider');
  el.setAttribute('aria-label', plainLabel(param));
  el.setAttribute('aria-valuemin', String(param.min));
  el.setAttribute('aria-valuemax', String(param.max));
  el.setAttribute('aria-valuenow', String(value));
  el.setAttribute('aria-valuetext', formatValue(param, value));
}

function svgEl(tag, attrs) {
  const el = document.createElementNS(SVG_NS, tag);
  Object.entries(attrs).forEach(([k, v]) => el.setAttribute(k, String(v)));
  return el;
}

function polar(r, deg) {
  const rad = ((deg - 90) * Math.PI) / 180;
  return [50 + r * Math.cos(rad), 50 + r * Math.sin(rad)];
}

function arcPath(r, fromDeg, toDeg) {
  const end = toDeg - fromDeg < 0.01 ? fromDeg + 0.01 : toDeg;
  const [x0, y0] = polar(r, fromDeg);
  const [x1, y1] = polar(r, end);
  const large = end - fromDeg > 180 ? 1 : 0;
  return `M${x0} ${y0} A${r} ${r} 0 ${large} 1 ${x1} ${y1}`;
}

// Drags run on mouse events, not pointer events. Inside Premiere's panels a real press can
// arrive only as a pointermove, because the browser still thinks a button is down from a
// release it never saw, so pointerdown never fires (seen live 15-10-2026); mousedown always
// does. The move and release are watched on the whole document, so the drag keeps going
// past the control's edge, and a release the panel never heard ends it on the next move.
function onMouseDrag(el, { start, move, end }) {
  el.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault(); // no text selection, no native drag
    el.focus();
    start(e);
    const onMove = (ev) => {
      if (ev.buttons === 0) {
        finish();
        return;
      }
      move(ev);
    };
    const finish = () => {
      document.removeEventListener('mousemove', onMove, true);
      document.removeEventListener('mouseup', finish, true);
      window.removeEventListener('blur', finish);
      end();
    };
    document.addEventListener('mousemove', onMove, true);
    document.addEventListener('mouseup', finish, true);
    window.addEventListener('blur', finish);
  });
}

export function createKnob({ param, value = param.default, size = 30, onChange = () => {} }) {
  let current = settle(param, value);
  const angleFor = (v) => -KNOB_SWEEP_DEG / 2 + ((v - param.min) / rangeOf(param)) * KNOB_SWEEP_DEG;
  const zeroAngle = isBipolar(param) ? angleFor(0) : -KNOB_SWEEP_DEG / 2;

  const el = document.createElement('div');
  el.className = 'knob';
  el.style.width = el.style.height = `${size}px`;
  el.tabIndex = 0;

  const svg = svgEl('svg', { viewBox: '0 0 100 100', 'aria-hidden': 'true' });
  const track = svgEl('path', { class: 'k-track', d: arcPath(42, -KNOB_SWEEP_DEG / 2, KNOB_SWEEP_DEG / 2), fill: 'none', 'stroke-width': 9, 'stroke-linecap': 'round' });
  const arc = svgEl('path', { class: 'k-value', fill: 'none', 'stroke-width': 9, 'stroke-linecap': 'round' });
  const [tx0, ty0] = polar(48, angleFor(param.default));
  const [tx1, ty1] = polar(36, angleFor(param.default));
  const tick = svgEl('line', { class: 'k-default', x1: tx0, y1: ty0, x2: tx1, y2: ty1, 'stroke-width': 3 });
  const cap = svgEl('circle', { class: 'k-cap', cx: 50, cy: 50, r: 30, 'stroke-width': 2 });
  const pointer = svgEl('line', { class: 'k-pointer', 'stroke-width': 7, 'stroke-linecap': 'round' });
  svg.append(track, arc, tick, cap, pointer);
  el.append(svg);

  const tip = document.createElement('div');
  tip.className = 'knob-tip';
  tip.hidden = true;
  el.append(tip);

  function render() {
    const a = angleFor(current);
    arc.setAttribute('d', arcPath(42, Math.min(zeroAngle, a), Math.max(zeroAngle, a)));
    const [x0, y0] = polar(8, a);
    const [x1, y1] = polar(24, a);
    Object.entries({ x1: x0, y1: y0, x2: x1, y2: y1 }).forEach(([k, v]) => pointer.setAttribute(k, v));
    setSliderAria(el, param, current);
    tip.textContent = formatValue(param, current);
  }

  function set(v, source) {
    const next = settle(param, v);
    if (next === current && source !== 'commit') return;
    current = next;
    render();
    onChange(current, source);
  }

  // Up and right both turn a knob up, so a sideways or circular drag works as well.
  let drag = null;
  el.title = KNOB_HINT;
  onMouseDrag(el, {
    start: (e) => {
      drag = { x: e.clientX, y: e.clientY, start: current };
      el.classList.add('is-dragging');
      tip.hidden = false;
    },
    move: (e) => {
      const factor = e.shiftKey ? FINE_FACTOR : 1;
      const moved = drag.y - e.clientY + (e.clientX - drag.x);
      set(drag.start + (moved / DRAG_PX_FULL_RANGE) * rangeOf(param) * factor, 'drag');
    },
    end: () => {
      drag = null;
      el.classList.remove('is-dragging');
      tip.hidden = true;
      set(current, 'commit');
    },
  });
  el.addEventListener('dblclick', () => set(param.default, 'commit'));
  let wheelTimer = null;
  el.addEventListener('wheel', (e) => {
    e.preventDefault();
    const factor = e.shiftKey ? FINE_FACTOR : 1;
    const px = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? e.deltaY * WHEEL_LINE_PX : e.deltaY;
    const step = clamp(px, -WHEEL_MAX_PX, WHEEL_MAX_PX) / WHEEL_PX_FULL_RANGE;
    set(current - step * rangeOf(param) * factor, 'drag');
    clearTimeout(wheelTimer);
    wheelTimer = setTimeout(() => set(current, 'commit'), WHEEL_COMMIT_MS);
  }, { passive: false });
  el.addEventListener('keydown', (e) => {
    const next = valueForKey(param, current, e);
    if (next === null) return;
    e.preventDefault();
    set(next, 'commit');
  });

  el.setValue = (v) => { current = settle(param, v); render(); };
  render();
  return el;
}

export function createFader({ param, value = param.default, keyframed = false, onChange = () => {} }) {
  let current = settle(param, value);
  const pos = (v) => (v - param.min) / rangeOf(param);

  const root = document.createElement('div');
  root.className = 'fader';
  root.innerHTML = `
    <div class="fader-top">
      <span class="fader-label">${plainLabel(param)}${keyframed ? '<i data-lucide="diamond" class="kf-marker" aria-label="has keyframes"></i>' : ''}</span>
      <button type="button" class="fader-value" title="Double-click to type a value"></button>
    </div>
    <div class="fader-track" tabindex="0"><div class="fader-fill"></div><div class="fader-thumb"></div></div>
    <div class="fader-error" role="alert" hidden></div>`;
  const valueBtn = root.querySelector('.fader-value');
  const track = root.querySelector('.fader-track');
  const fill = root.querySelector('.fader-fill');
  const thumb = root.querySelector('.fader-thumb');
  const error = root.querySelector('.fader-error');

  function render() {
    const p = pos(current);
    const z = isBipolar(param) ? pos(0) : 0;
    fill.style.left = `${Math.min(p, z) * 100}%`;
    fill.style.width = `${Math.abs(p - z) * 100}%`;
    thumb.style.left = `${p * 100}%`;
    valueBtn.textContent = formatValue(param, current);
    setSliderAria(track, param, current);
  }

  function set(v, source) {
    current = settle(param, v);
    render();
    onChange(current, source);
  }

  const fromPointer = (e) => {
    const box = track.getBoundingClientRect();
    return param.min + clamp((e.clientX - box.left) / box.width, 0, 1) * rangeOf(param);
  };
  onMouseDrag(track, {
    start: (e) => set(fromPointer(e), 'drag'),
    move: (e) => set(fromPointer(e), 'drag'),
    end: () => set(current, 'commit'),
  });
  track.addEventListener('dblclick', () => set(param.default, 'commit'));
  track.addEventListener('keydown', (e) => {
    const next = valueForKey(param, current, e);
    if (next === null) return;
    e.preventDefault();
    set(next, 'commit');
  });

  valueBtn.addEventListener('dblclick', () => {
    const input = document.createElement('input');
    input.className = 'fader-input';
    input.inputMode = 'decimal';
    input.setAttribute('aria-label', `${plainLabel(param)} value`);
    input.value = String(Number(current.toFixed(3)));
    valueBtn.replaceWith(input);
    input.focus();
    input.select();
    const close = () => { input.replaceWith(valueBtn); error.hidden = true; render(); };
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { close(); return; }
      if (e.key !== 'Enter') return;
      const typed = Number.parseFloat(input.value);
      if (Number.isNaN(typed)) {
        input.classList.add('is-error');
        error.textContent = `Type a number between ${param.min} and ${param.max}.`;
        error.hidden = false;
        return;
      }
      set(typed, 'commit');
      close();
    });
    input.addEventListener('blur', close);
  });

  root.setValue = (v) => { current = settle(param, v); render(); };
  render();
  return root;
}

export function createSegmented({ param, value = param.default, onChange = () => {} }) {
  const field = document.createElement('div');
  field.className = 'field';
  field.innerHTML = `<span class="field-label">${param.label}</span><div class="segmented" role="radiogroup" aria-label="${param.label}"></div>`;
  const group = field.querySelector('.segmented');
  const buttons = param.options.map((label, i) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.setAttribute('role', 'radio');
    b.textContent = label;
    b.addEventListener('click', () => select(i, true));
    b.addEventListener('keydown', (e) => {
      const delta = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
      if (!delta) return;
      e.preventDefault();
      select((i + delta + param.options.length) % param.options.length, true);
      buttons[(i + delta + param.options.length) % param.options.length].focus();
    });
    return b;
  });
  group.append(...buttons);
  function select(index, fire) {
    buttons.forEach((b, i) => {
      b.setAttribute('aria-checked', String(i === index));
      b.tabIndex = i === index ? 0 : -1;
    });
    if (fire) onChange(index, 'commit');
  }
  field.setValue = (i) => select(i, false);
  select(value, false);
  return field;
}

export function createSelect({ param, value = param.default, onChange = () => {} }) {
  const field = document.createElement('label');
  field.className = 'field';
  field.innerHTML = `<span class="field-label">${param.label}</span><select class="select"></select>`;
  const select = field.querySelector('select');
  param.options.forEach((label, i) => select.add(new Option(label, String(i))));
  select.value = String(value);
  select.addEventListener('change', () => onChange(Number(select.value), 'commit'));
  field.setValue = (i) => { select.value = String(i); };
  return field;
}

export function createToggle({ param, value = param.default, onChange = () => {} }) {
  const label = document.createElement('label');
  label.className = 'toggle';
  label.innerHTML = `<input type="checkbox"><span class="lamp" aria-hidden="true"></span><span>${param.label}</span>`;
  const input = label.querySelector('input');
  input.checked = Boolean(value);
  input.addEventListener('change', () => onChange(input.checked, 'commit'));
  label.setValue = (v) => { input.checked = Boolean(v); };
  return label;
}

// One immutable snapshot of every control value. Each change replaces the snapshot and
// tells subscribers which control changed and which element changed it (the origin), so
// two controls bound to the same parameter stay in step without echoing.
export function createStore(initial) {
  let state = Object.freeze({ ...initial });
  const listeners = new Set();
  return {
    get: (key) => state[key],
    snapshot: () => state,
    set(key, value, origin = null, source = 'commit') {
      state = Object.freeze({ ...state, [key]: value });
      listeners.forEach((fn) => fn(key, value, origin, source));
    },
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
}

// Builds a control with `factory` and keeps it in step with `store` both ways.
export function bound(factory, param, store, extra = {}) {
  let el = null;
  const onChange = (v, source) => store.set(param.key, v, el, source);
  el = factory({ param, value: store.get(param.key), onChange, ...extra });
  if (!el) return null;
  store.subscribe((key, v, origin) => {
    if (key === param.key && origin !== el) el.setValue(v);
  });
  return el;
}

// Picks the right control for a parameter, as components.md says.
export function createControl({ param, value, onChange, keyframed = false }) {
  if (param.type === 'float' || param.type === 'angle') return createFader({ param, value, keyframed, onChange });
  if (param.type === 'checkbox') return createToggle({ param, value, onChange });
  if (param.type === 'popup' && Array.isArray(param.options)) {
    return param.options.length <= 3
      ? createSegmented({ param, value, onChange })
      : createSelect({ param, value, onChange });
  }
  return null;
}

// A module is "doing something" when its master is up and any 0-based amount is above 0.
export function isModuleActive(params, values) {
  const master = params.find((p) => p.master);
  if (!master || values[master.key] <= 0) return false;
  return params.some((p) => !p.master && p.type === 'float' && p.min === 0 && !unitOf(p) && values[p.key] > 0);
}

// Rack row, laid out like the original's: [icon] [NAME centred] [master knob] [RESET] [+].
export function createModuleRow({ module, params, store, expanded = false, disabled = false, keyframed = [] }) {
  const master = params.find((p) => p.master);
  const details = params.filter((p) => !p.master);
  const keys = new Set(params.map((p) => p.key));
  const bodyId = `module-${module.key}`;

  const row = document.createElement('section');
  row.className = 'module';
  row.classList.toggle('is-disabled', disabled);
  row.setAttribute('aria-label', module.label);
  row.innerHTML = `
    <div class="module-head">
      <i data-lucide="${MODULE_ICONS[module.key]}" class="module-icon" aria-hidden="true"></i>
      <button type="button" class="module-name" aria-expanded="${expanded}" aria-controls="${bodyId}">
        ${module.label}${keyframed.length ? '<i data-lucide="diamond" class="kf-marker" aria-label="has keyframes"></i>' : ''}
      </button>
      <span class="master-slot"></span>
      <button type="button" class="key is-reset" aria-label="Reset ${module.label}">Reset</button>
      <button type="button" class="module-expand" aria-expanded="${expanded}" aria-controls="${bodyId}" aria-label="${module.label} controls">
        <i data-lucide="plus" class="icon-closed" aria-hidden="true"></i><i data-lucide="minus" class="icon-open" aria-hidden="true"></i>
      </button>
    </div>
    <div class="module-body" id="${bodyId}" role="region" aria-label="${module.label} controls" ${expanded ? '' : 'hidden'}></div>`;

  row.querySelector('.master-slot').replaceWith(bound(createKnob, master, store, { size: 30 }));
  const body = row.querySelector('.module-body');
  details
    .map((p) => bound((opts) => createControl({ ...opts, keyframed: keyframed.includes(p.key) }), p, store))
    .filter(Boolean)
    .forEach((el) => body.append(el));

  const refreshActive = () => row.classList.toggle('is-active', isModuleActive(params, store.snapshot()));
  store.subscribe((key) => { if (keys.has(key)) refreshActive(); });

  row.querySelector('.key.is-reset').addEventListener('click', () => {
    params.forEach((p) => store.set(p.key, p.default));
  });
  const toggles = row.querySelectorAll('[aria-controls]');
  toggles.forEach((t) => t.addEventListener('click', () => {
    const open = t.getAttribute('aria-expanded') !== 'true';
    toggles.forEach((x) => x.setAttribute('aria-expanded', String(open)));
    body.hidden = !open;
  }));
  refreshActive();
  return row;
}

// Knob with its label above and a typeable readout below (GLOBAL grid).
export function createKnobCell({ param, store, label = plainLabel(param) }) {
  const id = `readout-${param.key}`;
  const cell = document.createElement('div');
  cell.className = 'knob-cell';
  cell.innerHTML = `<label class="knob-cell-label" for="${id}">${label}</label><span class="knob-slot"></span>
    <input id="${id}" class="knob-readout" inputmode="decimal" autocomplete="off" spellcheck="false">`;
  const readout = cell.querySelector('input');
  cell.querySelector('.knob-slot').replaceWith(bound(createKnob, param, store, { size: 44 }));

  const show = (v) => { readout.value = Number(v).toFixed(2); };
  show(store.get(param.key));
  store.subscribe((key, v) => { if (key === param.key && document.activeElement !== readout) show(v); });
  readout.addEventListener('change', () => {
    const typed = Number.parseFloat(readout.value);
    if (Number.isNaN(typed)) {
      readout.setAttribute('aria-invalid', 'true');
      return;
    }
    readout.removeAttribute('aria-invalid');
    store.set(param.key, settle(param, typed), readout);
    show(store.get(param.key));
  });
  return cell;
}

// The GLOBAL section the SERVICE toggle reveals: output trims plus every module amount.
export const GLOBAL_KNOBS = [
  ['mix', 'Dry/Wet'], ['brightness', 'Brightness'], ['contrast', 'Contrast'],
  ['colour_master', 'Colour Signal'], ['bend_master', 'Circuit Bend'], ['damage_master', 'Damage'],
  ['noise_master', 'Noise'], ['waves_master', 'Waves'], ['screen_master', 'Screen'],
  ['capture_master', 'Capture'], ['glass_master', 'Glass & Optics'], ['chroma_bleed', 'Chroma Bleed'],
];

export function createGlobalSection({ params, store }) {
  const byKey = Object.fromEntries(params.map((p) => [p.key, p]));
  const section = document.createElement('section');
  section.className = 'global';
  section.setAttribute('aria-labelledby', 'global-label');
  section.innerHTML = '<h2 class="section-label" id="global-label">Global</h2><div class="global-grid"></div><div class="global-extra"></div>';
  const grid = section.querySelector('.global-grid');
  GLOBAL_KNOBS.forEach(([key, label]) => grid.append(createKnobCell({ param: byKey[key], store, label })));
  const extra = section.querySelector('.global-extra');
  extra.append(createKnobCell({ param: byKey.clean_signal, store, label: 'Clean Signal' }));
  extra.append(bound(createControl, byKey.quality, store));
  return section;
}
