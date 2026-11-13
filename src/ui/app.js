'use strict';

// ---------- помощники ----------

const BOOT = window.__BOOT__ || { inprivate: false, version: '', engine: '' };
const $ = (selector, root = document) => root.querySelector(selector);
const send = (cmd, data = {}) => window.ipc.postMessage(JSON.stringify({ cmd, ...data }));

/** Создаёт элемент. Данные со страниц попадают только как текст, не как HTML. */
function h(tag, attrs = {}, ...kids) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value == null || value === false) continue;
    if (key === 'class') el.className = value;
    else if (key === 'style') el.style.cssText = value;
    else if (key.startsWith('on')) el.addEventListener(key.slice(2), value);
    else el.setAttribute(key, value === true ? '' : value);
  }
  for (const kid of kids.flat()) if (kid != null && kid !== false) el.append(kid);
  return el;
}

const ICONS = {
  back: '<path d="M19 12H5"/><path d="m12 19-7-7 7-7"/>',
  forward: '<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>',
  reload: '<path d="M21 12a9 9 0 1 1-2.64-6.36L21 8"/><path d="M21 3v5h-5"/>',
  x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
  plus: '<path d="M12 5v14"/><path d="M5 12h14"/>',
  minus: '<path d="M5 12h14"/>',
  star: '<path d="m12 2 3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01z"/>',
  shield: '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>',
  download: '<path d="M12 4v11"/><path d="m7 11 5 5 5-5"/><path d="M5 20h14"/>',
  clock: '<circle cx="12" cy="12" r="9.5"/><path d="M12 7v5l3 2"/>',
  library: '<path d="M4 4v16"/><path d="M9 4v16"/><path d="m14 5 5 15"/>',
  lock: '<rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>',
  alert: '<circle cx="12" cy="12" r="9.5"/><path d="M12 7.5v5"/><path d="M12 16.5h.01"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20.5 20.5-4.5-4.5"/>',
  globe: '<circle cx="12" cy="12" r="9.5"/><path d="M2.5 12h19"/><path d="M12 2.5c3 3 3 16 0 19"/><path d="M12 2.5c-3 3-3 16 0 19"/>',
  trash: '<path d="M4 7h16"/><path d="M18 7v12a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V7"/><path d="M9 7V4h6v3"/>',
  folder: '<path d="M3 6a2 2 0 0 1 2-2h4l2 3h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
  open: '<path d="M14 4h6v6"/><path d="M20 4 10 14"/><path d="M19 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h4"/>',
  check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
  chevron: '<path d="m6 9 6 6 6-6"/>',
  up: '<path d="m6 15 6-6 6 6"/>',
  down: '<path d="m6 9 6 6 6-6"/>',
  dots: '<circle cx="5" cy="12" r="1.3" fill="currentColor"/><circle cx="12" cy="12" r="1.3" fill="currentColor"/><circle cx="19" cy="12" r="1.3" fill="currentColor"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>',
  mask: '<path d="M3 10c0-2 1.5-3 4-3h10c2.5 0 4 1 4 3 0 4-2.5 7-5 7-1.6 0-2.4-1.5-4-1.5S9.6 17 8 17c-2.5 0-5-3-5-7z"/><circle cx="8.5" cy="11" r="1.4" fill="currentColor"/><circle cx="15.5" cy="11" r="1.4" fill="currentColor"/>',
  print: '<path d="M6 9V3h12v6"/><rect x="3" y="9" width="18" height="8" rx="2"/><path d="M7 14h10v7H7z"/>',
  code: '<path d="m8 7-5 5 5 5"/><path d="m16 7 5 5-5 5"/>',
  expand: '<path d="M4 9V4h5"/><path d="M20 9V4h-5"/><path d="M4 15v5h5"/><path d="M20 15v5h-5"/>',
  zoom: '<circle cx="11" cy="11" r="7"/><path d="m20.5 20.5-4.5-4.5"/><path d="M8 11h6"/><path d="M11 8v6"/>',
  bar: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 9h18"/><path d="M7 6.5h3"/>',
  pin: '<path d="M9 4h6l-1 5 3 3v2H7v-2l3-3z"/><path d="M12 14v6"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>',
  speaker: '<path d="M4 9v6h4l5 4V5L8 9z"/><path d="M16.5 8.5a5 5 0 0 1 0 7"/><path d="M19 6a8.5 8.5 0 0 1 0 12"/>',
  muted: '<path d="M4 9v6h4l5 4V5L8 9z"/><path d="m17 9 5 6"/><path d="m22 9-5 6"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  pause: '<path d="M8 5v14"/><path d="M16 5v14"/>',
  play: '<path d="M7 4.5v15l12-7.5z"/>',
  cookie: '<path d="M12 3a9 9 0 1 0 9 9 3 3 0 0 1-3-3 3 3 0 0 1-3-3 3 3 0 0 1-3-3z"/><path d="M8.5 11.5h.01M12 16h.01M15.5 13h.01" stroke-width="2.6"/>',
  gauge: '<path d="M4.5 18a9 9 0 1 1 15 0"/><path d="m12 13 4-5"/><circle cx="12" cy="13" r="1.4" fill="currentColor"/>',
  moon: '<path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/>',
  leaf: '<path d="M5 19c0-8 5-13 14-14 0 9-5 14-13 14z"/><path d="M5 19 13 11"/>',
  bolt: '<path d="M13 3 5 14h6l-1 7 8-11h-6z"/>',
  window: '<rect x="3" y="5" width="18" height="15" rx="2"/><path d="M3 9h18"/>',
  image: '<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="10" r="2"/><path d="m21 16-5-5-9 9"/>',
  smile: '<circle cx="12" cy="12" r="9.5"/><path d="M8 14.5a5 5 0 0 0 8 0"/><path d="M9 9.5h.01M15 9.5h.01" stroke-width="2.6"/>',
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0"/><path d="M12 17.5V21"/>',
  undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>',
  redo: '<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>',
  cut: '<circle cx="6" cy="18" r="3"/><circle cx="18" cy="18" r="3"/><path d="M8.1 15.9 18 4"/><path d="M15.9 15.9 6 4"/>',
  paste: '<rect x="6" y="4" width="12" height="17" rx="2"/><path d="M9 4h6v2H9z"/>',
  pastego: '<rect x="4" y="4" width="11" height="16" rx="2"/><path d="M7 4h5v2H7z"/><path d="M14 15h7"/><path d="m18 12 3 3-3 3"/>',
  selectall: '<path d="M4 7V5a1 1 0 0 1 1-1h2M17 4h2a1 1 0 0 1 1 1v2M20 17v2a1 1 0 0 1-1 1h-2M7 20H5a1 1 0 0 1-1-1v-2"/><path d="M9 9h6v6H9z"/>',
  'win-min': '<path d="M5 12h14" stroke-width="1.2"/>',
  'win-max': '<rect x="5.5" y="5.5" width="13" height="13" rx="2" stroke-width="1.2"/>',
  'win-restore': '<rect x="5.5" y="8.5" width="10" height="10" rx="2" stroke-width="1.2"/><path d="M8.5 5.5h8a2 2 0 0 1 2 2v8" stroke-width="1.2"/>',
  'win-close': '<path d="m6 6 12 12M18 6 6 18" stroke-width="1.2"/>',
};

function icon(name, size = 17) {
  const template = document.createElement('template');
  const small = name.startsWith('win-') ? 15 : size;
  template.innerHTML =
    `<svg viewBox="0 0 24 24" width="${small}" height="${small}" fill="none" stroke="currentColor" ` +
    `stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${ICONS[name]}</svg>`;
  return template.content.firstChild;
}

for (const el of document.querySelectorAll('[data-icon]')) el.prepend(icon(el.dataset.icon));

function parseUrl(url) {
  try { return new URL(url); } catch { return null; }
}

const hostOf = (url) => (parseUrl(url)?.hostname || '').replace(/^www\./, '');

/** Адрес в том виде, в каком его приятно читать: без https:// и хвостового слэша. */
function prettyUrl(url) {
  return url.replace(/^https:\/\//, '').replace(/^www\./, '').replace(/\/$/, '');
}

function hashHue(text) {
  let hash = 0;
  for (const ch of text) hash = (hash * 31 + ch.codePointAt(0)) >>> 0;
  return hash % 360;
}

/** Значок сайта: известный адрес значка, иначе favicon.ico, иначе цветная буква. */
function favicon(url, large = false, known = '') {
  const parsed = parseUrl(url);
  const box = h('span', { class: large ? 'fav lg' : 'fav' });
  if (!parsed || !parsed.protocol.startsWith('http')) {
    box.append(icon('globe', large ? 22 : 14));
    return box;
  }
  // Сразу цветная буква, а картинка подменяет её, только когда загрузилась:
  // через медленную сеть значок может идти секунды, пустого места не будет.
  const host = hostOf(url) || '?';
  box.textContent = host[0].toUpperCase();
  box.classList.add('letter');
  box.style.setProperty('--hue', hashHue(host));
  const img = new Image();
  img.onload = () => {
    if (img.naturalWidth < 2) return;
    box.classList.remove('letter');
    box.replaceChildren(img);
  };
  img.src = known || parsed.origin + '/favicon.ico';
  return box;
}

function formatBytes(bytes) {
  if (!(bytes > 0)) return '0 Б';
  const units = ['Б', 'КБ', 'МБ', 'ГБ'];
  const power = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** power;
  return `${value.toFixed(value >= 100 || power === 0 ? 0 : 1)} ${units[power]}`;
}

/** Перезапускает CSS-анимацию, привязанную к классу. */
function replay(el, cls) {
  el.classList.remove(cls);
  void el.offsetWidth;
  el.classList.add(cls);
}

let toastTimer = 0;
function toast(text) {
  const el = $('#toast');
  el.textContent = text;
  el.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), 2200);
}

// ---------- состояние ----------

let S = {
  tabs: [], active: 0, bookmarked: false, bookmarks: [],
  downloads: [], adblock: true, blockedTotal: 0,
  engines: [], search: '', settings: {}, dark: false, maximized: false, fullscreen: false,
};
const activeTab = () => S.tabs.find((tab) => tab.id === S.active);
const engineName = () => S.engines.find((engine) => engine.id === S.search)?.name || 'интернете';

const el = {
  chrome: $('#chrome'), titlebar: $('#titlebar'), tabs: $('#tabs'),
  back: $('#back'), forward: $('#forward'), reload: $('#reload'),
  url: $('#url'), siteicon: $('#siteicon'), star: $('#star'), zoombadge: $('#zoombadge'),
  shield: $('#shield'), badge: $('#shieldbadge'), downloads: $('#downloads'),
  hubbtn: $('#hubbtn'), menubtn: $('#menubtn'), progress: $('#progress'),
  favbar: $('#favbar'), findq: $('#findq'), findcount: $('#findcount'),
  sheet: $('#sheet'), suggestions: $('#suggestions'),
  clock: $('#clock'), date: $('#date'), startq: $('#startq'), tiles: $('#tiles'), stat: $('#stat'),
  layer: $('#layer'), backdrop: $('#backdrop'),
  hub: $('#hub'), hubh1: $('#hubh1'), hubq: $('#hubq'), hubaction: $('#hubaction'),
  hublist: $('#hublist'), hubind: $('#hubind'), menu: $('#menu'), catcher: $('#menucatcher'),
};

document.body.classList.toggle('inprivate', BOOT.inprivate);
$('#hubfoot').textContent = `Rust Browser ${BOOT.version}` + (BOOT.engine ? ` · WebView2 ${BOOT.engine}` : '');

// ---------- оформление: цвет, анимации, виды панели и новой вкладки ----------

function hexToHsl(hex) {
  const n = parseInt(hex.slice(1), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => v / 255);
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  let h = 0, s = 0;
  const l = (max + min) / 2;
  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
    h *= 60;
  }
  return [h, s, l];
}

const hsl = (h, s, l) => `hsl(${((h % 360) + 360) % 360} ${Math.round(s * 100)}% ${Math.round(l * 100)}%)`;

let appearanceKey = null;
function applyAppearance(s) {
  const key = [s.accent, s.tintFrame, s.motion, s.barStyle, s.newtabLayout, S.dark].join('|');
  if (key === appearanceKey) return;
  appearanceKey = key;
  const root = document.documentElement.style;
  if (s.accent) {
    // из одного цвета — вся палитра: оттенок для тёмной и светлой темы и два соседних для узоров
    const [h, sat, l] = hexToHsl(s.accent);
    const light = S.dark ? Math.max(l, 0.68) : Math.min(Math.max(l, 0.42), 0.56);
    const satx = Math.min(1, Math.max(sat, 0.55));
    root.setProperty('--accent', hsl(h, satx, light));
    root.setProperty('--accent-2', hsl(h + 50, satx, light));
    root.setProperty('--accent-3', hsl(h - 140, satx * 0.9, light));
    root.setProperty('--on-accent', light > 0.6 ? '#10111a' : '#ffffff');
    root.setProperty('--frame-glow', hsl(h, satx, light).replace(')', ' / 0.14)'));
  } else {
    for (const name of ['--accent', '--accent-2', '--accent-3', '--on-accent', '--frame-glow']) root.removeProperty(name);
  }
  const body = document.body.classList;
  body.toggle('tint', !!s.tintFrame);
  for (const m of ['smooth', 'fast', 'bouncy', 'off']) body.toggle('motion-' + m, s.motion === m);
  for (const b of ['classic', 'compact', 'floating', 'vertical']) body.toggle('bar-' + b, s.barStyle === b);
  for (const n of ['clock', 'minimal', 'tiles', 'greeting']) body.toggle('nt-' + n, s.newtabLayout === n);
}

const PALETTE = [
  ['', 'Индиго', '#4f5bf5'], ['#2f7cf6', 'Синий'], ['#12a4b5', 'Бирюза'], ['#22a06b', 'Зелёный'],
  ['#7bb31a', 'Лайм'], ['#e39b0e', 'Янтарь'], ['#f2672a', 'Оранжевый'], ['#e5484d', 'Красный'],
  ['#e04a9a', 'Розовый'], ['#8b5cf6', 'Фиолетовый'], ['#6b7280', 'Графит'],
];

function colorSetting() {
  const swatches = PALETTE.map(([value, name, shown]) => {
    const button = h('button', { class: 'swatch', title: name, style: `--c:${shown || value}`, onclick: () => set('accent', value) });
    settingsUpdaters.push(() => button.classList.toggle('on', (S.settings.accent || '') === value));
    return button;
  });
  const picker = h('input', { type: 'color', class: 'swatch custom', title: 'Свой цвет' });
  picker.addEventListener('input', () => set('accent', picker.value));
  settingsUpdaters.push(() => {
    const custom = S.settings.accent && !PALETTE.some(([v]) => v === S.settings.accent);
    picker.classList.toggle('on', !!custom);
    picker.value = S.settings.accent || '#4f5bf5';
  });
  return h('div', { class: 'set wallset' },
    h('div', { class: 'label' }, 'Цвет браузера', h('small', {}, 'Кнопки, выделение, узоры обоев — последний кружок для своего цвета')),
    h('div', { class: 'walls swatches' }, ...swatches, picker));
}

function setState(next) {
  // Rust присылает только изменившиеся части: закладки, загрузки и параметры — не каждый раз.
  S = { ...S, ...next };
  document.documentElement.dataset.theme = S.dark ? 'dark' : 'light';
  document.body.classList.toggle('maximized', S.maximized);
  document.body.classList.toggle('fullscreen', S.fullscreen);
  document.body.classList.toggle('favbar', !!S.settings.favbar);
  applyAppearance(S.settings);
  renderTabs();
  renderToolbar();
  renderFavbar();
  renderStart();
  if (hubSection && hubSection !== 'history') renderHubList(false);
  if (menuKind === 'main') refreshMainMenu();
}

// Высоту панели знает только интерфейс: сообщаем её Rust, чтобы страница встала ровно под ней.
let lastChrome = 0;
// При вертикальных вкладках панель — колонка слева: тогда важна её ширина.
let lastChromeKey = '';
const vertical = () => document.body.classList.contains('bar-vertical');
new ResizeObserver(() => {
  const box = el.chrome.getBoundingClientRect();
  const side = vertical() ? Math.round(box.width) : 0;
  const height = side ? 0 : Math.round(box.height);
  if (!height && !side) return;
  const key = height + '|' + side;
  if (key === lastChromeKey) return;
  lastChromeKey = key;
  lastChrome = height || side;
  document.documentElement.style.setProperty('--bar-h', height + 'px');
  document.documentElement.style.setProperty('--side-w', side + 'px');
  send('chrome', { height, side });
}).observe(el.chrome);

// ---------- окно ----------

$('#win-min').addEventListener('click', () => send('minimize'));
$('#win-max').addEventListener('click', () => send('maximize'));
$('#win-close').addEventListener('click', () => send('close'));

// Пустое место в полосе вкладок двигает окно, двойной щелчок разворачивает.
el.titlebar.addEventListener('mousedown', (event) => {
  if (event.button !== 0) return;
  const free = event.target === el.titlebar || event.target.classList.contains('drag') || event.target === el.tabs;
  if (!free) return;
  if (event.detail === 2) send('maximize');
  else send('drag');
});
for (const edge of document.querySelectorAll('.rz')) {
  edge.addEventListener('mousedown', (event) => {
    if (event.button === 0) send('resize', { dir: edge.dataset.dir });
  });
}

// ---------- вкладки ----------

const tabEls = new Map();

const newTabBtn = $('#newtab');

function aliveTabNodes() {
  return [...el.tabs.children].filter((node) => node.classList.contains('tab') && !node.classList.contains('leave'));
}

function renderTabs() {
  const before = new Map(aliveTabNodes().map((node) => [node, node.getBoundingClientRect().left]));

  const alive = new Set(S.tabs.map((tab) => tab.id));
  for (const [id, node] of tabEls) {
    if (alive.has(id)) continue;
    tabEls.delete(id);
    node.classList.add('leave');
    setTimeout(() => node.remove(), 260);
  }
  for (const tab of S.tabs) {
    let node = tabEls.get(tab.id);
    if (!node) {
      node = makeTab(tab.id);
      tabEls.set(tab.id, node);
      el.tabs.insertBefore(node, newTabBtn);
    }
    updateTab(node, tab);
  }

  // Порядок мог поменяться (перетаскивание, закрепление) — переставляем и анимируем сдвиг.
  const wanted = S.tabs.map((tab) => tabEls.get(tab.id));
  const current = aliveTabNodes();
  if (wanted.some((node, i) => node !== current[i])) {
    for (const node of wanted) el.tabs.insertBefore(node, newTabBtn);
    for (const node of wanted) {
      const was = before.get(node);
      if (was == null) continue;
      const dx = was - node.getBoundingClientRect().left;
      if (Math.abs(dx) > 1) {
        node.animate([{ transform: `translateX(${dx}px)` }, { transform: 'none' }],
          { duration: 240, easing: 'cubic-bezier(.2,.8,.2,1)' });
      }
    }
  }
}

function makeTab(id) {
  const close = h('span', { class: 'close tab-btn', title: 'Закрыть вкладку (Ctrl+W)' }, icon('x', 13));
  close.addEventListener('click', (event) => {
    event.stopPropagation();
    send('close_tab', { id });
  });
  const audio = h('span', { class: 'audio tab-btn' });
  audio.addEventListener('click', (event) => {
    event.stopPropagation();
    send('mute', { id });
  });
  const node = h('div', { class: 'tab' },
    h('i', { class: 'bg' }), h('i', { class: 'foot l' }), h('i', { class: 'foot r' }),
    h('span', { class: 'fav' }), h('span', { class: 'title' }), audio, close);
  node.dataset.id = id;
  node.addEventListener('pointerdown', (event) => startTabDrag(event, id, node));
  node.addEventListener('auxclick', (event) => {
    if (event.button === 1) send('close_tab', { id });
  });
  node.addEventListener('contextmenu', (event) => {
    event.preventDefault();
    openTabMenu(id, event.clientX, event.clientY);
  });
  return node;
}

function updateTab(node, tab) {
  node.classList.toggle('active', tab.id === S.active);
  node.classList.toggle('pinned', tab.pinned);
  node.classList.toggle('sleeping', tab.sleeping && tab.id !== S.active);
  const title = tab.start ? 'Новая вкладка' : tab.title || prettyUrl(tab.url) || 'Загрузка…';
  const titleEl = node.querySelector('.title');
  if (titleEl.textContent !== title) titleEl.textContent = title;
  node.title = tab.sleeping && tab.id !== S.active ? `${title}\nВкладка спит и не тратит ресурсы` : title;

  // Значок пересоздаём, только когда он действительно меняется.
  const key = tab.loading ? 'loading' : tab.start ? 'start' : (tab.favicon || parseUrl(tab.url)?.origin || '');
  if (node.dataset.fav !== key) {
    node.dataset.fav = key;
    const fav =
      key === 'loading' ? h('span', { class: 'spinner fav' })
      : key === 'start' ? h('span', { class: 'fav' }, icon('search', 14))
      : favicon(tab.url, false, tab.favicon);
    node.querySelector('.fav').replaceWith(fav);
  }

  const audio = node.querySelector('.audio');
  const audioKey = tab.muted ? 'muted' : tab.audio ? 'on' : '';
  if (audio.dataset.k !== audioKey) {
    audio.dataset.k = audioKey;
    audio.className = 'audio tab-btn' + (audioKey ? ' on' : '') + (tab.muted ? ' muted' : '');
    audio.replaceChildren(icon(tab.muted ? 'muted' : 'speaker', 14));
    audio.title = tab.muted ? 'Включить звук вкладки' : 'Выключить звук вкладки';
  }
}

/** Нажатие активирует вкладку, а протягивание переставляет её среди соседей. */
function startTabDrag(event, id, node) {
  if (event.button !== 0 || event.target.closest('.tab-btn')) return;
  send('activate', { id });

  const nodes = aliveTabNodes();
  const from = nodes.indexOf(node);
  // вкладки в строку тянутся по горизонтали, в колонку — по вертикали
  const vert = vertical();
  const axis = vert
    ? { pos: (e) => e.clientY, start: (r) => r.top, end: (r) => r.bottom, size: (r) => r.height, move: 'translateY' }
    : { pos: (e) => e.clientX, start: (r) => r.left, end: (r) => r.right, size: (r) => r.width, move: 'translateX' };
  const rects = nodes.map((n) => n.getBoundingClientRect());
  const startX = axis.pos(event);
  let dragging = false;
  let target = from;
  node.setPointerCapture(event.pointerId);

  let detach = false;
  const strip = el.tabs.getBoundingClientRect();
  const move = (ev) => {
    // протянули вниз далеко за полосу вкладок — вкладка уедет в новое окно
    const away = vert ? ev.clientX > strip.right + 70 : ev.clientY > strip.bottom + 70;
    detach = dragging && away && !S.tabs.find((t) => t.id === id)?.start;
    node.classList.toggle('detaching', detach);
    const dx = axis.pos(ev) - startX;
    if (!dragging && Math.abs(dx) < 6) return;
    if (!dragging) {
      dragging = true;
      node.classList.add('dragging');
      nodes.forEach((n) => n !== node && n.classList.add('shifting'));
    }
    const min = axis.start(rects[0]) - axis.start(rects[from]);
    const max = axis.end(rects[rects.length - 1]) - axis.end(rects[from]);
    const shift = Math.max(min, Math.min(max, dx));
    node.style.transform = `${axis.move}(${shift}px)`;

    const center = axis.start(rects[from]) + axis.size(rects[from]) / 2 + shift;
    target = from;
    rects.forEach((r, i) => {
      const middle = axis.start(r) + axis.size(r) / 2;
      // запас в пиксель: дотянутая до края вкладка должна встать на крайнее место,
      // даже если дробные координаты чуть разошлись
      if (i < from && center <= middle + 1) target = Math.min(target, i);
      if (i > from && center >= middle - 1) target = Math.max(target, i);
    });
    const width = axis.size(rects[from]) + (vert ? 2 : 0);
    nodes.forEach((n, i) => {
      if (n === node) return;
      let offset = 0;
      if (from < i && i <= target) offset = -width;
      else if (target <= i && i < from) offset = width;
      n.style.transform = offset ? `${axis.move}(${offset}px)` : '';
    });
  };

  const up = () => {
    node.removeEventListener('pointermove', move);
    node.removeEventListener('pointerup', up);
    node.removeEventListener('pointercancel', up);
    if (!dragging) return;
    // Сразу ставим вкладку на новое место, чтобы ничего не прыгало до ответа Rust.
    for (const n of nodes) {
      n.classList.remove('shifting', 'dragging', 'detaching');
      n.style.transform = '';
    }
    if (detach) {
      send('detach', { id });
      return;
    }
    if (target !== from) {
      const ref = target > from ? nodes[target].nextSibling : nodes[target];
      el.tabs.insertBefore(node, ref);
      send('move_tab', { id, index: target });
    }
  };

  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', up);
  node.addEventListener('pointercancel', up);
}

newTabBtn.addEventListener('click', () => send('new_tab'));
el.tabs.addEventListener('dblclick', (event) => {
  if (event.target === el.tabs) send('new_tab');
});

// ---------- панель навигации ----------

let lastTabId = 0;
let lastBookmarked = false;
let lastBlocked = 0;
let lastZoom = 1;

function renderToolbar() {
  const tab = activeTab();
  if (!tab) return;
  const switched = tab.id !== lastTabId;
  lastTabId = tab.id;
  if (document.body.classList.contains('start') !== tab.start) {
    document.body.classList.toggle('start', tab.start);
    updateLive();
  }
  if (switched && tab.start) wake();

  el.back.disabled = !tab.canBack;
  el.forward.disabled = !tab.canForward;
  el.reload.disabled = tab.start;
  el.reload.classList.toggle('loading', tab.loading);
  el.reload.title = tab.loading ? 'Остановить (Esc)' : 'Обновить (F5)';
  el.progress.classList.toggle('loading', tab.loading);

  if (document.activeElement !== el.url || switched) {
    el.url.value = prettyUrl(tab.url);
    if (switched) closeSheet();
  }
  const scheme = parseUrl(tab.url)?.protocol;
  const kind = scheme === 'https:' ? 'secure' : scheme === 'http:' ? 'insecure' : '';
  if (el.siteicon.dataset.kind !== kind) {
    el.siteicon.dataset.kind = kind;
    el.siteicon.className = kind;
    el.siteicon.replaceChildren(icon(kind === 'secure' ? 'lock' : kind === 'insecure' ? 'alert' : 'search', 15));
    el.siteicon.title =
      kind === 'secure' ? 'Защищённое соединение' : kind === 'insecure' ? 'Соединение не защищено' : '';
  }

  const zoom = Math.round(tab.zoom * 100);
  el.zoombadge.textContent = zoom + '%';
  el.zoombadge.classList.toggle('show', zoom !== 100 && !tab.start);
  if (tab.zoom !== lastZoom && !switched) replay(el.zoombadge, 'bump');
  lastZoom = tab.zoom;

  el.star.style.visibility = tab.start ? 'hidden' : 'visible';
  el.star.classList.toggle('on', S.bookmarked);
  el.star.title = S.bookmarked ? 'Убрать из избранного (Ctrl+D)' : 'Добавить в избранное (Ctrl+D)';
  if (S.bookmarked && !lastBookmarked && !switched) replay(el.star, 'pop');
  lastBookmarked = S.bookmarked;

  el.shield.classList.toggle('off', !S.adblock);
  el.shield.title = S.adblock
    ? `Блокировка рекламы включена — на этой странице: ${tab.blocked}. Нажмите, чтобы выключить`
    : 'Блокировка рекламы выключена. Нажмите, чтобы включить';
  const count = S.adblock ? tab.blocked : 0;
  el.badge.textContent = count > 99 ? '99+' : count;
  el.badge.classList.toggle('show', count > 0);
  if (count > lastBlocked && !switched) replay(el.badge, 'bump');
  lastBlocked = count;

  if (switched && finding) closeFind(false);
  renderDownloadRing();
}

function renderDownloadRing() {
  const running = S.downloads.filter((d) => d.state === 'progress' || d.state === 'paused');
  el.downloads.classList.toggle('active', running.length > 0);
  const total = running.reduce((sum, d) => sum + Math.max(d.total, 0), 0);
  const received = running.reduce((sum, d) => sum + d.received, 0);
  const known = running.length > 0 && running.every((d) => d.total > 0);
  el.downloads.classList.toggle('unknown', running.length > 0 && !known);
  if (known) {
    el.downloads.style.setProperty('--offset', (100.5 * (1 - received / total)).toFixed(1));
  } else {
    el.downloads.style.removeProperty('--offset');
  }
}

el.siteicon.addEventListener('mousedown', (event) => event.preventDefault());
el.siteicon.addEventListener('click', () => {
  const tab = activeTab();
  if (tab && !tab.start && /^https?:/.test(tab.url)) send('site_info');
});

function showSiteInfo(info) {
  const tab = activeTab();
  const zoom = Math.round(info.zoom * 100);
  const row = (name, text) => h('div', { class: 'si-row' }, h('span', { class: 'ic' }, icon(name, 16)), h('span', { class: 'lbl' }, text));
  const nodes = [
    h('div', { class: 'si-head ' + (info.secure ? 'ok' : 'bad') },
      h('span', { class: 'si-icon' }, icon(info.secure ? 'lock' : 'alert', 20)),
      h('div', {},
        h('b', {}, info.secure ? 'Подключение защищено' : 'Подключение не защищено'),
        h('small', {}, info.host))),
    h('div', { class: 'si-note' }, info.secure
      ? 'Данные между вами и сайтом шифруются: их не прочитать по пути.'
      : 'Не вводите на этом сайте пароли и данные карт — их могут перехватить.'),
    sep(),
    row('shield', S.adblock
      ? (info.ubol ? `Реклама и трекеры блокируются uBlock Origin Lite${info.blocked ? ` · по списку доменов: ${info.blocked}` : ''}` : `Заблокировано на странице: ${info.blocked}`)
      : 'Блокировка рекламы выключена'),
    h('div', { class: 'si-row' },
      h('span', { class: 'ic' }, icon('cookie', 16)),
      h('span', { class: 'lbl' }, `Файлы cookie сайта: ${info.cookies}`),
      info.cookies > 0 && h('button', { class: 'text-btn outline', onclick: () => send('site_clear_cookies') }, 'Удалить')),
    zoom !== 100 && h('div', { class: 'si-row' },
      h('span', { class: 'ic' }, icon('zoom', 16)),
      h('span', { class: 'lbl' }, `Масштаб для этого сайта: ${zoom}%`),
      h('button', { class: 'text-btn outline', onclick: () => { send('zoom', { dir: 'reset' }); closeMenu(); } }, 'Сбросить')),
  ].filter(Boolean);
  if (!tab) return;
  showMenu('site', nodes, () => {
    const anchor = el.siteicon.getBoundingClientRect();
    el.menu.style.left = Math.max(8, anchor.left - 10) + 'px';
    el.menu.style.top = anchor.bottom + 10 + 'px';
    el.menu.style.transformOrigin = 'top left';
  });
}

el.back.addEventListener('click', () => send('back'));
el.forward.addEventListener('click', () => send('forward'));
el.reload.addEventListener('click', () => send(activeTab()?.loading ? 'stop' : 'reload'));
el.star.addEventListener('click', () => send('bookmark_toggle'));
el.zoombadge.addEventListener('click', () => send('zoom', { dir: 'reset' }));
el.shield.addEventListener('click', () => {
  send('adblock_toggle');
  toast(S.adblock ? 'Блокировка рекламы выключена' : 'Блокировка рекламы включена');
});
el.downloads.addEventListener('click', () => openHub('downloads'));
el.hubbtn.addEventListener('click', () => openHub(hubSection || 'bookmarks'));
el.menubtn.addEventListener('click', () => (menuKind === 'main' ? closeMenu() : openMainMenu()));

// ---------- панель избранного ----------

let favKey = null;

function renderFavbar() {
  if (!S.settings.favbar) return;
  const key = JSON.stringify(S.bookmarks);
  if (key === favKey) return;
  favKey = key;
  const chips = S.bookmarks.map((bookmark) => h('button', {
    class: 'chip', title: `${bookmark.title}\n${bookmark.url}`,
    onclick: (event) => send('open', { url: bookmark.url, background: event.ctrlKey }),
    onauxclick: (event) => { if (event.button === 1) send('open', { url: bookmark.url, background: true }); },
  }, favicon(bookmark.url), h('span', {}, bookmark.title || hostOf(bookmark.url))));
  el.favbar.replaceChildren(...(chips.length
    ? chips
    : [h('span', { class: 'favbar-hint' }, 'Добавляйте сайты в избранное звёздочкой — они появятся здесь')]));
}

// ---------- поиск по странице ----------

let finding = false;
let findTimer = 0;

function openFind() {
  if (activeTab()?.start) return;
  finding = true;
  document.body.classList.add('finding');
  el.findq.focus();
  el.findq.select();
  if (el.findq.value.trim()) send('find', { term: el.findq.value });
}

function closeFind(notify = true) {
  if (!finding) return;
  finding = false;
  document.body.classList.remove('finding');
  el.findcount.textContent = '';
  if (notify) send('find_close');
}

function setFind(index, count) {
  if (!finding) return;
  const none = el.findq.value && count === 0;
  el.findcount.classList.toggle('none', none);
  // WebView2 считает совпадения с единицы; до первого перехода номер может быть -1.
  el.findcount.textContent = !el.findq.value ? '' : none ? 'Нет совпадений'
    : index > 0 ? `${index} из ${count}` : `${count} совп.`;
}

el.findq.addEventListener('input', () => {
  clearTimeout(findTimer);
  findTimer = setTimeout(() => {
    if (el.findq.value) send('find', { term: el.findq.value });
    else { el.findcount.textContent = ''; send('find', { term: '' }); }
  }, 120);
});
el.findq.addEventListener('keydown', (event) => {
  if (event.key === 'Enter') send(event.shiftKey ? 'find_prev' : 'find_next');
  else if (event.key === 'Escape') { event.stopPropagation(); closeFind(); }
});
$('#findnext').addEventListener('click', () => send('find_next'));
$('#findprev').addEventListener('click', () => send('find_prev'));
$('#findclose').addEventListener('click', () => closeFind());

// ---------- адресная строка и подсказки ----------

let suggestSeq = 0;
let suggestRows = [];
let selected = -1;
let sheetOpen = false;

let sheetHeight = 0;

function closeSheet() {
  suggestSeq++; // запоздавшие ответы больше не подходят
  if (!sheetOpen) return;
  sheetOpen = false;
  sheetHeight = 0;
  selected = -1;
  suggestRows = [];
  el.sheet.classList.remove('open');
  if (!layerKind) send('overlay', { mode: 'none' });
}

function go(text) {
  closeSheet();
  el.url.blur();
  send('navigate', { text });
}

function setSuggestions(seq, list) {
  const text = el.url.value.trim();
  if (seq !== suggestSeq || document.activeElement !== el.url || !text) return;

  // Первая строка — то, что произойдёт по Enter с набранным текстом.
  const looksLikeUrl = !text.includes(' ') && (text.includes('.') || text.includes('://'));
  const rows = [
    { url: text, title: looksLikeUrl ? 'Открыть адрес' : `Искать в ${engineName()}`, kind: looksLikeUrl ? 'globe' : 'search', typed: true },
    ...list.map((item) => ({ ...item, kind: item.kind === 'bookmark' ? 'star' : 'clock' })),
  ];
  // Список выезжает с анимацией только при первом появлении. Дальше, пока человек
  // печатает, строки меняются на месте, а новые лишь мягко проявляются — без мигания.
  const firstShow = !sheetOpen;
  const known = new Set(suggestRows.map((row) => row.url));
  suggestRows = rows;
  selected = -1;

  el.suggestions.replaceChildren(...rows.map((row, index) => {
    const motion = firstShow ? '' : row.typed || known.has(row.url) ? ' still' : ' appear';
    const node = h('div', { class: 'sug' + motion, style: `--i:${index}` },
      h('span', { class: row.kind === 'star' ? 'kind bookmark' : 'kind' }, icon(row.kind, 15)),
      h('span', { class: 't' }, row.typed ? text : row.title || prettyUrl(row.url)),
      h('span', { class: 'u' }, row.typed ? `— ${row.title}` : prettyUrl(row.url)),
    );
    // mousedown, а не click: иначе поле потеряет фокус раньше, чем сработает нажатие.
    node.addEventListener('mousedown', (event) => {
      event.preventDefault();
      go(row.url);
    });
    return node;
  }));

  if (vertical()) {
    const box = $('#urlbox').getBoundingClientRect();
    document.documentElement.style.setProperty('--sheet-top', Math.round(box.bottom + 6) + 'px');
  }
  el.sheet.classList.add('open');
  sheetOpen = true;
  // Размер окна интерфейса меняем, только если высота списка правда изменилась.
  const height = el.sheet.offsetHeight;
  if (!layerKind && height !== sheetHeight) {
    sheetHeight = height;
    send('overlay', { mode: 'sheet', height });
  }
}

function moveSelection(step) {
  if (!suggestRows.length) return;
  selected = (selected + step + suggestRows.length + 1) % (suggestRows.length + 1);
  if (selected === suggestRows.length) selected = -1;
  [...el.suggestions.children].forEach((node, index) => node.classList.toggle('sel', index === selected));
}

el.url.addEventListener('focus', () => {
  const tab = activeTab();
  if (tab) el.url.value = tab.url;
  el.url.select();
});
el.url.addEventListener('blur', () => {
  if (menuKind === 'input') return; // меню поля — набранный текст не трогаем
  closeSheet();
  const tab = activeTab();
  if (tab) el.url.value = prettyUrl(tab.url);
});
el.url.addEventListener('input', () => {
  const text = el.url.value.trim();
  if (!text) return closeSheet();
  send('suggest', { text, seq: ++suggestSeq });
});
el.url.addEventListener('keydown', (event) => {
  if (event.key === 'ArrowDown') { event.preventDefault(); moveSelection(1); }
  else if (event.key === 'ArrowUp') { event.preventDefault(); moveSelection(-1); }
  else if (event.key === 'Enter') {
    const text = selected >= 0 ? suggestRows[selected].url : el.url.value.trim();
    if (text) go(text);
  } else if (event.key === 'Escape') {
    event.stopPropagation();
    el.url.blur();
    send('focus_content');
  }
});

function focusUrl() {
  const target = activeTab()?.start && !layerKind ? el.startq : el.url;
  target.focus();
  target.select();
}

// ---------- стартовая страница ----------

let tilesKey = null;
let engineKey = null;

const engineBtn = $('#engine');
const engineMenu = $('#enginemenu');
engineBtn.prepend(icon('chevron', 14));

function closeEngineMenu() {
  engineMenu.classList.remove('open');
  engineBtn.classList.remove('on');
}

function renderEngines() {
  const key = S.search + '|' + JSON.stringify(S.engines);
  if (key === engineKey) return;
  engineKey = key;
  $('#enginename').textContent = engineName();
  el.startq.placeholder = `Искать в ${engineName()} или ввести адрес`;
  engineMenu.replaceChildren(...S.engines.map((engine, index) =>
    h('div', {
      class: 'item', style: `--i:${index}`,
      onclick: () => {
        send('set', { key: 'search', value: engine.id });
        closeEngineMenu();
        el.startq.focus();
      },
    }, h('span', { class: 'name' }, engine.name), engine.id === S.search && icon('check', 16))));
}

engineBtn.addEventListener('click', (event) => {
  event.stopPropagation();
  const open = engineMenu.classList.toggle('open');
  engineBtn.classList.toggle('on', open);
});
document.addEventListener('mousedown', (event) => {
  if (!event.target.closest('#enginemenu, #engine')) closeEngineMenu();
});
// Щелчок по самой рамке поиска ставит курсор в поле.
$('#searchbox').addEventListener('click', (event) => {
  if (event.target.id === 'searchbox') el.startq.focus();
});

// ----- обои -----

const WALLS = [
  ['aurora', 'Аврора'], ['sunset', 'Закат'], ['ocean', 'Океан'],
  ['forest', 'Лес'], ['night', 'Ночь'], ['mono', 'Без узора'],
  ['waves', 'Волны', true], ['stars', 'Звёзды', true], ['particles', 'Частицы', true], ['flow', 'Переливы', true],
];
// фон, на котором белый текст читается лучше тёмного
const CONTRAST_WALLS = ['image', 'video', 'stars', 'particles', 'flow'];

// ----- живые обои на canvas: звёзды и частицы -----
// Рисуем не чаще 30 раз в секунду и только пока новая вкладка на экране
// и человек что-то делает (или включено «двигаться всё время»).

const live = { kind: '', raf: 0, last: 0, items: [], w: 0, h: 0, shooting: null };
const canvas = $('#wallcanvas');
const ctx = canvas.getContext('2d');

function liveRunning() {
  const b = document.body.classList;
  return b.contains('start') && !document.hidden && S.settings.motion !== 'off'
    && (S.settings.wallpaperAlways || !b.contains('calm'));
}

function updateLive() {
  const on = liveRunning();
  document.body.classList.toggle('wall-always', !!S.settings.wallpaperAlways);
  const video = $('#wallvideo');
  if (live.kind === 'video' && video.src) { if (on) video.play().catch(() => {}); else video.pause(); }
  if ((live.kind === 'stars' || live.kind === 'particles') && on && !live.raf) {
    live.raf = requestAnimationFrame(drawLive);
  }
}

function resetLive() {
  const box = $('#start').getBoundingClientRect();
  const dpr = Math.min(devicePixelRatio || 1, 2);
  live.w = Math.max(1, box.width);
  live.h = Math.max(1, box.height);
  canvas.width = Math.round(live.w * dpr);
  canvas.height = Math.round(live.h * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const area = live.w * live.h;
  const rnd = Math.random;
  if (live.kind === 'stars') {
    live.items = Array.from({ length: Math.min(420, Math.round(area / 5000)) }, () => ({
      x: rnd() * live.w, y: rnd() * live.h, r: rnd() * 1.4 + 0.3, p: rnd() * 6.3, s: rnd() * 0.002 + 0.0006,
    }));
  } else {
    live.items = Array.from({ length: Math.min(90, Math.round(area / 15000)) }, () => ({
      x: rnd() * live.w, y: rnd() * live.h, vx: (rnd() - 0.5) * 0.35, vy: (rnd() - 0.5) * 0.35, r: rnd() * 1.8 + 1,
    }));
  }
}

function drawLive(t) {
  live.raf = 0;
  if (!liveRunning() || (live.kind !== 'stars' && live.kind !== 'particles')) return;
  live.raf = requestAnimationFrame(drawLive);
  if (t - live.last < 33) return;
  live.last = t;
  const box = $('#start').getBoundingClientRect();
  if (Math.abs(box.width - live.w) > 1 || Math.abs(box.height - live.h) > 1 || !live.items.length) resetLive();
  ctx.clearRect(0, 0, live.w, live.h);
  const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() || '#8288ff';
  if (live.kind === 'stars') {
    for (const s of live.items) {
      s.x -= s.r * 0.03;
      if (s.x < -2) s.x = live.w + 2;
      ctx.globalAlpha = 0.25 + 0.75 * (0.5 + 0.5 * Math.sin(s.p + t * s.s));
      ctx.fillStyle = '#fff';
      ctx.beginPath(); ctx.arc(s.x, s.y, s.r, 0, 6.283); ctx.fill();
    }
    // изредка падающая звезда
    if (!live.shooting && Math.random() < 0.004) {
      live.shooting = { x: Math.random() * live.w * 0.8 + live.w * 0.2, y: Math.random() * live.h * 0.4, life: 1 };
    }
    if (live.shooting) {
      const sh = live.shooting;
      const grad = ctx.createLinearGradient(sh.x, sh.y, sh.x + 90, sh.y - 45);
      grad.addColorStop(0, 'rgba(255,255,255,0.9)');
      grad.addColorStop(1, 'rgba(255,255,255,0)');
      ctx.globalAlpha = sh.life; ctx.strokeStyle = grad; ctx.lineWidth = 1.6;
      ctx.beginPath(); ctx.moveTo(sh.x, sh.y); ctx.lineTo(sh.x + 90, sh.y - 45); ctx.stroke();
      sh.x -= 9; sh.y += 4.5; sh.life -= 0.03;
      if (sh.life <= 0) live.shooting = null;
    }
  } else {
    const items = live.items;
    for (const p of items) {
      p.x += p.vx; p.y += p.vy;
      if (p.x < 0 || p.x > live.w) p.vx *= -1;
      if (p.y < 0 || p.y > live.h) p.vy *= -1;
    }
    ctx.strokeStyle = accent; ctx.lineWidth = 1;
    for (let i = 0; i < items.length; i++) {
      for (let j = i + 1; j < items.length; j++) {
        const dx = items[i].x - items[j].x, dy = items[i].y - items[j].y;
        const d2 = dx * dx + dy * dy;
        if (d2 < 17000) {
          ctx.globalAlpha = (1 - d2 / 17000) * 0.45;
          ctx.beginPath(); ctx.moveTo(items[i].x, items[i].y); ctx.lineTo(items[j].x, items[j].y); ctx.stroke();
        }
      }
    }
    ctx.fillStyle = accent; ctx.globalAlpha = 0.85;
    for (const p of items) { ctx.beginPath(); ctx.arc(p.x, p.y, p.r, 0, 6.283); ctx.fill(); }
  }
  ctx.globalAlpha = 1;
}

document.addEventListener('visibilitychange', updateLive);
let wallKey = null;

function renderWallpaper() {
  const s = S.settings;
  let kind = s.wallpaper || 'aurora';
  if ((kind === 'image' && !s.hasImage) || (kind === 'video' && !s.hasVideo)) kind = 'aurora';
  const key = kind + '|' + s.wallpaperUrl + '|' + s.videoUrl + '|' + s.wallpaperDim;
  if (key === wallKey) return updateLive();
  wallKey = key;
  document.body.dataset.wall = kind;
  document.body.classList.toggle('wall-contrast', CONTRAST_WALLS.includes(kind));
  live.kind = kind;
  live.items = [];
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const wall = $('#wall');
  const video = $('#wallvideo');
  wall.style.setProperty('--dim', (s.wallpaperDim ?? 35) / 100);
  if (kind !== 'video' && video.src) { video.pause(); video.removeAttribute('src'); video.load(); }
  if (kind === 'video') {
    wall.style.backgroundImage = '';
    video.oncanplay = () => wall.classList.add('show');
    video.src = s.videoUrl;
  } else if (kind === 'image') {
    // показываем фото, только когда оно загрузилось, — без мигания пустого фона
    const img = new Image();
    img.onload = () => {
      wall.style.backgroundImage = `url("${s.wallpaperUrl}")`;
      wall.classList.add('show');
    };
    img.src = s.wallpaperUrl;
  } else {
    wall.classList.remove('show');
    wall.style.backgroundImage = '';
  }
  updateLive();
}

$('#wallbtn').addEventListener('click', () => openHub('settings'));

function renderStart() {
  renderEngines();
  renderWallpaper();
  // в виде «Плитки» сайтов больше
  const limit = S.settings.newtabLayout === 'tiles' ? 24 : 12;
  const key = limit + JSON.stringify(S.bookmarks);
  if (key !== tilesKey) {
    tilesKey = key;
    const tiles = S.bookmarks.slice(0, limit).map((bookmark, index) =>
      h('div', {
        class: 'tile', style: `--i:${index}`, title: bookmark.url,
        onclick: () => send('open', { url: bookmark.url }),
      }, favicon(bookmark.url, true), h('span', { class: 'name' }, bookmark.title || hostOf(bookmark.url))));
    el.tiles.replaceChildren(...(tiles.length
      ? tiles
      : [h('div', { class: 'tiles-hint' }, 'Здесь появятся сайты из избранного — добавляйте их звёздочкой или Ctrl+D')]));
  }
  el.stat.replaceChildren(
    icon('shield', 14),
    S.adblock ? 'Заблокировано рекламы и трекеров: ' : 'Блокировка рекламы выключена',
    S.adblock ? h('b', {}, S.blockedTotal.toLocaleString('ru-RU')) : null,
  );
}

// Фон стартовой страницы двигается, пока есть активность, и замирает через 6 секунд покоя.
let calmTimer = 0;
function wake() {
  const wasCalm = document.body.classList.contains('calm');
  document.body.classList.remove('calm');
  clearTimeout(calmTimer);
  calmTimer = setTimeout(() => { document.body.classList.add('calm'); updateLive(); }, 6000);
  if (wasCalm) updateLive();
}
for (const type of ['mousemove', 'keydown', 'mousedown']) {
  document.addEventListener(type, wake, { passive: true });
}
wake();

function greetingFor(hour) {
  if (hour >= 5 && hour < 12) return 'Доброе утро';
  if (hour >= 12 && hour < 17) return 'Добрый день';
  if (hour >= 17 && hour < 23) return 'Добрый вечер';
  return 'Доброй ночи';
}

function tick() {
  const now = new Date();
  $('#greeting').textContent = greetingFor(now.getHours());
  el.clock.textContent = now.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
  el.date.textContent = now.toLocaleDateString('ru-RU', { weekday: 'long', day: 'numeric', month: 'long' });
}
// Раз в минуту, ровно в её начале: интерфейс почти не просыпается зря.
function scheduleTick() {
  tick();
  setTimeout(scheduleTick, 60000 - (Date.now() % 60000) + 50);
}
scheduleTick();

el.startq.addEventListener('keydown', (event) => {
  const text = el.startq.value.trim();
  if (event.key === 'Enter' && text) {
    el.startq.value = '';
    send('navigate', { text });
  }
});

// ---------- слой поверх страницы ----------
// Меню и библиотека рисуются над страницей. Rust растягивает интерфейс на всё окно
// и подкладывает снимок страницы; показываем слой, когда он скажет, что готово.

let layerKind = null; // null | 'hub' | 'menu'
let readyResolve = null;

function requestLayer(kind) {
  const already = layerKind !== null;
  layerKind = kind;
  if (sheetOpen) { sheetOpen = false; el.sheet.classList.remove('open'); }
  if (already) return Promise.resolve();
  return new Promise((resolve) => {
    readyResolve = resolve;
    send('overlay', { mode: 'full' });
  });
}

function setBackdrop(url) {
  el.backdrop.classList.toggle('show', !!url);
  if (url) el.backdrop.src = url;
  else el.backdrop.removeAttribute('src');
}

function overlayReady() {
  el.layer.classList.add('open');
  const resolve = readyResolve;
  readyResolve = null;
  if (resolve) resolve();
}

/** Убирает слой; Rust сжимает интерфейс обратно, когда анимация закончится. */
function releaseLayer(delay, refocus = true) {
  layerKind = null;
  setTimeout(() => {
    if (layerKind) return; // за это время открыли что-то новое
    el.layer.classList.remove('open', 'hub', 'closing');
    setBackdrop(null);
    send('overlay', { mode: 'none' });
    if (refocus) send('focus_content');
  }, delay);
}

/** Rust сменил вкладку под слоем — закрываем всё без анимации. */
function closeOverlays() {
  layerKind = null;
  readyResolve = null;
  hubSection = null;
  menuKind = null;
  el.layer.classList.remove('open', 'hub', 'closing');
  el.menu.classList.remove('open', 'closing');
  el.catcher.classList.remove('open');
  el.hubbtn.classList.remove('on');
  el.menubtn.classList.remove('on');
  setBackdrop(null);
}

// ---------- меню ----------

let menuKind = null; // null | 'main' | 'tab'
let menuItems = [];
let menuSel = -1;

function item({ icon: name, label, kb, onClick, disabled, check, danger, keep }) {
  const node = h('button', { class: 'mi' + (danger ? ' danger' : ''), disabled },
    h('span', { class: 'ic' }, name ? icon(name, 16) : null),
    h('span', { class: 'lbl' }, label),
    check ? h('span', { class: 'chk' }, icon('check', 15)) : null,
    kb ? h('span', { class: 'kb' }, kb) : null);
  node.addEventListener('click', () => {
    if (!keep) closeMenu();
    onClick();
  });
  return node;
}
const sep = () => h('div', { class: 'sep' });

async function showMenu(kind, nodes, place) {
  if (layerKind === 'hub') closeHub(true);
  menuKind = kind;
  el.menu.classList.remove('closing', 'open');
  el.menu.replaceChildren(...nodes);
  menuItems = nodes.filter((n) => n.classList?.contains('mi') && !n.disabled);
  menuSel = -1;
  await requestLayer('menu');
  if (menuKind !== kind) return;
  el.catcher.classList.add('open');
  el.menu.style.left = el.menu.style.top = el.menu.style.right = '';
  el.menu.classList.add('open');
  place(el.menu.getBoundingClientRect());
  el.menu.focus();
}

function closeMenu() {
  if (!menuKind) return;
  const kind = menuKind;
  menuKind = null;
  el.menubtn.classList.remove('on');
  el.catcher.classList.remove('open');
  el.menu.classList.add('closing');
  setTimeout(() => el.menu.classList.remove('open', 'closing'), 120);
  if (layerKind === 'menu') releaseLayer(120, kind !== 'input');
  if (kind === 'input' && menuInput) menuInput.focus();
}

el.catcher.addEventListener('mousedown', (event) => {
  event.preventDefault();
  closeMenu();
});
el.catcher.addEventListener('contextmenu', (event) => event.preventDefault());

function zoomRow() {
  const tab = activeTab();
  const disabled = !tab || tab.start;
  return h('div', { class: 'zoomrow' },
    h('span', { class: 'lbl' }, h('span', { class: 'ic' }, icon('zoom', 16)), 'Масштаб'),
    h('button', { class: 'icon-btn', title: 'Уменьшить (Ctrl+−)', disabled, onclick: () => send('zoom', { dir: 'out' }) }, icon('minus', 16)),
    h('span', { class: 'val', id: 'zoomval' }, `${Math.round((tab?.zoom || 1) * 100)}%`),
    h('button', { class: 'icon-btn', title: 'Увеличить (Ctrl++)', disabled, onclick: () => send('zoom', { dir: 'in' }) }, icon('plus', 16)),
    h('button', { class: 'icon-btn', title: 'Во весь экран (F11)', onclick: () => { closeMenu(); send('fullscreen'); } }, icon('expand', 16)));
}

function mainMenuNodes() {
  const tab = activeTab();
  const page = tab && !tab.start;
  return [
    item({ icon: 'plus', label: 'Новая вкладка', kb: 'Ctrl+T', onClick: () => send('new_tab') }),
    item({ icon: 'window', label: 'Новое окно', kb: 'Ctrl+N', onClick: () => send('new_window') }),
    item({ icon: 'mask', label: 'Новое окно InPrivate', kb: 'Ctrl+Shift+N', onClick: () => send('inprivate') }),
    sep(),
    zoomRow(),
    sep(),
    item({ icon: 'star', label: 'Избранное', kb: 'Ctrl+Shift+O', onClick: () => openHub('bookmarks') }),
    item({ icon: 'clock', label: 'Журнал', kb: 'Ctrl+H', onClick: () => openHub('history') }),
    item({ icon: 'download', label: 'Загрузки', kb: 'Ctrl+J', onClick: () => openHub('downloads') }),
    item({ icon: 'gauge', label: 'Диспетчер вкладок', kb: 'Shift+Esc', onClick: () => openHub('tasks') }),
    sep(),
    item({ icon: 'search', label: 'Найти на странице', kb: 'Ctrl+F', disabled: !page, onClick: () => setTimeout(openFind, 140) }),
    item({ icon: 'print', label: 'Печать', kb: 'Ctrl+P', disabled: !page, onClick: () => send('print') }),
    item({ icon: 'bar', label: 'Панель избранного', kb: 'Ctrl+Shift+B', check: S.settings.favbar,
      onClick: () => send('set', { key: 'favbar', value: !S.settings.favbar }) }),
    item({ icon: 'code', label: 'Инструменты разработчика', kb: 'F12', disabled: !page, onClick: () => send('devtools') }),
    sep(),
    item({ icon: 'gear', label: 'Параметры', onClick: () => openHub('settings') }),
  ];
}

function openMainMenu() {
  el.menubtn.classList.add('on');
  showMenu('main', mainMenuNodes(), (rect) => {
    const button = el.menubtn.getBoundingClientRect();
    el.menu.style.top = button.bottom + 6 + 'px';
    el.menu.style.left = Math.max(8, Math.min(button.right, innerWidth - 8) - rect.width) + 'px';
    el.menu.style.transformOrigin = 'top right';
  });
}

function refreshMainMenu() {
  const value = $('#zoomval');
  if (value) value.textContent = `${Math.round((activeTab()?.zoom || 1) * 100)}%`;
}

// ----- меню текстового поля -----

let menuInput = null;
let pendingPaste = null; // что сделать с текстом из буфера обмена, когда его пришлёт Rust

/** Пункты меню не забирают фокус у поля — иначе пропало бы выделение. */
function keepFocus(nodes) {
  for (const node of nodes) node.addEventListener('mousedown', (event) => event.preventDefault());
  return nodes;
}

function insertText(input, text) {
  const start = input.selectionStart ?? input.value.length;
  const end = input.selectionEnd ?? start;
  input.setRangeText(text.replace(/[\r\n]+/g, ' '), start, end, 'end');
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

function openInputMenu(input, x, y) {
  menuInput = input;
  input.focus();
  const hasSelection = input.selectionStart !== input.selectionEnd;
  const run = (command) => () => {
    input.focus();
    document.execCommand(command);
  };
  const nodes = keepFocus([
    item({ icon: 'smile', label: 'Эмодзи', kb: 'Win+Точка', onClick: () => { input.focus(); send('system_keys', { keys: 'emoji' }); } }),
    item({ icon: 'mic', label: 'Голосовой ввод', kb: 'Win+H', onClick: () => { input.focus(); send('system_keys', { keys: 'voice' }); } }),
    sep(),
    item({ icon: 'undo', label: 'Отменить', kb: 'Ctrl+Z', onClick: run('undo') }),
    item({ icon: 'redo', label: 'Вернуть', kb: 'Ctrl+Y', onClick: run('redo') }),
    sep(),
    item({ icon: 'cut', label: 'Вырезать', kb: 'Ctrl+X', disabled: !hasSelection, onClick: run('cut') }),
    item({ icon: 'copy', label: 'Копировать', kb: 'Ctrl+C', disabled: !hasSelection, onClick: run('copy') }),
    item({ icon: 'paste', label: 'Вставить', kb: 'Ctrl+V', onClick: () => {
      pendingPaste = (text) => { input.focus(); insertText(input, text); };
      send('clipboard');
    } }),
    input === el.url && item({ icon: 'pastego', label: 'Вставить и перейти', kb: 'Ctrl+Shift+L', onClick: () => send('paste_go') }),
    item({ icon: 'trash', label: 'Удалить', disabled: !hasSelection, onClick: run('delete') }),
    sep(),
    item({ icon: 'selectall', label: 'Выбрать все', kb: 'Ctrl+A', disabled: !input.value, onClick: () => { input.focus(); input.select(); } }),
  ].filter(Boolean));
  showMenu('input', nodes, (rect) => {
    el.menu.style.left = Math.min(x, innerWidth - rect.width - 8) + 'px';
    el.menu.style.top = Math.min(y, innerHeight - rect.height - 8) + 'px';
    el.menu.style.transformOrigin = 'top left';
  });
}

function pasteText(text) {
  const action = pendingPaste;
  pendingPaste = null;
  if (action && text) action(text);
}

function openTabMenu(id, x, y) {
  const tab = S.tabs.find((t) => t.id === id);
  if (!tab) return;
  const index = S.tabs.indexOf(tab);
  const nodes = [
    item({ icon: 'plus', label: 'Новая вкладка', kb: 'Ctrl+T', onClick: () => send('new_tab') }),
    sep(),
    item({ icon: 'reload', label: 'Обновить', disabled: tab.start, onClick: () => send('reload_tab', { id }) }),
    item({ icon: 'copy', label: 'Дублировать', disabled: tab.start, onClick: () => send('duplicate', { id }) }),
    item({ icon: 'pin', label: tab.pinned ? 'Открепить вкладку' : 'Закрепить вкладку', onClick: () => send('pin', { id }) }),
    item({ icon: 'window', label: 'Переместить в новое окно', disabled: tab.start, onClick: () => send('detach', { id }) }),
    item({ icon: 'moon', label: 'Выгрузить из памяти', disabled: tab.start || tab.id === S.active,
      onClick: () => send('unload_tab', { id }) }),
    item({ icon: tab.muted ? 'speaker' : 'muted', label: tab.muted ? 'Включить звук' : 'Выключить звук вкладки',
      disabled: tab.start, onClick: () => send('mute', { id }) }),
    sep(),
    item({ icon: 'x', label: 'Закрыть другие вкладки', disabled: S.tabs.length < 2, onClick: () => send('close_others', { id }) }),
    item({ icon: 'x', label: 'Закрыть вкладки справа', disabled: index === S.tabs.length - 1, onClick: () => send('close_right', { id }) }),
    item({ icon: 'x', label: 'Закрыть', kb: 'Ctrl+W', danger: true, onClick: () => send('close_tab', { id }) }),
  ];
  showMenu('tab', nodes, (rect) => {
    el.menu.style.left = Math.min(x, innerWidth - rect.width - 8) + 'px';
    el.menu.style.top = Math.min(y, innerHeight - rect.height - 8) + 'px';
    el.menu.style.transformOrigin = 'top left';
  });
}

// ---------- библиотека: избранное, журнал, загрузки, параметры ----------

const SECTIONS = {
  bookmarks: { title: 'Избранное', icon: 'star', empty: 'В избранном пока пусто', action: '' },
  history: { title: 'Журнал', icon: 'clock', empty: 'Журнал пуст', action: 'Очистить журнал' },
  downloads: { title: 'Загрузки', icon: 'download', empty: 'Загрузок пока нет', action: 'Очистить список' },
  tasks: { title: 'Диспетчер вкладок', icon: 'gauge', empty: '', action: 'Выгрузить фоновые' },
  settings: { title: 'Параметры', icon: 'gear', empty: '', action: '' },
};

let hubSection = null;
let tasksData = null;
let tasksTimer = 0;
let historyItems = [];
let hubKey = null;

async function openHub(section) {
  if (hubSection === section && layerKind === 'hub') return closeHub();
  if (menuKind) {
    menuKind = null;
    el.menubtn.classList.remove('on');
    el.catcher.classList.remove('open');
    el.menu.classList.remove('open', 'closing');
  }
  closeFind();
  const wasOpen = layerKind === 'hub';
  hubSection = section;
  closeSheet();

  el.hubbtn.classList.add('on');
  el.hubq.value = '';
  resetHubAction();
  el.hubh1.textContent = SECTIONS[section].title;
  el.hubaction.textContent = SECTIONS[section].action;
  el.hub.classList.toggle('no-search', section === 'settings');
  el.hubind.style.setProperty('--pos', Object.keys(SECTIONS).indexOf(section));
  for (const button of document.querySelectorAll('#hubnav button')) {
    button.classList.toggle('on', button.dataset.sec === section);
  }

  hubKey = null;
  clearInterval(tasksTimer);
  if (section === 'history') {
    historyItems = [];
    el.hublist.replaceChildren();
    send('history', { q: '' });
  } else if (section === 'tasks') {
    // память меняется — обновляем список, пока диспетчер открыт
    tasksData = null;
    el.hublist.replaceChildren();
    send('tasks');
    tasksTimer = setInterval(() => { if (hubSection === 'tasks') send('tasks'); }, 2000);
  } else {
    renderHubList(true);
  }
  if (wasOpen) return;
  // Если слой уже был открыт под меню, сразу переключаем его на библиотеку.
  layerKind = null;
  await requestLayer('hub');
  if (layerKind !== 'hub') return;
  el.layer.classList.remove('closing');
  el.layer.classList.add('hub');
}

function closeHub(instant = false) {
  if (!hubSection) return;
  clearInterval(tasksTimer);
  hubSection = null;
  el.hubbtn.classList.remove('on');
  if (instant) {
    el.layer.classList.remove('hub', 'closing');
    return;
  }
  el.layer.classList.add('closing');
  releaseLayer(170);
}

function setHistory(items) {
  historyItems = items;
  if (hubSection === 'history') renderHubList(true);
}

function emptyState(section) {
  const info = SECTIONS[section];
  const text = el.hubq.value.trim() ? 'Ничего не найдено' : info.empty;
  return h('div', { class: 'empty' }, icon(info.icon, 56), text);
}

function actionButton(name, title, handler) {
  return h('button', {
    class: 'icon-btn', title,
    onclick: (event) => { event.stopPropagation(); handler(event); },
  }, icon(name, 15));
}

/** Строка исчезает с анимацией, затем выполняется удаление. */
function removeRow(node, after) {
  node.classList.add('gone');
  setTimeout(() => { node.remove(); after(); }, 190);
}

function openFromHub(url, event) {
  const background = event.ctrlKey || event.button === 1;
  send('open', { url, background, newTab: !background && event.shiftKey });
  if (!background) closeHub();
}

function dayLabel(seconds) {
  const date = new Date(seconds * 1000);
  const now = new Date();
  const midnight = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((midnight(now) - midnight(date)) / 864e5);
  if (days === 0) return 'Сегодня';
  if (days === 1) return 'Вчера';
  return date.toLocaleDateString('ru-RU', {
    day: 'numeric', month: 'long',
    year: date.getFullYear() === now.getFullYear() ? undefined : 'numeric',
  });
}

function historyRows() {
  const nodes = [];
  let lastDay = '';
  historyItems.forEach((visit, index) => {
    const day = dayLabel(visit.time);
    if (day !== lastDay) {
      lastDay = day;
      nodes.push(h('div', { class: 'day' }, day));
    }
    const time = new Date(visit.time * 1000).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' });
    const node = h('div', {
      class: 'row', style: `--i:${index}`,
      onclick: (event) => openFromHub(visit.url, event),
      onauxclick: (event) => { if (event.button === 1) openFromHub(visit.url, event); },
    },
      h('span', { class: 'time' }, time),
      favicon(visit.url),
      h('span', { class: 't' }, visit.title || prettyUrl(visit.url)),
      h('span', { class: 'u' }, hostOf(visit.url)),
      h('span', { class: 'acts' }, actionButton('trash', 'Удалить из журнала', () => {
        historyItems = historyItems.filter((item) => item !== visit);
        removeRow(node, () => send('history_remove', { url: visit.url, time: visit.time }));
      })),
    );
    nodes.push(node);
  });
  return nodes;
}

function bookmarkRows() {
  const query = el.hubq.value.trim().toLowerCase();
  return S.bookmarks
    .filter((b) => !query || b.title.toLowerCase().includes(query) || b.url.toLowerCase().includes(query))
    .map((bookmark, index) => {
      const node = h('div', {
        class: 'row', style: `--i:${index}`,
        onclick: (event) => openFromHub(bookmark.url, event),
        onauxclick: (event) => { if (event.button === 1) openFromHub(bookmark.url, event); },
      },
        favicon(bookmark.url),
        h('span', { class: 't' }, bookmark.title || prettyUrl(bookmark.url)),
        h('span', { class: 'u' }, prettyUrl(bookmark.url)),
        h('span', { class: 'acts' }, actionButton('trash', 'Удалить из избранного', () => {
          removeRow(node, () => send('bookmark_remove', { url: bookmark.url }));
        })),
      );
      return node;
    });
}

function downloadRows() {
  const query = el.hubq.value.trim().toLowerCase();
  const control = (d, action) => () => send('download_control', { id: d.id, action });
  return [...S.downloads].reverse()
    .filter((d) => !query || d.name.toLowerCase().includes(query))
    .map((d, index) => {
      const known = d.total > 0;
      const size = known ? `${formatBytes(d.received)} из ${formatBytes(d.total)}` : `${formatBytes(d.received)}…`;
      const meta =
        d.state === 'done' ? `${formatBytes(d.received)} · ${hostOf(d.url)}`
        : d.state === 'failed' ? 'Загрузка не удалась'
        : d.state === 'canceled' ? 'Загрузка отменена'
        : d.state === 'paused' ? `Пауза · ${size}`
        : size;
      const running = d.state === 'progress' || d.state === 'paused';
      const bad = d.state === 'failed' || d.state === 'canceled';
      const acts =
        d.state === 'done' ? [
          actionButton('open', 'Открыть файл', () => send('download_open', { id: d.id })),
          actionButton('folder', 'Показать в папке', () => send('download_reveal', { id: d.id })),
        ]
        : d.state === 'progress' ? [
          actionButton('pause', 'Пауза', control(d, 'pause')),
          actionButton('x', 'Отменить', control(d, 'cancel')),
        ]
        : d.state === 'paused' ? [
          actionButton('play', 'Продолжить', control(d, 'resume')),
          actionButton('x', 'Отменить', control(d, 'cancel')),
        ]
        : [actionButton('trash', 'Убрать из списка', () => send('download_forget', { id: d.id }))];
      return h('div', { class: `row dl ${d.state}${running ? ' running' : ''}`, style: `--i:${index}` },
        h('span', { class: 'fileicon' }, icon(bad ? 'alert' : 'file', 18)),
        h('div', { class: 'body' },
          h('div', { class: 'name' }, d.name),
          h('div', { class: 'meta' }, meta),
          running && h('div', { class: (known ? 'bar' : 'bar unknown') + (d.state === 'paused' ? ' paused' : '') },
            h('i', { style: known ? `--pct:${(100 * d.received / d.total).toFixed(1)}%` : '' })),
        ),
        h('span', { class: 'acts' }, ...acts),
      );
    });
}

// ----- параметры -----
// Страница строится один раз при открытии, а при каждом обновлении состояния
// только обновляет значения — так не сбиваются анимации переключателей.

let settingsUpdaters = [];

function segmented(options, get, onPick) {
  const pill = h('i', { class: 'pill' });
  const buttons = options.map(([value, label]) =>
    h('button', { onclick: () => onPick(value) }, label));
  const root = h('div', { class: 'seg' }, pill, ...buttons);
  const update = () => {
    const current = get();
    buttons.forEach((button, i) => {
      const on = options[i][0] === current;
      button.classList.toggle('on', on);
      if (on && button.offsetWidth) {
        pill.style.width = button.offsetWidth + 'px';
        pill.style.transform = `translateX(${button.offsetLeft - 3}px)`;
      }
    });
  };
  settingsUpdaters.push(update);
  return root;
}

function toggle(get, onChange) {
  const input = h('input', { type: 'checkbox' });
  input.addEventListener('change', () => onChange(input.checked));
  settingsUpdaters.push(() => { input.checked = !!get(); });
  return h('label', { class: 'switch' }, input, h('i'));
}

function setting(label, hint, ...controls) {
  const small = h('small', {}, typeof hint === 'function' ? '' : hint);
  if (typeof hint === 'function') settingsUpdaters.push(() => { small.textContent = hint(); });
  return h('div', { class: 'set' },
    h('div', { class: 'label' }, label, hint ? small : null),
    h('div', { class: 'controls' }, ...controls));
}

function confirmButton(label, confirmLabel, onConfirm) {
  const button = h('button', { class: 'text-btn outline' }, label);
  let armed = false;
  button.addEventListener('click', () => {
    if (armed) {
      armed = false;
      button.classList.remove('danger');
      button.textContent = label;
      onConfirm();
      return;
    }
    armed = true;
    button.classList.add('danger');
    button.textContent = confirmLabel;
    setTimeout(() => {
      if (!armed) return;
      armed = false;
      button.classList.remove('danger');
      button.textContent = label;
    }, 3000);
  });
  return button;
}

const set = (key, value) => send('set', { key, value });

function wallpaperSetting() {
  const tiles = WALLS.map(([id, name, liveWall]) =>
    h('button', { class: `walltile w-${id}`, title: name, onclick: () => set('wallpaper', id) },
      liveWall && h('em', {}, 'живые'), h('span', {}, name)));
  const video = h('button', { class: 'walltile w-video', title: 'Своё видео (mp4, webm)', onclick: () => send('pick_wallpaper_video') },
    h('em', {}, 'видео'), h('span', {}, 'Своё видео'));
  const always = setting('Живые обои двигаются всё время', 'Иначе замирают через 6 секунд без движения мыши — так меньше расход',
    toggle(() => S.settings.wallpaperAlways, (value) => set('wallpaper_always', value)));
  const videoRow = setting('Своё видео', 'Звук выключен, видео повторяется по кругу',
    h('button', { class: 'text-btn outline', onclick: () => send('pick_wallpaper_video') }, 'Заменить'),
    h('button', { class: 'text-btn outline', onclick: () => send('remove_wallpaper_video') }, 'Убрать видео'));
  const photo = h('button', { class: 'walltile w-photo', title: 'Своё фото', onclick: () => send('pick_wallpaper') },
    h('i'), h('span', {}, 'Своё фото'));
  const dim = h('input', { type: 'range', min: 0, max: 85, step: 5, class: 'range' });
  dim.addEventListener('input', () => set('wallpaper_dim', Number(dim.value)));
  const dimRow = setting('Затемнение фото', 'Чтобы часы и поиск хорошо читались',
    dim, h('button', { class: 'text-btn outline', onclick: () => send('remove_wallpaper') }, 'Убрать фото'));
  settingsUpdaters.push(() => {
    const s = S.settings;
    const current = s.wallpaper === 'image' && s.hasImage ? 'image' : s.wallpaper;
    tiles.forEach((tile, i) => tile.classList.toggle('on', WALLS[i][0] === current));
    video.classList.toggle('on', s.wallpaper === 'video' && s.hasVideo);
    videoRow.style.display = s.hasVideo ? '' : 'none';
    photo.classList.toggle('on', current === 'image');
    photo.querySelector('i').style.backgroundImage = s.hasImage ? `url("${s.wallpaperUrl}")` : '';
    photo.querySelector('span').textContent = s.hasImage ? 'Своё фото' : 'Загрузить фото';
    dim.value = s.wallpaperDim ?? 35;
    dimRow.style.display = s.hasImage ? '' : 'none';
  });
  return h('div', {},
    h('div', { class: 'set wallset' },
      h('div', { class: 'label' }, 'Обои новой вкладки', h('small', {}, 'Готовый узор, живые обои, своё фото, GIF или видео')),
      h('div', { class: 'walls' }, ...tiles, photo, video)),
    dimRow, videoRow, always);
}

/** Кнопка настроек uBlock Origin Lite: его собственная страница во вкладке. */
function ubolButton() {
  const button = h('button', {
    class: 'text-btn outline',
    onclick: () => {
      send('open', { url: `chrome-extension://${S.ubolId}/dashboard.html`, newTab: true });
      closeHub();
    },
  }, 'Фильтры');
  settingsUpdaters.push(() => { button.style.display = S.ubolId ? '' : 'none'; });
  return button;
}

function importCard() {
  const sources = S.importSources || [];
  if (!sources.length) return null;
  return h('div', { class: 'card' }, h('h2', {}, 'Избранное'),
    setting('Перенести избранное', 'Папки становятся общим списком, повторы пропускаются. Пароли не переносятся — браузеры хранят их зашифрованными',
      ...sources.map(([id, name]) => h('button', {
        class: 'text-btn outline',
        onclick: () => send('import_bookmarks', { source: id }),
      }, `Из ${name}`))));
}

function defaultBrowserSetting() {
  const button = h('button', {
    class: 'text-btn primary',
    onclick: () => {
      send('make_default');
      toast('В Параметрах Windows выберите Rust Browser для ссылок');
    },
  }, 'Сделать основным');
  settingsUpdaters.push(() => { button.style.display = S.isDefault ? 'none' : ''; });
  return setting('Браузер по умолчанию',
    () => S.isDefault
      ? 'Ссылки из других программ открываются в Rust Browser'
      : 'Сейчас ссылки из Telegram и других программ открывает другой браузер',
    button);
}

function settingsPage() {
  settingsUpdaters = [];
  const cards = [
    h('div', { class: 'card' }, h('h2', {}, 'Оформление'),
      setting('Тема', 'Как в Windows или всегда светлая/тёмная',
        segmented([['system', 'Как в системе'], ['light', 'Светлая'], ['dark', 'Тёмная']],
          () => S.settings.theme, (value) => set('theme', value))),
      colorSetting(),
      setting('Подкрасить рамку окна', 'Полоса вкладок и панель слегка окрашиваются в цвет браузера',
        toggle(() => S.settings.tintFrame, (value) => set('tint_frame', value))),
      setting('Вид панели', 'Классика, всё в одну строку, парящая панель или вкладки слева',
        segmented([['classic', 'Классика'], ['compact', 'Компакт'], ['floating', 'Плавающая'], ['vertical', 'Вертикальные вкладки']],
          () => S.settings.barStyle, (value) => set('bar_style', value))),
      setting('Новая вкладка', 'Что показывать на новой вкладке',
        segmented([['clock', 'Часы и поиск'], ['greeting', 'Приветствие'], ['tiles', 'Плитки'], ['minimal', 'Минимализм']],
          () => S.settings.newtabLayout, (value) => set('newtab_layout', value))),
      setting('Анимации', 'Как быстро и с каким характером всё появляется',
        segmented([['smooth', 'Плавные'], ['fast', 'Быстрые'], ['bouncy', 'Пружинистые'], ['off', 'Без анимаций']],
          () => S.settings.motion, (value) => set('motion', value))),
      setting('Панель избранного', 'Сайты из избранного под адресной строкой · Ctrl+Shift+B',
        toggle(() => S.settings.favbar, (value) => set('favbar', value))),
      wallpaperSetting()),

    importCard(),

    h('div', { class: 'card' }, h('h2', {}, 'Поиск'),
      setting('Поисковая система', 'Для адресной строки и строки поиска на новой вкладке',
        segmented(S.engines.map((e) => [e.id, e.name]), () => S.search, (value) => set('search', value)))),

    h('div', { class: 'card' }, h('h2', {}, 'Запуск'),
      defaultBrowserSetting(),
      setting('При запуске', BOOT.inprivate ? 'В окне InPrivate вкладки не сохраняются' : 'Что открывать при старте браузера',
        segmented([[true, 'Вкладки прошлого сеанса'], [false, 'Новую вкладку']],
          () => S.settings.restoreSession, (value) => set('restore_session', value)))),

    h('div', { class: 'card' }, h('h2', {}, 'Производительность'),
      setting('Усыплять неактивные вкладки', 'Через это время фоновая вкладка засыпает, а через втрое больше выгружается из памяти',
        segmented([[5, '5 мин'], [15, '15 мин'], [30, '30 мин'], [60, '1 ч'], [0, 'Никогда']],
          () => S.settings.sleepMinutes, (value) => set('sleep_minutes', value))),
      setting('Режим эффективности для фоновых вкладок', 'Windows даёт их процессам меньше ресурсов, как в Edge. Вкладку на экране и со звуком не трогаем',
        toggle(() => S.settings.efficiencyMode, (value) => set('efficiency_mode', value))),
      setting('Экономия при работе от батареи', () => S.onBattery
          ? 'Сейчас от батареи: фоновые вкладки засыпают через 5 минут'
          : 'От батареи фоновые вкладки будут засыпать через 5 минут',
        toggle(() => S.settings.batterySaver, (value) => set('battery_saver', value))),
      setting('Блокировка рекламы и трекеров',
        () => S.ubolId
          ? 'uBlock Origin Lite: фильтры uBlock, EasyList, EasyPrivacy и другие'
          : `Около 3 600 рекламных доменов · заблокировано запросов: ${S.blockedTotal.toLocaleString('ru-RU')}`,
        ubolButton(),
        toggle(() => S.adblock, (value) => set('adblock', value)))),

    h('div', { class: 'card' }, h('h2', {}, 'Загрузки'),
      setting('Папка для загрузок', () => S.settings.downloadDir || `${S.settings.defaultDownloadDir} (по умолчанию)`,
        h('button', { class: 'text-btn outline', onclick: () => send('open_download_dir') }, 'Открыть'),
        h('button', { class: 'text-btn outline', onclick: () => send('reset_download_dir') }, 'По умолчанию'),
        h('button', { class: 'text-btn primary', onclick: () => send('pick_download_dir') }, 'Изменить'))),

    h('div', { class: 'card' }, h('h2', {}, 'Конфиденциальность'),
      setting('Предлагать сохранять пароли', 'Пароли хранятся в профиле браузера в зашифрованном виде',
        toggle(() => S.settings.passwords, (value) => set('passwords', value))),
      setting('Журнал посещений', 'Все страницы из раздела «Журнал»',
        confirmButton('Очистить', 'Точно очистить?', () => { send('history_clear'); toast('Журнал очищен'); })),
      setting('Данные сайтов', 'Файлы cookie, кэш и сохранённые пароли — вы выйдете из аккаунтов',
        confirmButton('Удалить', 'Точно удалить?', () => send('clear_data')))),

    h('div', { class: 'card' }, h('h2', {}, 'Сочетания клавиш'),
      h('div', { class: 'keys' }, ...[
        ['Новая вкладка / закрыть', 'Ctrl+T / Ctrl+W'], ['Вернуть закрытую вкладку', 'Ctrl+Shift+T'],
        ['Новое окно / окно InPrivate', 'Ctrl+N / Ctrl+Shift+N'], ['Следующая / предыдущая вкладка', 'Ctrl+Tab / Ctrl+Shift+Tab'],
        ['Адресная строка', 'Ctrl+L, Alt+D'], ['Найти на странице', 'Ctrl+F'],
        ['Масштаб', 'Ctrl + / Ctrl − / Ctrl+0'], ['Во весь экран', 'F11'],
        ['Избранное / журнал / загрузки', 'Ctrl+Shift+O / Ctrl+H / Ctrl+J'],
        ['Диспетчер вкладок', 'Shift+Esc'], ['Меню', 'Alt+F'],
      ].flatMap(([what, keys]) => [h('span', {}, what), h('kbd', {}, keys)]))),
  ];
  // значения выставляем после вставки в документ, когда известны размеры кнопок
  requestAnimationFrame(() => settingsUpdaters.forEach((update) => update()));
  return cards.filter(Boolean).map((card, i) => { card.style.setProperty('--i', i); return card; });
}

// ----- диспетчер вкладок -----

const MB = (bytes) => `${Math.round(bytes / 1048576)} МБ`;
const TASK_STATES = {
  active: 'На экране', loaded: 'Работает', sleeping: 'Спит', unloaded: 'Выгружена', start: 'Новая вкладка',
};

function showTasks(data) {
  tasksData = data;
  if (hubSection === 'tasks') renderHubList(!el.hublist.children.length);
}

function taskRows() {
  if (!tasksData) return [h('div', { class: 'empty' }, icon('gauge', 56), 'Считаю память…')];
  const query = el.hubq.value.trim().toLowerCase();
  const d = tasksData;
  const summary = h('div', { class: 'tasksum' },
    h('div', {}, h('b', {}, MB(d.total)), h('small', {}, 'занимает браузер')),
    h('div', {}, h('b', {}, MB(d.other)), h('small', {}, 'интерфейс и служебные процессы')),
    h('div', {}, h('b', {}, d.rows.filter((r) => r.state === 'active' || r.state === 'loaded' || r.state === 'sleeping').length + ' из ' + d.rows.length),
      h('small', {}, 'вкладок в памяти')),
    d.onBattery && h('div', { class: 'battery' }, icon('bolt', 15), `От батареи: вкладки засыпают через ${d.sleepLimit} мин`));
  const rows = d.rows
    .filter((r) => !query || (r.title + r.url).toLowerCase().includes(query))
    .map((r, index) => h('div', { class: 'row task', style: `--i:${index}`, onclick: () => { send('activate', { id: r.id }); closeHub(); } },
      r.state === 'start' ? h('span', { class: 'fav' }, icon('search', 14)) : favicon(r.url, false, r.favicon),
      h('span', { class: 't' }, r.title || (r.state === 'start' ? 'Новая вкладка' : prettyUrl(r.url))),
      h('span', { class: `state s-${r.state}` }, TASK_STATES[r.state]),
      r.eco && h('span', { class: 'state s-eco', title: 'Режим эффективности Windows: процесс получает меньше ресурсов' }, icon('leaf', 12), 'эко'),
      h('span', { class: 'mem' }, r.memory ? MB(r.memory) + (r.shared ? '*' : '') : '—'),
      h('span', { class: 'acts' }, (r.state === 'loaded' || r.state === 'sleeping')
        ? actionButton('moon', 'Выгрузить из памяти', () => send('unload_tab', { id: r.id }))
        : h('i', { class: 'spacer' }))));
  const shared = d.rows.some((r) => r.shared);
  return [summary, ...rows, shared && h('div', { class: 'tasknote' }, '* общий процесс с другой вкладкой того же сайта')];
}

function renderHubList(animate) {
  if (!hubSection) return;
  if (hubSection === 'tasks') {
    const key = 'tasks|' + el.hubq.value + '|' + JSON.stringify(tasksData);
    if (key === hubKey) return;
    hubKey = key;
    el.hublist.classList.toggle('stagger', animate);
    el.hublist.replaceChildren(...taskRows().filter(Boolean));
    return;
  }
  if (hubSection === 'settings') {
    if (hubKey === 'settings') {
      settingsUpdaters.forEach((update) => update());
      return;
    }
    hubKey = 'settings';
    el.hublist.classList.toggle('stagger', animate);
    el.hublist.replaceChildren(...settingsPage());
    el.hublist.scrollTop = 0;
    return;
  }
  // Без изменений в данных список не трогаем — иначе анимации дёргались бы на каждое обновление.
  const data = hubSection === 'history' ? historyItems : hubSection === 'bookmarks' ? S.bookmarks : S.downloads;
  const key = hubSection + '|' + el.hubq.value + '|' + JSON.stringify(data);
  if (key === hubKey) return;
  hubKey = key;

  const rows =
    hubSection === 'history' ? historyRows()
    : hubSection === 'bookmarks' ? bookmarkRows()
    : downloadRows();
  el.hublist.classList.toggle('stagger', animate);
  el.hublist.replaceChildren(...(rows.length ? rows : [emptyState(hubSection)]));
  if (animate) el.hublist.scrollTop = 0;
}

let searchTimer = 0;
el.hubq.addEventListener('input', () => {
  if (hubSection === 'history') {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => send('history', { q: el.hubq.value }), 140);
  } else {
    renderHubList(true);
  }
});

// Очистка журнала подтверждается вторым нажатием.
let armed = false;
function resetHubAction() {
  armed = false;
  el.hubaction.classList.remove('danger');
}
el.hubaction.addEventListener('click', () => {
  if (hubSection === 'downloads') return send('downloads_clear');
  if (hubSection === 'tasks') return send('unload_background');
  if (hubSection !== 'history') return;
  if (!armed) {
    armed = true;
    el.hubaction.classList.add('danger');
    el.hubaction.textContent = 'Точно очистить?';
    setTimeout(() => {
      if (!armed) return;
      resetHubAction();
      if (hubSection) el.hubaction.textContent = SECTIONS[hubSection].action;
    }, 3000);
  } else {
    resetHubAction();
    el.hubaction.textContent = SECTIONS.history.action;
    send('history_clear');
  }
});

for (const button of document.querySelectorAll('#hubnav button')) {
  button.addEventListener('click', () => {
    if (hubSection !== button.dataset.sec) openHub(button.dataset.sec);
  });
}
$('#hubclose').addEventListener('click', () => closeHub());
$('#scrim').addEventListener('mousedown', () => closeHub());

// ---------- клавиатура ----------

document.addEventListener('keydown', (event) => {
  if (menuKind) {
    if (event.key === 'Escape') { closeMenu(); return; }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const step = event.key === 'ArrowDown' ? 1 : -1;
      menuSel = (menuSel + step + menuItems.length) % menuItems.length;
      menuItems.forEach((node, i) => node.classList.toggle('sel', i === menuSel));
      return;
    }
    if (event.key === 'Enter' && menuSel >= 0) { menuItems[menuSel].click(); return; }
  }
  if (event.key !== 'Escape') return;
  if (engineMenu.classList.contains('open')) closeEngineMenu();
  else if (finding) closeFind();
  else if (hubSection) closeHub();
  else if (activeTab()?.loading) send('stop');
});
window.addEventListener('contextmenu', (event) => {
  event.preventDefault();
  const input = event.target.closest('input[type=text]');
  if (input) openInputMenu(input, event.clientX, event.clientY);
});

// ---------- вызовы из Rust ----------

window.ui = {
  setState,
  setSuggestions,
  setHistory,
  setBackdrop,
  overlayReady,
  closeOverlays,
  focusUrl,
  openHub,
  openMenu: () => (menuKind === 'main' ? closeMenu() : openMainMenu()),
  openFind,
  closeFind: () => closeFind(false),
  setFind,
  zoomChanged: refreshMainMenu,
  downloadStarted: () => replay(el.downloads, 'bounce'),
  dataCleared: () => toast('Данные сайтов удалены'),
  pasteText,
  showSiteInfo,
  showTasks,
  cookiesCleared: (count) => { closeMenu(); toast(`Удалено файлов cookie: ${count}`); },
  imported: (count) => toast(count ? `Перенесено в избранное: ${count}` : 'Новых сайтов для избранного не нашлось'),
};

send('ready');
