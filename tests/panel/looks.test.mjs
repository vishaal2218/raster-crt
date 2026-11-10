import { test } from 'node:test';
import assert from 'node:assert/strict';
import { lookValues, currentValues, sameLook, lcdLabel } from '../../panel/js/looks.js';
import { createStore } from '../../panel/js/primitives.js';

const PARAMS = [
  { key: 'preset', type: 'popup', default: 0 },
  { key: 'mix', type: 'float', min: 0, max: 1, default: 1 },
  { key: 'mask_type', type: 'popup', default: 0, options: ['a', 'b', 'c'] },
  { key: 'wave_rgb', type: 'checkbox', default: false },
];

test('a look fills unlisted controls with defaults and clamps the rest', () => {
  assert.deepEqual(lookValues({ mix: 3, wave_rgb: 1 }, PARAMS), { mix: 1, mask_type: 0, wave_rgb: true });
});

test('the Preset control is never part of a look', () => {
  const store = createStore({ preset: 5, mix: 0.5, mask_type: 2, wave_rgb: false });
  assert.deepEqual(currentValues(store, PARAMS), { mix: 0.5, mask_type: 2, wave_rgb: false });
});

test('tiny float differences still count as the same look', () => {
  const a = { mix: 0.5, mask_type: 1, wave_rgb: true };
  assert.ok(sameLook(a, { ...a, mix: 0.5004 }, PARAMS));
  assert.ok(!sameLook(a, { ...a, mix: 0.6 }, PARAMS));
  assert.ok(!sameLook(a, { ...a, wave_rgb: false }, PARAMS));
});

test('the LCD shows the name, CUSTOM after edits, or CUSTOM alone', () => {
  const look = { name: 'Lost Sync', values: { mix: 0.5 } };
  const matching = { mix: 0.5, mask_type: 0, wave_rgb: false };
  assert.equal(lcdLabel(look, matching, PARAMS), 'LOST SYNC');
  assert.equal(lcdLabel(look, { ...matching, mix: 0.9 }, PARAMS), 'CUSTOM · LOST SYNC');
  assert.equal(lcdLabel(null, matching, PARAMS), 'CUSTOM');
});
