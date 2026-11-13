// Сквозные проверки Rust Browser через отладочный порт WebView2.
// Запускается из run-e2e.ps1, который поднимает тестовую копию браузера.
// node e2e.mjs <порт> <file-URL тестовой страницы> <путь к exe>
import { spawn } from 'node:child_process';

const [, , PORT, FIXTURE, EXE] = process.argv;
const results = [];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function targets() {
  return (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).filter((t) => t.type === 'page');
}

/** Соединение с целью: интерфейс браузера или страница вкладки. */
async function connect(pick) {
  let target;
  for (let i = 0; i < 50 && !target; i++) {
    target = (await targets().catch(() => [])).find(pick);
    if (!target) await sleep(200);
  }
  if (!target) throw new Error('цель не найдена');
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  let id = 0;
  const pending = new Map();
  const listeners = [];
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result ?? m.error); pending.delete(m.id); }
    else listeners.forEach((fn) => fn(m));
  };
  const call = (method, params = {}) => new Promise((r) => {
    const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params }));
  });
  const evaluate = async (expression) => {
    const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || 'ошибка в скрипте');
    return r.result?.value;
  };
  return { ws, call, evaluate, on: (fn) => listeners.push(fn), close: () => ws.close() };
}

const isUi = (t) => t.url.startsWith('about:') && t.title === 'Rust Browser';

async function waitFor(fn, timeout = 8000, step = 150) {
  const start = Date.now();
  let last;
  while (Date.now() - start < timeout) {
    try { last = await fn(); if (last) return last; } catch (e) { last = e.message; }
    await sleep(step);
  }
  throw new Error(`не дождались (последнее значение: ${JSON.stringify(last)})`);
}

async function check(name, fn) {
  const start = Date.now();
  try {
    await fn();
    results.push({ name, ok: true, ms: Date.now() - start });
    console.log(`  ✓ ${name} (${Date.now() - start} мс)`);
  } catch (error) {
    results.push({ name, ok: false, error: error.message });
    console.log(`  ✗ ${name}: ${error.message}`);
  }
}

const ui = await connect(isUi);
const tabsCount = () => ui.evaluate('S.tabs.length');

await check('интерфейс загрузился', async () => {
  await waitFor(() => ui.evaluate('S.tabs.length >= 1 && lastChrome > 0'));
});

await check('открывается страница во вкладке', async () => {
  await ui.evaluate(`send('navigate', { text: ${JSON.stringify(FIXTURE)} })`);
  await waitFor(async () => (await targets()).some((t) => t.title === 'Тестовая страница'));
  await waitFor(() => ui.evaluate('activeTab().url.startsWith("file:") && !activeTab().loading'));
});

await check('новая вкладка открывается и закрывается', async () => {
  const before = await tabsCount();
  await ui.evaluate("send('new_tab')");
  await waitFor(async () => (await tabsCount()) === before + 1 && (await ui.evaluate('activeTab().start')));
  await ui.evaluate("send('close_tab', { id: S.active })");
  await waitFor(async () => (await tabsCount()) === before);
});

await check('меню «⋯» открывается поверх страницы', async () => {
  await ui.evaluate('openMainMenu()');
  await waitFor(() => ui.evaluate("menuKind === 'main' && el.layer.classList.contains('open') && innerHeight > 200"));
  await ui.evaluate('closeMenu()');
  await waitFor(() => ui.evaluate('innerHeight < 200'), 3000);
});

await check('параметры открываются', async () => {
  await ui.evaluate("openHub('settings')");
  await waitFor(() => ui.evaluate("el.hub.offsetHeight > 200 && document.querySelectorAll('.card').length >= 5"));
  await ui.evaluate('closeHub()');
  await waitFor(() => ui.evaluate('innerHeight < 200'), 3000);
});

await check('поиск по странице находит текст', async () => {
  await ui.evaluate("openFind(); el.findq.value = 'браузер'; el.findq.dispatchEvent(new Event('input'))");
  await waitFor(() => ui.evaluate("/\\d/.test(el.findcount.textContent) && !el.findcount.classList.contains('none')"));
  await ui.evaluate('closeFind()');
});

await check('масштаб меняется и показывается', async () => {
  await ui.evaluate("send('zoom', { dir: 'in' })");
  await waitFor(() => ui.evaluate('activeTab().zoom > 1 && el.zoombadge.classList.contains("show")'));
  await ui.evaluate("send('zoom', { dir: 'reset' })");
  await waitFor(() => ui.evaluate('activeTab().zoom === 1'));
});

await check('у адресной строки есть своё меню', async () => {
  const [x, y] = await ui.evaluate('(() => { const b = el.url.getBoundingClientRect(); return [b.left + 60, b.top + b.height / 2]; })()');
  for (const type of ['mousePressed', 'mouseReleased']) {
    await ui.call('Input.dispatchMouseEvent', { type, x, y, button: 'right', buttons: type === 'mousePressed' ? 2 : 0, clickCount: 1 });
  }
  await waitFor(() => ui.evaluate("menuKind === 'input' && el.menu.querySelectorAll('.mi').length >= 9"));
  await ui.evaluate('closeMenu()');
});

await check('подсказки не перезапускают анимацию при наборе', async () => {
  await ui.evaluate('el.url.focus(); el.url.select()');
  for (const ch of 'тест') {
    await ui.call('Input.insertText', { text: ch });
    await sleep(120);
  }
  await waitFor(() => ui.evaluate("el.sheet.classList.contains('open')"));
  const rows = await ui.evaluate("[...el.suggestions.children].map((n) => n.className)");
  if (rows.some((c) => c === 'sug')) throw new Error(`строки снова анимируются: ${rows}`);
  await ui.evaluate("el.url.blur(); send('focus_content')");
});

await check('обои меняются', async () => {
  const before = await tabsCount();
  await ui.evaluate("send('set', { key: 'wallpaper', value: 'ocean' }); send('new_tab')");
  await waitFor(() => ui.evaluate("document.body.dataset.wall === 'ocean'"));
  await ui.evaluate("send('set', { key: 'wallpaper', value: 'aurora' }); send('close_tab', { id: S.active })");
  // закрытие асинхронное: ждём, иначе следующая проверка посчитает вкладки неверно
  await waitFor(async () => (await tabsCount()) === before);
});

await check('цвет браузера и вертикальные вкладки', async () => {
  await ui.evaluate("send('set', { key: 'accent', value: '#e5484d' }); send('set', { key: 'bar_style', value: 'vertical' })");
  await waitFor(() => ui.evaluate("document.body.classList.contains('bar-vertical') && lastChromeKey.startsWith('0|')"));
  const accent = await ui.evaluate("getComputedStyle(document.documentElement).getPropertyValue('--accent')");
  if (!accent.includes('hsl')) throw new Error(`цвет не применился: ${accent}`);
  await ui.evaluate("send('set', { key: 'accent', value: '' }); send('set', { key: 'bar_style', value: 'classic' })");
  await waitFor(() => ui.evaluate("!document.body.classList.contains('bar-vertical') && lastChromeKey.endsWith('|0')"));
});

await check('живые обои рисуются', async () => {
  const before = await tabsCount();
  await ui.evaluate("send('set', { key: 'wallpaper', value: 'particles' }); send('new_tab')");
  await waitFor(() => ui.evaluate("live.kind === 'particles' && live.items.length > 0"));
  await ui.evaluate("send('set', { key: 'wallpaper', value: 'aurora' }); send('close_tab', { id: S.active })");
  await waitFor(async () => (await tabsCount()) === before);
});

await check('боковая панель открывается и закрывается', async () => {
  await ui.evaluate("send('toggle_panel')");
  await waitFor(async () => (await ui.evaluate('S.panelOpen')) && (await targets()).some((t) => t.title === 'Панель'));
  await ui.evaluate("send('toggle_panel')");
  await waitFor(async () => !(await ui.evaluate('S.panelOpen')));
});

await check('uBlock Origin Lite установлен и блокирует рекламу', async () => {
  await waitFor(() => ui.evaluate('!!S.ubolId'), 30000, 500);
  const page = await connect((t) => t.title === 'Тестовая страница');
  const failed = new Map();
  const urls = new Map();
  page.on((m) => {
    if (m.method === 'Network.requestWillBeSent') urls.set(m.params.requestId, m.params.request.url);
    if (m.method === 'Network.loadingFailed') failed.set(urls.get(m.params.requestId), m.params.errorText);
  });
  await page.call('Network.enable');
  // домен из фильтров uBlock, которого нет в собственном списке браузера
  await page.evaluate("fetch('https://0019x.com/x.js', { mode: 'no-cors' }).catch(() => 0)");
  await waitFor(() => [...failed.keys()].some((u) => u?.includes('0019x.com')), 8000);
  const error = [...failed].find(([u]) => u?.includes('0019x.com'))[1];
  page.close();
  if (!error.includes('BLOCKED_BY_CLIENT')) throw new Error(`запрос не заблокирован: ${error}`);
});

await check('повторный запуск передаёт адрес открытому окну', async () => {
  const before = await tabsCount();
  const started = Date.now();
  const child = spawn(EXE, [FIXTURE + '#second'], { env: process.env, stdio: 'ignore' });
  const code = await new Promise((r) => child.on('exit', r));
  if (code !== 0) throw new Error(`второй запуск завершился с кодом ${code}`);
  if (Date.now() - started > 3000) throw new Error('второй запуск закрывался слишком долго');
  await waitFor(async () => (await tabsCount()) === before + 1);
});

ui.close();
const failedCount = results.filter((r) => !r.ok).length;
console.log(`\nИтого: ${results.length - failedCount} из ${results.length} проверок пройдено`);
process.exit(failedCount ? 1 : 0);
