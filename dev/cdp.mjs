// Evaluates JavaScript inside the Raster CRT panel through its own debug port (panel/.debug).
//   node dev/cdp.mjs <file-with-js-expression> [port]
// The expression may return a promise; the result is printed as JSON.
import fs from 'node:fs';

const DEFAULT_PORT = 8793;
const TIMEOUT_MS = 60000;
const [file, port = DEFAULT_PORT] = process.argv.slice(2);
if (!file) {
  console.error('usage: node dev/cdp.mjs <file-with-js-expression> [port]');
  process.exit(2);
}
const expression = fs.readFileSync(file, 'utf8');
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const target = targets.find((t) => t.type === 'page') ?? targets[0];
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve) => ws.addEventListener('open', resolve));
ws.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } }));
ws.addEventListener('message', (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id !== 1) return;
  const r = msg.result;
  if (r.exceptionDetails) console.log('EXCEPTION', JSON.stringify(r.exceptionDetails).slice(0, 2000));
  else console.log(typeof r.result.value === 'string' ? r.result.value : JSON.stringify(r.result.value, null, 1));
  ws.close();
  process.exit(0);
});
setTimeout(() => { console.log('timeout'); process.exit(1); }, TIMEOUT_MS);
