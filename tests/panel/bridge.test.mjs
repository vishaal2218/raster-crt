import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createBridge, parseEnvelope, scriptLiteral, BridgeError } from '../../panel/js/bridge.js';

test('parseEnvelope returns data from an ok envelope', () => {
  assert.deepEqual(parseEnvelope('{"ok":true,"data":{"a":1}}'), { a: 1 });
});

test('parseEnvelope turns a host error into a BridgeError with its code', () => {
  assert.throws(() => parseEnvelope('{"ok":false,"error":{"code":"KEYFRAMED","detail":"Dry/Wet"}}'),
    (e) => e instanceof BridgeError && e.code === 'KEYFRAMED' && e.detail === 'Dry/Wet');
});

test('parseEnvelope rejects EvalScript failures and non-JSON as BRIDGE errors', () => {
  assert.throws(() => parseEnvelope('EvalScript error.'), (e) => e.code === 'BRIDGE');
  assert.throws(() => parseEnvelope('not json'), (e) => e.code === 'BRIDGE');
});

test('scriptLiteral survives line separators that end an ES3 line', () => {
  const lit = scriptLiteral({ label: 'a b' });
  assert.ok(!lit.includes(' '));
  assert.deepEqual(JSON.parse(eval(lit)), { label: 'a b' });
});

test('call sends RC.<name>(json) and refuses unknown entry points', async () => {
  const sent = [];
  const cep = { evalScript: (s, cb) => { sent.push(s); cb('{"ok":true,"data":7}'); } };
  const bridge = createBridge(cep);
  assert.equal(await bridge.call('write', { label: 'Contrast', value: 1.2 }), 7);
  assert.match(sent[0], /^RC\.write\(".*Contrast.*"\)$/);
  await assert.rejects(bridge.call('deleteEverything'), (e) => e.code === 'BRIDGE');
});
