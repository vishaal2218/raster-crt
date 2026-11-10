// Raster CRT panel: builds the rack from params.json, follows the selected clip, keeps the
// panel and Effect Controls in step (sync.js), and runs the presets: factory looks, the
// user's own library (library.js), the browser (browser.js) and the CUSTOM marker.
import { createStore, createModuleRow, createGlobalSection } from './primitives.js';
import { createBridge } from './bridge.js';
import { createSync, HOST } from './sync.js';
import { renderIcons } from './icons.js';
import { createLibrary, nameProblem } from './library.js';
import { createBrowser } from './browser.js';
import { lookValues, currentValues, lcdLabel } from './looks.js';

const cep = window.__adobe_cep__;
const fs = window.require('fs');
const path = window.require('path');
const os = window.require('os');
const root = decodeURI(cep.getSystemPath('extension')).replace(/^file:\/\//, '');
const readJson = (rel) => JSON.parse(fs.readFileSync(path.join(root, rel), 'utf8'));
const $ = (id) => document.getElementById(id);
const LIBRARY_DIR = path.join(os.homedir(), 'Library', 'Application Support', 'Raster CRT');
const DELETE_CONFIRM_MS = 3000;

const LCD_TEXT = {
  noSequence: 'NO SEQUENCE OPEN',
  noClip: 'NO CLIP SELECTED',
  noEffect: 'ADD RASTER CRT TO THIS CLIP',
};

const ERROR_TEXT = {
  KEYFRAMED: (d) => `'${d}' has keyframes. Change it in Effect Controls.`,
  NO_CONTROL: (d) => `This clip's Raster CRT has no '${d}' control. Reinstall the effect.`,
  NOT_INSTALLED: () => "Raster CRT isn't installed in Premiere.",
  NO_QE_CLIP: () => "Couldn't find that clip on the timeline. Select it again.",
  BRIDGE: () => "Premiere isn't responding · retrying",
};

function setStatus(text, tone = '') {
  const el = $('status');
  el.hidden = !text;
  el.textContent = text || '';
  el.className = `status${tone ? ` is-${tone}` : ''}`;
  el.setAttribute('role', tone === 'danger' ? 'alert' : 'status');
}

/** Builds the SIGNAL and SCREEN rows and the GLOBAL grid; returns rows by module key. */
function buildRack(modules, params, store) {
  const rows = new Map();
  for (const section of ['signal', 'screen']) {
    const wrap = document.createElement('section');
    wrap.className = 'rack-section';
    const heading = document.createElement('h2');
    heading.className = 'section-label';
    heading.textContent = section;
    wrap.append(heading);
    modules.filter((m) => m.section === section).forEach((m) => {
      const row = createModuleRow({ module: m, params: params.filter((p) => p.module === m.key), store });
      rows.set(m.key, row);
      wrap.append(row);
    });
    $('rack').append(wrap);
  }
  $('global-slot').append(createGlobalSection({ params, store }));
  return rows;
}

/** "Name", or "Name 2", "Name 3"... until the library doesn't have it. */
function freeName(name, library) {
  if (!library.exists(name)) return name;
  for (let n = 2; ; n += 1) {
    const candidate = `${name.slice(0, 36)} ${n}`;
    if (!library.exists(candidate)) return candidate;
  }
}

function main() {
  const { modules, params } = readJson('params.json');
  const factory = readJson('presets/factory.json').presets;
  const store = createStore(Object.fromEntries(params.map((p) => [p.key, p.default])));
  const bridge = createBridge(cep);
  const library = createLibrary({ fs, path, dir: LIBRARY_DIR });
  const rows = buildRack(modules, params, store);
  let selection = { hasEffect: false };
  let active = null; // { kind: 'factory' | 'personal', name, values, index? }
  let personal = [];
  let pendingDelete = null;
  let sync = null; // created below; store callbacks only run after start

  const onError = (err) => {
    if (!err) {
      setStatus(selection.count > 1 ? 'Several clips selected · editing the first' : '');
      return;
    }
    const text = (ERROR_TEXT[err.code] || (() => err.message || 'Premiere reported a problem · retrying'))(err.detail);
    setStatus(text, err.code === 'KEYFRAMED' ? 'warning' : 'danger');
    if (err.code === 'BRIDGE') $('led').className = 'led is-error';
  };

  try {
    personal = library.load();
  } catch (err) {
    setStatus(err.message, 'warning');
  }

  const factoryLook = (index) => ({ kind: 'factory', name: factory[index].name, values: factory[index].values, index });

  const showLcd = () => {
    const text = !selection.sequence ? LCD_TEXT.noSequence
      : !selection.clip ? LCD_TEXT.noClip
      : !selection.hasEffect ? LCD_TEXT.noEffect
      : lcdLabel(active, currentValues(store, params), params);
    $('lcd-text').textContent = text;
    $('lcd').title = text; // the full name when it doesn't fit
    // the look's thumbnail beside its name (factory looks only; a changed one keeps its base)
    const thumb = selection.hasEffect && active?.kind === 'factory' ? factory[active.index]?.thumbnail : null;
    $('lcd').querySelector('.lcd-thumb').style.backgroundImage = thumb ? `url("presets/thumbnails/${thumb}")` : '';
  };

  // The Preset popup can also change in Effect Controls. A change read from Premiere (not
  // the first read of a newly selected clip) applies that whole look, so the popup works
  // like the browser while the panel is open.
  let seenPreset = { clipId: null, value: null };
  store.subscribe((key, value, origin, source) => {
    if (key === 'preset' && origin === HOST) {
      const changedByUser = source === 'host' && seenPreset.clipId === selection.clipId && seenPreset.value !== null && seenPreset.value !== value;
      seenPreset = { clipId: selection.clipId, value };
      if (value > 0 && factory[value - 1]) {
        active = factoryLook(value - 1);
        if (changedByUser) sync.applyAll(lookValues(active.values, params), value).catch(onError);
      } else if (active?.kind === 'factory') {
        active = null;
      }
    }
    showLcd();
  });

  // ---------- browser ----------
  const browserOpen = () => !$('browser-slot').hidden;
  const setBrowser = (open) => {
    $('browser-slot').hidden = !open;
    $('body').hidden = open;
    $('service').hidden = open;
    $('lcd').setAttribute('aria-expanded', String(open));
    if (open) browser.render(personal, active);
  };
  const applyLook = (look, presetIndex) => {
    active = look;
    sync.applyAll(lookValues(look.values, params), presetIndex).catch(onError);
    setBrowser(false);
    showLcd();
  };
  const browser = createBrowser({
    factory,
    onPickFactory: (_look, index) => applyLook(factoryLook(index), index + 1),
    onPickPersonal: (look) => applyLook({ kind: 'personal', name: look.name, values: look.values }, 0),
    onDeletePersonal: (look) => {
      if (pendingDelete !== look.name) {
        pendingDelete = look.name;
        setStatus(`Click delete again to remove '${look.name}'.`, 'warning');
        setTimeout(() => { if (pendingDelete === look.name) pendingDelete = null; }, DELETE_CONFIRM_MS);
        return;
      }
      pendingDelete = null;
      try {
        personal = library.remove(look.name);
        if (active?.kind === 'personal' && active.name === look.name) active = null;
        setStatus(`Deleted '${look.name}'`, 'success');
        browser.render(personal, active);
        showLcd();
      } catch (err) {
        setStatus(err.message, 'danger');
      }
    },
  });
  $('browser-slot').append(browser.el);

  // ---------- save sheet ----------
  const sheet = {
    open() {
      $('save-sheet').hidden = false;
      $('save-replace').hidden = true;
      $('save-name').value = active?.kind === 'personal' ? active.name : active ? `${active.name} copy` : 'My look';
      this.check();
      $('save-name').focus();
      $('save-name').select();
    },
    close() {
      $('save-sheet').hidden = true;
      $('save').focus();
    },
    check() {
      const problem = nameProblem($('save-name').value);
      $('save-error').hidden = !problem;
      $('save-error').textContent = problem || '';
      $('save-name').setAttribute('aria-invalid', String(Boolean(problem)));
      $('save-confirm').disabled = Boolean(problem);
      return !problem;
    },
    store(name) {
      try {
        const values = currentValues(store, params);
        personal = library.save(name, values);
        active = { kind: 'personal', name: name.trim(), values };
        setStatus(`Saved '${name.trim()}'`, 'success');
        this.close();
        showLcd();
      } catch (err) {
        setStatus(err.message, 'danger');
      }
    },
    confirm() {
      if (!this.check()) return;
      const name = $('save-name').value.trim();
      if (library.exists(name) && $('save-replace').hidden) {
        $('save-replace-text').textContent = `Replace '${name}'? It already exists.`;
        $('save-replace').hidden = false;
        return;
      }
      this.store(name);
    },
  };
  $('save').addEventListener('click', () => sheet.open());
  $('save-name').addEventListener('input', () => { $('save-replace').hidden = true; sheet.check(); });
  $('save-name').addEventListener('keydown', (e) => { if (e.key === 'Enter') sheet.confirm(); });
  $('save-confirm').addEventListener('click', () => sheet.confirm());
  $('save-cancel').addEventListener('click', () => sheet.close());
  $('save-overwrite').addEventListener('click', () => sheet.store($('save-name').value));
  $('save-both').addEventListener('click', () => sheet.store(freeName($('save-name').value.trim(), library)));

  // ---------- selection, keyframes, sync ----------
  const onSelection = (sel) => {
    selection = sel;
    $('led').className = 'led is-on';
    $('led').setAttribute('aria-label', 'Connected to Premiere');
    $('clip-name').textContent = sel.clip || sel.sequence || 'No sequence';
    $('body').classList.toggle('is-disabled', !sel.hasEffect);
    ['prev', 'next', 'reset', 'save'].forEach((id) => { $(id).disabled = !sel.hasEffect; });
    if (!sel.hasEffect && browserOpen()) setBrowser(false);
    showLcd();
  };
  const onRead = (read) => {
    const keyframed = new Set(read.keyframed);
    rows.forEach((row, key) => {
      row.classList.toggle('has-keyframes', params.some((p) => p.module === key && keyframed.has(p.label)));
    });
  };
  sync = createSync({ bridge, params, store, onSelection, onError, onRead });

  // ---------- preset bar ----------
  const stepLook = (delta) => {
    const count = factory.length;
    const current = active?.kind === 'factory' ? active.index : delta > 0 ? -1 : 0;
    const next = (current + delta + count) % count;
    applyLook(factoryLook(next), next + 1);
  };
  $('prev').addEventListener('click', () => stepLook(-1));
  $('next').addEventListener('click', () => stepLook(1));
  $('reset').addEventListener('click', () => {
    active = null;
    sync.resetAll().catch(onError);
    showLcd();
  });
  $('lcd').addEventListener('click', () => {
    if (selection.clip && !selection.hasEffect) {
      bridge.call('addEffect').then(sync.pollOnce, onError);
      return;
    }
    if (selection.hasEffect) setBrowser(!browserOpen());
  });
  $('service').addEventListener('click', () => {
    const open = $('service').getAttribute('aria-expanded') !== 'true';
    $('service').setAttribute('aria-expanded', String(open));
    $('global-slot').hidden = !open;
  });
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;
    if (!$('save-sheet').hidden) sheet.close();
    else if (browserOpen()) setBrowser(false);
    else if ($('service').getAttribute('aria-expanded') === 'true') $('service').click();
  });

  renderIcons();
  bridge.ensureHostScript(path.join(root, 'jsx', 'host.jsx')).then(sync.start, (err) => {
    onError(err);
    sync.start();
  });
  // Exposed for live tests over the panel's debug port (8793).
  window.rasterCrt = { store, sync, bridge, HOST, library, factory, applyLook, factoryLook, sheet };
}

try {
  main();
} catch (err) {
  setStatus(`Raster CRT couldn't start: ${err.message}`, 'danger');
}
