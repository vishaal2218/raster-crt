// Looks as control values: a factory or personal look is a partial set of values; every
// control it leaves out sits at its default. Also tells whether the clip still matches.

const MATCH_TOLERANCE = 1e-3;

function clampTo(param, v) {
  if (param.type === 'checkbox') return Boolean(v);
  if (param.type === 'popup') return Math.max(0, Math.round(Number(v) || 0));
  const n = Number(v);
  if (!Number.isFinite(n)) return param.default;
  return Math.min(param.max, Math.max(param.min, n));
}

/** Every control's value for a look (the Preset control itself is left out). */
export function lookValues(values, params) {
  const out = {};
  for (const p of params) {
    if (p.key === 'preset') continue;
    out[p.key] = p.key in (values || {}) ? clampTo(p, values[p.key]) : p.default;
  }
  return out;
}

/** The store's current values for every control except Preset. */
export function currentValues(store, params) {
  return Object.fromEntries(params.filter((p) => p.key !== 'preset').map((p) => [p.key, store.get(p.key)]));
}

/** True when two complete value sets describe the same look. */
export function sameLook(a, b, params) {
  return params.every((p) => {
    if (p.key === 'preset') return true;
    const x = a[p.key];
    const y = b[p.key];
    if (typeof x === 'number' && typeof y === 'number') return Math.abs(x - y) <= MATCH_TOLERANCE;
    return x === y;
  });
}

/** What the LCD shows: the look's name, CUSTOM after edits, or CUSTOM alone. */
export function lcdLabel(look, current, params) {
  if (!look) return 'CUSTOM';
  const name = look.name.toUpperCase();
  return sameLook(lookValues(look.values, params), current, params) ? name : `CUSTOM · ${name}`;
}
