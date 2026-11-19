// Сквозные проверки Rust Browser через отладочный порт WebView2.
// Запускается из run-e2e.ps1, который поднимает тестовую копию браузера.
// node e2e.mjs <порт> <file-URL тестовой страницы> <путь к exe>
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

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

await check('выделенный текст — появляется кнопка «Перевести»', async () => {
  const page = await connect((t) => t.url === FIXTURE);
  try {
    await page.evaluate("(() => { const p = document.querySelector('main p') || document.querySelector('p'); const r = document.createRange(); r.selectNodeContents(p); getSelection().removeAllRanges(); getSelection().addRange(r); document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true })); })()");
    await waitFor(() => page.evaluate("!!document.querySelector('rb-translate')"));
    await page.evaluate("getSelection().removeAllRanges(); document.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))");
  } finally {
    page.close();
  }
});

await check('заставка запуска отыграла и освободила окно', async () => {
  await waitFor(() => ui.evaluate("!document.getElementById('splash').className && window.__rbShape < 300"));
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
  await waitFor(() => ui.evaluate("menuKind === 'main' && el.layer.classList.contains('open') && window.__rbShape > 200"));
  await ui.evaluate('closeMenu()');
  await waitFor(() => ui.evaluate('window.__rbShape < 200'), 3000);
});

await check('параметры открываются', async () => {
  await ui.evaluate("openHub('settings')");
  await waitFor(() => ui.evaluate("el.hub.offsetHeight > 200 && document.querySelectorAll('.card').length >= 1 && document.querySelectorAll('.setcat.on').length === 1"));
  await ui.evaluate('closeHub()');
  await waitFor(() => ui.evaluate('window.__rbShape < 200'), 3000);
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
  await waitFor(() => ui.evaluate("menuKind === 'input' && el.menu.querySelectorAll('.mi').length >= 7"));
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

await check('параметры по разделам и поиск по ним', async () => {
  await ui.evaluate("openHub('settings')");
  await waitFor(() => ui.evaluate("document.querySelectorAll('.setcat').length >= 10"));
  await ui.evaluate("openSettingsCat('privacy')");
  await waitFor(() => ui.evaluate("[...document.querySelectorAll('.card h2')].some((h) => h.textContent === 'Пароли')"));
  await ui.evaluate("el.hubq.value = 'прозрачн'; el.hubq.dispatchEvent(new Event('input'))");
  await waitFor(() => ui.evaluate("document.querySelectorAll('.card .set').length === 1 && el.hublist.textContent.includes('Прозрачность панелей')"));
  await ui.evaluate("el.hubq.value = ''; closeHub()");
  await waitFor(() => ui.evaluate('window.__rbShape < 300'), 3000);
});

await check('поиск по параметрам находит сочетания клавиш', async () => {
  await ui.evaluate("openHub('settings')");
  await waitFor(() => ui.evaluate("document.querySelectorAll('.setcat').length >= 10"));
  await ui.evaluate("el.hubq.value = 'ctrl+f'; el.hubq.dispatchEvent(new Event('input'))");
  // в редакторе клавиш остаётся только строка с этим сочетанием
  await waitFor(() => ui.evaluate("[...document.querySelectorAll('.keyrow kbd')].map((k) => k.textContent).join() === 'Ctrl+F'"));
  await ui.evaluate("el.hubq.value = ''; closeHub()");
  await waitFor(() => ui.evaluate('window.__rbShape < 300'), 3000);
});

await check('меню целиком внутри окна', async () => {
  await ui.evaluate('openMainMenu()');
  await waitFor(() => ui.evaluate("menuKind === 'main' && el.layer.classList.contains('open')"));
  await sleep(400); // ждём конца анимации появления
  const box = await ui.evaluate('(() => { const r = el.menu.getBoundingClientRect(); return [r.right, r.bottom, innerWidth, innerHeight]; })()');
  if (box[0] > box[2] || box[1] > box[3]) throw new Error(`меню вылезло за окно: ${box}`);
  await ui.evaluate('closeMenu()');
  await waitFor(() => ui.evaluate('window.__rbShape < 200'), 3000);
});

await check('дата на новой вкладке: месяц со строчной', async () => {
  const date = await ui.evaluate("getComputedStyle(el.date).textTransform + ' ' + el.date.textContent");
  if (!/^none [А-ЯЁ][а-яё]+, \d+ [а-яё]+$/.test(date)) throw new Error(`дата: ${date}`);
});

await check('заставки в параметрах не обрывают друг друга', async () => {
  await ui.evaluate("openHub('settings')");
  await waitFor(() => ui.evaluate("el.hub.offsetHeight > 200"));
  await ui.evaluate("playSplash('compass', true)");
  await sleep(600);
  await ui.evaluate("playSplash('aurora', true)");
  await sleep(900); // первая уже закончилась бы, вторая ещё должна играть
  const cls = await ui.evaluate("document.getElementById('splash').className");
  if (cls !== 'show s-aurora') throw new Error(`вторая заставка оборвалась: ${cls}`);
  await waitFor(() => ui.evaluate("!document.getElementById('splash').className"), 3000);
  await ui.evaluate('closeHub()');
  await waitFor(() => ui.evaluate('window.__rbShape < 300'), 3000);
});

await check('остров и свои кнопки на панели', async () => {
  await ui.evaluate("send('set', { key: 'bar_style', value: 'island' }); send('set', { key: 'buttons', value: [{ name: 'Тест', url: 'https://example.org/' }] })");
  await waitFor(() => ui.evaluate("document.body.classList.contains('bar-island') && document.querySelectorAll('#mybtns .mybtn').length === 1 && getComputedStyle($('#toolbar')).borderRadius !== '0px'"));
  // над страницей остров висит: форма окна интерфейса — полоса вкладок и сам остров
  await waitFor(() => ui.evaluate("document.body.classList.contains('island-float') && chromeShape().parts.length >= 2"));
  await ui.evaluate("send('set', { key: 'island_tabs', value: true })");
  await waitFor(() => ui.evaluate("document.body.classList.contains('island-tabs') && getComputedStyle(el.chrome).display === 'grid' && $('#urlbox').offsetWidth >= 200"));
  await ui.evaluate("send('set', { key: 'island_tabs', value: false }); send('set', { key: 'bar_style', value: 'classic' }); send('set', { key: 'buttons', value: [] })");
  await waitFor(() => ui.evaluate("!document.body.classList.contains('bar-island') && !document.querySelectorAll('#mybtns .mybtn').length && lastChromeKey.endsWith('|0')"));
});

await check('разделённый экран и группы вкладок', async () => {
  await waitFor(() => ui.evaluate('S.tabs.length === 1')).catch((e) => { throw new Error('шаг 1: ' + e.message); });
  const first = await ui.evaluate('S.active');
  const visible = async (hash) => {
    const target = (await targets()).find((t) => t.url === FIXTURE + hash);
    if (!target) return false;
    const page = await connect((t) => t.id === target.id);
    try { return await page.evaluate('document.visibilityState') === 'visible'; } finally { page.close(); }
  };
  await ui.evaluate(`send('open', { url: ${JSON.stringify(FIXTURE + '#b')}, newTab: true })`);
  await waitFor(async () => (await targets()).some((t) => t.url === FIXTURE + '#b')).catch((e) => { throw new Error('шаг 2: ' + e.message); });
  const second = await ui.evaluate('S.active');
  await ui.evaluate("send('split')");
  await waitFor(async () => (await ui.evaluate(`S.split?.left === ${second} && S.split?.right === ${first}`)) && (await visible('')) && (await visible('#b')), 15000).catch((e) => { throw new Error('шаг 3: ' + e.message); });
  // закрыли активную половину — на экране остаётся вторая
  await ui.evaluate(`send('close_tab', { id: ${second} })`);
  await waitFor(() => ui.evaluate(`!S.split && S.active === ${first}`)).catch((e) => { throw new Error('шаг 4: ' + e.message); });
  // группы: вкладка, брошенная внутрь группы, вступает в неё
  for (const hash of ['#g1', '#g2']) {
    await ui.evaluate(`send('open', { url: ${JSON.stringify(FIXTURE + hash)}, newTab: true })`);
    await waitFor(async () => (await targets()).some((t) => t.url === FIXTURE + hash)).catch((e) => { throw new Error('шаг 5: ' + e.message); });
  }
  await waitFor(() => ui.evaluate('S.tabs.length === 3')).catch((e) => { throw new Error('шаг 6: ' + e.message); });
  const [, g1, g2] = await ui.evaluate('S.tabs.map((t) => t.id)');
  await ui.evaluate(`send('group_new', { id: ${g1} })`);
  await waitFor(() => ui.evaluate('S.groups.length === 1')).catch((e) => { throw new Error('шаг 7: ' + e.message); });
  await ui.evaluate('closeMenu()');
  await ui.evaluate(`send('group_add', { id: ${g2}, group: S.groups[0].id })`);
  await waitFor(() => ui.evaluate(`S.tabs.every((t) => (t.group !== 0) === (t.id !== ${first}))`)).catch((e) => { throw new Error('шаг 8: ' + e.message); });
  await ui.evaluate(`send('move_tab', { id: ${first}, index: 1 })`);
  await waitFor(() => ui.evaluate(`S.tabs[1].id === ${first} && S.tabs[1].group === S.groups[0].id`)).catch((e) => { throw new Error('шаг 9: ' + e.message); });
  await ui.evaluate(`send('group_remove', { id: ${first} })`);
  await waitFor(() => ui.evaluate(`S.tabs[1].group === 0`)).catch((e) => { throw new Error('шаг 10: ' + e.message); });
  // свернули группу с активной вкладкой — активна вкладка вне группы
  await ui.evaluate(`send('activate', { id: ${g2} }); send('group_edit', { group: S.groups[0].id, collapsed: true })`);
  await waitFor(() => ui.evaluate(`S.active === ${first}`)).catch((e) => { throw new Error('шаг 11: ' + e.message); });
  await ui.evaluate(`send('group_close', { group: S.groups[0].id })`);
  await waitFor(() => ui.evaluate(`S.tabs.length === 1 && !S.groups.length && S.active === ${first}`)).catch((e) => { throw new Error('шаг 12: ' + e.message); });
});

await check('узкий остров: адресная строка-кнопка раскрывается', async () => {
  await ui.evaluate("send('set', { key: 'bar_style', value: 'island' }); send('set', { key: 'island_width', value: 40 })");
  await waitFor(() => ui.evaluate("document.body.classList.contains('url-collapsed') && urlbox.offsetWidth < 50"));
  await ui.evaluate("urlbox.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))");
  await waitFor(() => ui.evaluate("urlbox.classList.contains('expanded') && urlbox.offsetWidth > 300"));
  await ui.evaluate("el.url.blur(); send('set', { key: 'island_width', value: 62 }); send('set', { key: 'bar_style', value: 'classic' })");
  await waitFor(() => ui.evaluate("!urlbox.classList.contains('expanded') && !document.body.classList.contains('url-collapsed')"));
});

await check('готовые раскладки, адрес во вкладке и «только сайт»', async () => {
  const server = createServer((req, res) => { res.setHeader('content-type', 'text/html; charset=utf-8'); res.end('<title>Страница</title><p>текст</p>'); });
  await new Promise((r) => server.listen(8768, '127.0.0.1', r));
  try {
    await ui.evaluate("send('navigate', { text: 'http://127.0.0.1:8768/a' })");
    await waitFor(() => ui.evaluate("activeTab()?.url.includes(':8768/a') && !activeTab().start")).catch((e) => { throw new Error('шаг 1: ' + e.message); });
    // раскладка «Остров» одним щелчком и подсветка выбранной
    await ui.evaluate("settingsCat = 'layout'; openHub('settings')");
    await waitFor(() => ui.evaluate("document.querySelectorAll('.preset').length === 5")).catch((e) => { throw new Error('шаг 2: ' + e.message); });
    await ui.evaluate("[...document.querySelectorAll('.preset')].find((b) => b.textContent.startsWith('Остров')).click()");
    await waitFor(() => ui.evaluate("S.settings.barStyle === 'island' && !!S.settings.islandTabs")).catch((e) => { throw new Error('шаг 3: ' + e.message); });
    await waitFor(() => ui.evaluate("document.querySelector('.preset.on b')?.textContent === 'Остров'")).catch((e) => { throw new Error('шаг 4: ' + e.message); });
    await ui.evaluate("closeHub(); send('set', { key: 'bar_style', value: 'classic' }); send('set', { key: 'island_tabs', value: false })");
    await waitFor(() => ui.evaluate("!layerKind"));
    // адрес во вкладке: строки нет, вкладка показывает адрес, Ctrl+L раскрывает ввод
    await ui.evaluate("send('set', { key: 'url_in_tab', value: true })");
    await waitFor(() => ui.evaluate("document.body.classList.contains('url-in-tab') && getComputedStyle(urlbox).display === 'none'")).catch((e) => { throw new Error('шаг 5: ' + e.message); });
    await waitFor(() => ui.evaluate("document.querySelector('.tab.active .title').textContent.includes('127.0.0.1:8768/a')")).catch((e) => { throw new Error('шаг 6: ' + e.message); });
    await ui.evaluate("ui.focusUrl()"); // как Ctrl+L
    await waitFor(() => ui.evaluate("urlbox.classList.contains('expanded') && urlbox.offsetWidth > 300")).catch((e) => { throw new Error('шаг 7: ' + e.message); });
    // в неактивном тестовом окне blur не приходит — закрываем подсказки и строку сами
    await ui.evaluate("el.url.blur(); closeSheet(); collapseUrl(); send('set', { key: 'url_in_tab', value: false })");
    await waitFor(() => ui.evaluate("!document.body.classList.contains('url-in-tab') && getComputedStyle(urlbox).display !== 'none'")).catch((e) => { throw new Error('шаг 8: ' + e.message); });
    // «только сайт»: панели — невидимая полоска, сайт с самого верха
    await ui.evaluate("send('site_only')");
    await waitFor(() => ui.evaluate("S.siteOnly && document.body.classList.contains('site-only') && window.__rbShape <= 6")).catch(async (e) => { throw new Error('шаг 9: ' + e.message); });
    await ui.evaluate("send('site_only')");
    await waitFor(() => ui.evaluate("!S.siteOnly && window.__rbShape > 60")).catch((e) => { throw new Error('шаг 10: ' + e.message); });
  } finally {
    server.close();
  }
});

await check('адресная строка исправляет раскладку и опечатки', async () => {
  await ui.evaluate("ui.focusUrl(); el.url.value = 'ghbdtn'; el.url.dispatchEvent(new Event('input', { bubbles: true }))");
  await waitFor(() => ui.evaluate("suggestRows[0]?.fix && suggestRows[0].url === 'привет' && selected === 0"));
  await ui.evaluate("el.url.value = 'пртвет'; el.url.dispatchEvent(new Event('input', { bubbles: true }))");
  await waitFor(() => ui.evaluate("suggestRows[0]?.fix && suggestRows[0].url === 'привет'"));
  await ui.evaluate("el.url.value = 'привет'; el.url.dispatchEvent(new Event('input', { bubbles: true }))");
  await waitFor(() => ui.evaluate("suggestRows.length > 0 && !suggestRows[0].fix"));
  await ui.evaluate("el.url.value = ''; el.url.blur(); closeSheet()");
});

await check('тема AMOLED и прозрачные панели', async () => {
  await ui.evaluate("send('set', { key: 'theme', value: 'amoled' }); send('set', { key: 'glass', value: 40 })");
  await waitFor(() => ui.evaluate("document.body.classList.contains('amoled') && document.body.classList.contains('glass') && getComputedStyle(document.body).getPropertyValue('--bg').trim() === '#000000'"));
  await ui.evaluate("send('set', { key: 'theme', value: 'dark' }); send('set', { key: 'glass', value: 0 })");
  await waitFor(() => ui.evaluate("!document.body.classList.contains('amoled') && !document.body.classList.contains('glass')"));
});

await check('менеджер паролей сохраняет и подставляет пароль', async () => {
  const page = '<!doctype html><meta charset="utf-8"><title>Вход</title><form action="/done"><input id="u" type="email"><input id="p" type="password"><button id="go">Войти</button></form>';
  const server = createServer((req, res) => { res.setHeader('content-type', 'text/html; charset=utf-8'); res.end(req.url.startsWith('/done') ? '<title>Готово</title>ok' : page); });
  await new Promise((r) => server.listen(8766, '127.0.0.1', r));
  try {
    await ui.evaluate("send('navigate', { text: 'http://127.0.0.1:8766/login' })");
    let login = await connect((t) => t.url.includes(':8766/login'));
    await waitFor(() => login.evaluate("!!document.getElementById('go')"));
    await login.evaluate("document.getElementById('u').value = 'e2e@test.local'; document.getElementById('p').value = 'e2e-Пароль-1'; document.getElementById('go').click()");
    login.close();
    await waitFor(() => ui.evaluate("!!pwOffer && menuKind === 'pwsave'"));
    await ui.evaluate("document.querySelector('.pw-acts .primary').click()");
    await waitFor(() => ui.evaluate("(S.logins || []).some((l) => l.username === 'e2e@test.local')"));
    await ui.evaluate("send('navigate', { text: 'http://127.0.0.1:8766/login' })");
    await waitFor(() => ui.evaluate("(S.pwHere || []).length === 1 && pwkey.classList.contains('show')"));
    await ui.evaluate("send('pw_fill', { index: S.pwHere[0][0] })");
    login = await connect((t) => t.url.includes(':8766/login'));
    const filled = await waitFor(() => login.evaluate("document.getElementById('p').value === 'e2e-Пароль-1' && document.getElementById('u').value"));
    login.close();
    if (filled !== 'e2e@test.local') throw new Error(`подставилось не то: ${filled}`);
    await ui.evaluate("send('pw_delete', { index: S.pwHere[0][0] }); send('navigate', { text: " + JSON.stringify(FIXTURE) + " })");
    await waitFor(() => ui.evaluate("!(S.logins || []).length"));
  } finally {
    server.close();
  }
});

await check('видеообои из галереи играют', async () => {
  const walls = await ui.evaluate('(S.settings.liveWalls || []).length');
  if (!walls) throw new Error('галерея пуста: запустите assets/walls/fetch.ps1');
  const before = await tabsCount();
  await ui.evaluate("send('set', { key: 'wallpaper', value: 'live:snow' }); send('new_tab')");
  await waitFor(() => ui.evaluate("document.body.dataset.wall === 'video' && $('#wallvideo').readyState >= 2 && !$('#wallvideo').paused"), 10000);
  await ui.evaluate("send('set', { key: 'wallpaper', value: 'aurora' }); send('close_tab', { id: S.active })");
  await waitFor(async () => (await tabsCount()) === before);
});

await check('аккаунт: создание, неверный пароль, вход', async () => {
  const cloud = mkdtempSync(join(tmpdir(), 'rb-cloud-'));
  try {
    const args = (password) => JSON.stringify({ name: 'e2e-user', password, folder: cloud });
    await ui.evaluate(`send('account_create', ${args('e2e-sync-пароль')})`);
    await waitFor(() => ui.evaluate("(S.account || {}).name === 'e2e-user' && (S.account || {}).synced > 0"), 60000, 300);
    if (!existsSync(cloud)) throw new Error('нет папки');
    await ui.evaluate("send('account_logout')");
    await waitFor(() => ui.evaluate('!(S.account || {}).name'));
    await ui.evaluate(`accountErrorText = ''; send('account_login', ${args('не тот пароль')})`);
    await waitFor(() => ui.evaluate("accountErrorText === 'Неверный пароль'"), 60000, 300);
    await ui.evaluate(`send('account_login', ${args('e2e-sync-пароль')})`);
    await waitFor(() => ui.evaluate("(S.account || {}).name === 'e2e-user'"), 60000, 300);
    await ui.evaluate("send('account_logout')");
  } finally {
    rmSync(cloud, { recursive: true, force: true });
  }
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
