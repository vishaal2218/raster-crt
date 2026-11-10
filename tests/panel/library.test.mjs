import { test } from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { createLibrary, nameProblem, LIBRARY_FORMAT, MAX_PRESETS } from '../../panel/js/library.js';

// A tiny in-memory stand-in for Node's fs, enough for the library.
function memoryFs() {
  const files = new Map();
  return {
    files,
    existsSync: (p) => files.has(p),
    readFileSync: (p) => { if (!files.has(p)) throw new Error('ENOENT'); return files.get(p); },
    writeFileSync: (p, data) => files.set(p, String(data)),
    copyFileSync: (a, b) => files.set(b, files.get(a)),
    renameSync: (a, b) => { files.set(b, files.get(a)); files.delete(a); },
    mkdirSync: () => {},
  };
}

const DIR = '/lib/Raster CRT';
const FILE = path.join(DIR, 'presets.json');

test('names follow the preset rules', () => {
  assert.equal(nameProblem('Night drive'), null);
  assert.match(nameProblem('  '), /name/);
  assert.match(nameProblem('night/drive'), /can't contain/);
  assert.match(nameProblem('x'.repeat(41)), /40/);
});

test('an empty library loads as no looks', () => {
  assert.deepEqual(createLibrary({ fs: memoryFs(), path, dir: DIR }).load(), []);
});

test('saving writes the library format and keeps the previous file as a backup', () => {
  const fs = memoryFs();
  const lib = createLibrary({ fs, path, dir: DIR });
  lib.save('Night drive', { mix: 0.5 }, new Date('2026-10-15T07:00:00Z'));
  lib.save('Soft tape', { mix: 0.8 });
  const doc = JSON.parse(fs.files.get(FILE));
  assert.equal(doc.format, LIBRARY_FORMAT);
  assert.deepEqual(doc.presets.map((p) => p.name), ['Night drive', 'Soft tape']);
  assert.equal(doc.presets[0].created, '2026-10-15T07:00:00.000Z');
  assert.equal(JSON.parse(fs.files.get(`${FILE}.bak`)).presets.length, 1, 'backup is the file before the last save');
  assert.ok(!fs.files.has(`${FILE}.tmp`), 'no temporary file left behind');
});

test('saving an existing name replaces it, ignoring case', () => {
  const lib = createLibrary({ fs: memoryFs(), path, dir: DIR });
  lib.save('Night drive', { mix: 0.5 });
  const after = lib.save('NIGHT DRIVE', { mix: 0.1 });
  assert.equal(after.length, 1);
  assert.equal(after[0].values.mix, 0.1);
  assert.ok(lib.exists('night drive'));
});

test('bad names and a full library are refused with plain reasons', () => {
  const fs = memoryFs();
  const lib = createLibrary({ fs, path, dir: DIR });
  assert.throws(() => lib.save('a:b', {}), /can't contain/);
  fs.files.set(FILE, JSON.stringify({ format: LIBRARY_FORMAT, presets: Array.from({ length: MAX_PRESETS }, (_, i) => ({ name: `L${i}`, values: {} })) }));
  assert.throws(() => lib.save('One more', {}), /up to 500/);
});

test('a damaged or foreign file is reported and never overwritten by a load', () => {
  const fs = memoryFs();
  const lib = createLibrary({ fs, path, dir: DIR });
  fs.files.set(FILE, '{not json');
  assert.throws(() => lib.load(), /can't be read/);
  assert.equal(fs.files.get(FILE), '{not json');
  fs.files.set(FILE, JSON.stringify({ format: 'other/9', presets: [] }));
  assert.throws(() => lib.load(), /unknown format/);
});

test('deleting removes only that look', () => {
  const lib = createLibrary({ fs: memoryFs(), path, dir: DIR });
  lib.save('A', {});
  lib.save('B', {});
  assert.deepEqual(lib.remove('a').map((p) => p.name), ['B']);
});
