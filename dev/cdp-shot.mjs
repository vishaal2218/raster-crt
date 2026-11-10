// Saves a PNG of the Raster CRT panel as it renders inside Premiere (via its debug port).
//   node dev/cdp-shot.mjs <out.png> [port]
import fs from 'node:fs';

const DEFAULT_PORT = 8793;
const [out, port = DEFAULT_PORT] = process.argv.slice(2);
if (!out) {
  console.error('usage: node dev/cdp-shot.mjs <out.png> [port]');
  process.exit(2);
}
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const target = targets.find((t) => t.type === 'page') ?? targets[0];
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve) => ws.addEventListener('open', resolve));
ws.send(JSON.stringify({ id: 1, method: 'Page.captureScreenshot', params: { format: 'png', captureBeyondViewport: true } }));
ws.addEventListener('message', (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id !== 1) return;
  if (msg.error) {
    console.error('capture failed:', msg.error.message);
    process.exit(1);
  }
  fs.writeFileSync(out, Buffer.from(msg.result.data, 'base64'));
  console.log(`saved ${out}`);
  ws.close();
  process.exit(0);
});
setTimeout(() => { console.error('timeout'); process.exit(1); }, 30000);
