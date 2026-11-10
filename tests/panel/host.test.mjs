// Runs the ExtendScript host (ES3) in Node against a fake Premiere.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const SOURCE = fs.readFileSync(new URL('../../panel/jsx/host.jsx', import.meta.url), 'utf8');

function prop(displayName, value, { keyframed = false, header = false } = {}) {
  return {
    displayName,
    value,
    keys: [],
    getValue() { if (header) throw new Error('no value'); return this.value; },
    setValue(v) { this.value = v; },
    isTimeVarying() { return keyframed; },
    addKey(t) { this.keys.push({ t: t.seconds }); },
    setValueAtKey(t, v) { this.keys.find((k) => k.t === t.seconds).v = v; },
  };
}
const list = (items) => Object.assign([...items], { numItems: items.length });

function loadHost({ clips = [], hasSequence = true, playhead = 0 } = {}) {
  const seq = { name: 'Seq', getSelection: () => clips, getPlayerPosition: () => ({ seconds: playhead }) };
  const app = { version: '26.0', project: { activeSequence: hasSequence ? seq : null } };
  function Time() { this.seconds = 0; }
  const RC = new Function('app', 'Time', `${SOURCE}; return RC;`)(app, Time);
  const callJson = (name, args) => JSON.parse(RC[name](args ? JSON.stringify(args) : ''));
  return { RC, callJson };
}

function rasterClip(props) {
  return {
    name: 'Talking head', nodeId: 'n1', mediaType: 'Video', start: { seconds: 10 }, inPoint: { seconds: 2 },
    components: list([{ displayName: 'Motion', properties: list([]) }, { displayName: 'Raster CRT', properties: list(props) }]),
  };
}

test('selection reports the first selected video clip and whether it has Raster CRT', () => {
  const audio = { name: 'a', mediaType: 'Audio', components: list([]) };
  const { callJson } = loadHost({ clips: [audio, rasterClip([])] });
  assert.deepEqual(callJson('selection').data, { sequence: 'Seq', clip: 'Talking head', clipId: 'n1', count: 1, hasEffect: true });
});

test('read skips group headers and lists keyframed controls', () => {
  const { callJson } = loadHost({ clips: [rasterClip([
    prop('Global', null, { header: true }), prop('Dry/Wet', 1), prop('Quality', 2, { keyframed: true }),
  ])] });
  const { data } = callJson('read');
  assert.deepEqual(data.values, { 'Dry/Wet': 1, Quality: 2 });
  assert.deepEqual(data.keyframed, ['Quality']);
});

test('writing a keyframed control sets a keyframe at the playhead, in clip time', () => {
  const contrast = prop('Contrast', 1, { keyframed: true });
  const { callJson } = loadHost({ clips: [rasterClip([contrast])], playhead: 13.5 });
  assert.ok(callJson('write', { label: 'Contrast', value: 2 }).ok);
  assert.deepEqual(contrast.keys, [{ t: 5.5, v: 2 }], 'playhead 13.5 s - clip start 10 s + in point 2 s');
  assert.equal(contrast.value, 1, 'the static value is left alone');
});

test('writeMany writes what it can and reports keyframed skips', () => {
  const mix = prop('Dry/Wet', 1);
  const { callJson } = loadHost({ clips: [rasterClip([mix, prop('Contrast', 1, { keyframed: true })])] });
  const { data } = callJson('writeMany', { values: [{ label: 'Dry/Wet', value: 0.5 }, { label: 'Contrast', value: 2 }] });
  assert.deepEqual(data, { written: 1, skipped: ['Contrast'] });
  assert.equal(mix.value, 0.5);
});

test('errors come back as codes: no sequence, no clip, no effect, unknown control', () => {
  assert.equal(loadHost({ hasSequence: false }).callJson('read').error.code, 'NO_SEQUENCE');
  assert.equal(loadHost().callJson('read').error.code, 'NO_CLIP');
  const plain = { name: 'c', nodeId: 'x', mediaType: 'Video', components: list([]) };
  assert.equal(loadHost({ clips: [plain] }).callJson('read').error.code, 'NO_EFFECT');
  assert.equal(loadHost({ clips: [rasterClip([])] }).callJson('write', { label: 'Nope', value: 1 }).error.code, 'NO_CONTROL');
});

test('strings with quotes and control characters stay valid JSON', () => {
  const clip = rasterClip([]);
  clip.name = 'He said "hi"\n';
  assert.equal(loadHost({ clips: [clip] }).callJson('selection').data.clip, 'He said "hi"\n');
});
