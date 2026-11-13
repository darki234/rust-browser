// Рисует assets/icon.png (512) и assets/icon.ico (16–256) из assets/icon.svg.
// Растеризует сам Chromium: нужна запущенная копия браузера с отладочным портом.
// node assets/render-icon.mjs [порт]
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const port = process.argv[2] || 9333;
const svg = readFileSync(join(here, 'icon.svg'), 'utf8');
const targets = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).filter((t) => t.type === 'page');
const target = targets.find((t) => t.title === 'Rust Browser') || targets[0];
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((r) => { ws.onopen = r; });
let id = 0;
const call = (method, params) => new Promise((resolve) => {
  const my = ++id;
  ws.addEventListener('message', function on(e) {
    const m = JSON.parse(e.data);
    if (m.id === my) { ws.removeEventListener('message', on); resolve(m.result); }
  });
  ws.send(JSON.stringify({ id: my, method, params }));
});

async function render(size) {
  const expression = `new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      const c = document.createElement('canvas');
      c.width = c.height = ${size};
      const g = c.getContext('2d');
      g.imageSmoothingQuality = 'high';
      g.drawImage(img, 0, 0, ${size}, ${size});
      resolve(c.toDataURL('image/png').split(',')[1]);
    };
    img.src = 'data:image/svg+xml;base64,' + ${JSON.stringify(Buffer.from(svg).toString('base64'))};
  })`;
  const r = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  return Buffer.from(r.result.value, 'base64');
}

writeFileSync(join(here, 'icon.png'), await render(512));
// ICO с PNG внутри (Windows Vista и новее): заголовок, каталог, картинки
const sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];
const images = [];
for (const size of sizes) images.push(await render(size));
const header = Buffer.alloc(6 + 16 * sizes.length);
header.writeUInt16LE(0, 0); header.writeUInt16LE(1, 2); header.writeUInt16LE(sizes.length, 4);
let offset = header.length;
sizes.forEach((size, i) => {
  const at = 6 + 16 * i;
  header.writeUInt8(size >= 256 ? 0 : size, at);
  header.writeUInt8(size >= 256 ? 0 : size, at + 1);
  header.writeUInt16LE(1, at + 4); header.writeUInt16LE(32, at + 6);
  header.writeUInt32LE(images[i].length, at + 8); header.writeUInt32LE(offset, at + 12);
  offset += images[i].length;
});
writeFileSync(join(here, 'icon.ico'), Buffer.concat([header, ...images]));
console.log('icon.png и icon.ico готовы');
ws.close();
