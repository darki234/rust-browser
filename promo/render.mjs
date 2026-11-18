// node render.mjs <html> <out> [fps] [times-comma-separated]
import { spawn } from 'node:child_process';
import { writeFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os'; import { join } from 'node:path';
const [, , HTML, OUT, FPS = '30', TIMES, FFMPEG] = process.argv;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const port = 9361;
const prof = mkdtempSync(join(tmpdir(), 'edge-render-'));
const edge = spawn('C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${prof}`, '--allow-file-access-from-files', '--hide-scrollbars', '--force-device-scale-factor=1', '--window-size=1920,1080', 'about:blank'], { stdio: 'ignore' });
let t; for (let i = 0; i < 60 && !t; i++) { await sleep(250); try { t = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((x) => x.type === 'page'); } catch {} }
const ws = new WebSocket(t.webSocketDebuggerUrl); await new Promise((r) => (ws.onopen = r));
let id = 0; const pend = new Map(); ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id) { pend.get(m.id)?.(m); pend.delete(m.id); } };
const call = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expression) => { const m = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }); if (m.result?.exceptionDetails) throw new Error(JSON.stringify(m.result.exceptionDetails)); return m.result?.result?.value; };
await call('Emulation.setDeviceMetricsOverride', { width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false });
await call('Page.enable');
await call('Page.navigate', { url: 'file:///' + HTML.split(String.fromCharCode(92)).join('/') });
await sleep(1500); await ev('window.ready'); await sleep(500);
const withTimeout = (p, ms) => Promise.race([p, new Promise((_, j) => setTimeout(() => j(new Error('timeout')), ms))]);
const snap1 = async (tt, fmt) => { await ev(`render(${tt}); 1`); const m = await call('Page.captureScreenshot', { format: fmt, quality: fmt === 'jpeg' ? 95 : undefined, captureBeyondViewport: false }); return Buffer.from(m.result.data, 'base64'); };
const snap = async (tt, fmt) => { for (let a = 0; ; a++) { try { return await withTimeout(snap1(tt, fmt), 15000); } catch (e) { console.log('retry', tt, e.message); if (a > 4) throw e; } } };
if (TIMES) {
  for (const tt of TIMES.split(',')) writeFileSync(join(OUT, `f_${tt}.png`), await snap(Number(tt), 'png'));
} else {
  const fps = Number(FPS), dur = await ev('DUR'), n = Math.round(dur * fps);
  const ff = spawn(FFMPEG, ['-y', '-f', 'image2pipe', '-framerate', String(fps), '-c:v', 'mjpeg', '-i', '-', '-c:v', 'libx264', '-preset', 'slow', '-crf', '17', '-vf', 'scale=in_range=full:out_range=tv:in_color_matrix=bt601:out_color_matrix=bt709,format=yuv420p', '-color_range', 'tv', '-colorspace', 'bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', OUT], { stdio: ['pipe', 'ignore', 'inherit'] });
  const t0 = Date.now();
  for (let f = 0; f < n; f++) {
    const buf = await snap(f / fps, 'jpeg');
    if (!ff.stdin.write(buf)) await new Promise((r) => ff.stdin.once('drain', r));
    if (f % 150 === 0) console.log(`кадр ${f}/${n}, ${((Date.now() - t0) / 1000).toFixed(0)} с`);
  }
  ff.stdin.end(); await new Promise((r) => ff.on('close', r));
}
ws.close(); edge.kill(); process.exit(0);
