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
  sidebar: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M15 4v16"/><path d="M17.5 8h1M17.5 11h1"/>',
  window: '<rect x="3" y="5" width="18" height="15" rx="2"/><path d="M3 9h18"/>',
  image: '<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="10" r="2"/><path d="m21 16-5-5-9 9"/>',
  smile: '<circle cx="12" cy="12" r="9.5"/><path d="M8 14.5a5 5 0 0 0 8 0"/><path d="M9 9.5h.01M15 9.5h.01" stroke-width="2.6"/>',
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0"/><path d="M12 17.5V21"/>',
  undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>',
  redo: '<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>',
  key: '<circle cx="7.5" cy="15.5" r="4.5"/><path d="m10.7 12.3 9.3-9.3"/><path d="m16 7 3 3"/><path d="m14 9 2 2"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>',
  eyeoff: '<path d="M10.6 5.1A10 10 0 0 1 12 5c6.5 0 10 7 10 7a17 17 0 0 1-2.2 3.1"/><path d="M6.6 6.6C3.7 8.4 2 12 2 12s3.5 7 10 7a9.7 9.7 0 0 0 5.4-1.6"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/><path d="m3 3 18 18"/>',
  puzzle: '<path d="M10 3.5a2 2 0 0 1 4 0V6h4a1 1 0 0 1 1 1v4h-2.5a2 2 0 0 0 0 4H19v4a1 1 0 0 1-1 1h-4v-2.5a2 2 0 0 0-4 0V20H6a1 1 0 0 1-1-1v-4h2.5a2 2 0 0 0 0-4H5V7a1 1 0 0 1 1-1h4z"/>',
  translate: '<path d="M4 5h8"/><path d="M8 3v2"/><path d="M5.5 5c.8 3 3.3 5.6 6 6.5"/><path d="M10.5 5c-.7 3.2-3 6-6.5 7.5"/><path d="m12 20 4-9 4 9"/><path d="M13.4 17h5.2"/>',
  pip: '<rect x="2.5" y="4.5" width="19" height="15" rx="2.5"/><rect x="12" y="11.5" width="7" height="5" rx="1" fill="currentColor"/>',
  moon2: '<path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/>',
  card: '<rect x="2.5" y="5" width="19" height="14" rx="2.5"/><path d="M2.5 10h19"/><path d="M6 15h4"/>',
  home: '<path d="M3 11 12 4l9 7"/><path d="M5 10v10h14V10"/><path d="M10 20v-6h4v6"/>',
  split: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M12 4v16"/>',
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
// растёт, когда браузер нашёл новый значок: плитки перерисуются с ним
let iconsVersion = 0;

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
  // Крупный значок браузер находит сам (apple-touch-icon и т. п.) и хранит у себя;
  // не нашёл — пробуем обычный favicon.ico.
  const fallback = known || parsed.origin + '/favicon.ico';
  if (large && !known) {
    img.onerror = () => { img.onerror = null; img.src = fallback; };
    img.src = 'http://rb.localhost/siteicon/' + encodeURIComponent(host.replace(/^www\./, '')) + '?v=' + iconsVersion;
  } else {
    img.src = fallback;
  }
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
  // поверх сайта окно интерфейса узкое — просим место под уведомление
  send('toast', { w: el.offsetWidth, h: el.offsetHeight });
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    el.classList.remove('show');
    toastTimer = setTimeout(() => send('toast', {}), 350);
  }, 2200);
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
  const key = [s.accent, s.tintFrame, s.motion, s.barStyle, s.newtabLayout, S.dark, s.theme, s.glass, s.islandWidth, s.islandRadius, s.islandTabs].join('|');
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
  for (const b of ['classic', 'compact', 'floating', 'vertical', 'island']) body.toggle('bar-' + b, s.barStyle === b);
  // остров: ширина в процентах окна, скругление 0–100 % (100 % — капсула при высоте 46 px)
  body.toggle('island-tabs', s.barStyle === 'island' && !!s.islandTabs);
  root.setProperty('--island-w', String(s.islandWidth ?? 62));
  root.setProperty('--island-r', Math.round(((s.islandRadius ?? 100) / 100) * 23) + 'px');
  requestAnimationFrame(reportChrome);
  for (const n of ['clock', 'minimal', 'tiles', 'greeting']) body.toggle('nt-' + n, s.newtabLayout === n);
  body.toggle('amoled', s.theme === 'amoled');
  // стекло: непрозрачность панелей 100 % − выбранная прозрачность
  body.toggle('glass', s.glass > 0);
  root.setProperty('--glass-a', (100 - (s.glass || 0)) + '%');
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

let lastActiveTab = null;
function setState(next) {
  // Rust присылает только изменившиеся части: закладки, загрузки и параметры — не каждый раз.
  S = { ...S, ...next };
  document.documentElement.dataset.theme = S.dark ? 'dark' : 'light';
  document.body.classList.toggle('maximized', S.maximized);
  document.body.classList.toggle('fullscreen', S.fullscreen);
  document.body.classList.toggle('favbar', !!S.settings.favbar);
  document.body.classList.toggle('panel-open', !!S.panelOpen);
  $('#panelbtn').classList.toggle('on', !!S.panelOpen);
  applyAppearance(S.settings);
  if (S.active !== lastActiveTab) { lastActiveTab = S.active; setIslandHidden(false); }
  renderTabs();
  // вкладки внутри острова прокручиваются: активная всегда на виду
  if (tabsScroll()) tabEls.get(S.active)?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  renderToolbar();
  renderFavbar();
  renderStart();
  renderPasswordKey();
  renderExtButton();
  renderMyButtons();
  renderAutofillButton();
  renderSplitButton();
  renderCompact();
  renderBoostMenu();
  // фокус ушёл со строки (в неактивном окне blur может не прийти) — сворачиваем
  if (urlbox.classList.contains('expanded') && document.activeElement !== el.url && !sheetOpen) collapseUrl();
  requestAnimationFrame(measureUrl);
  el.menubtn.classList.toggle('dot', !!S.install?.updateReady);
  if (hubSection && hubSection !== 'history') renderHubList(false);
  if (menuKind === 'main') refreshMainMenu();
}

// Высоту панели знает только интерфейс: сообщаем её Rust, чтобы страница встала ровно под ней.
let lastChrome = 0;
// При вертикальных вкладках панель — колонка слева: тогда важна её ширина.
let lastChromeKey = '';
const vertical = () => document.body.classList.contains('bar-vertical');
// «Остров» висит над страницей: Rust показывает интерфейс только в полосе вкладок
// и в самом острове (с его скруглением), а страница начинается сразу под вкладками.
const islandFloats = () => document.body.classList.contains('bar-island') && !document.body.classList.contains('island-tabs');
let islandHidden = false;
let floatKey = '';
function chromeShape() {
  if (!islandFloats() || document.body.classList.contains('fullscreen')) return null;
  const top = Math.round(el.titlebar.getBoundingClientRect().bottom);
  const parts = [[0, 0, innerWidth, top, 0]];
  if (!islandHidden) {
    for (const node of [$('#toolbar'), $('#favbar'), $('#findbar'), ...(sheetOpen ? [el.sheet] : [])]) {
      const r = node.getBoundingClientRect();
      if (r.width < 2 || r.height < 2 || getComputedStyle(node).display === 'none') continue;
      const radius = parseFloat(getComputedStyle(node).borderTopLeftRadius) || 0;
      parts.push([Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height) + 1, Math.round(radius)]);
    }
  }
  return { top, parts };
}
function reportChrome() {
  const box = el.chrome.getBoundingClientRect();
  const side = vertical() ? Math.round(box.width) : 0;
  const real = side ? 0 : Math.round(box.height);
  const compact = document.body.classList.contains('compact-mode');
  const height = compact ? 6 : real;
  if (!height && !side) return;
  const float = compact ? null : chromeShape();
  const key = height + '|' + side + '|' + JSON.stringify(float);
  if (key === floatKey) return;
  floatKey = key;
  lastChromeKey = height + '|' + side;
  lastChrome = height || side;
  document.documentElement.style.setProperty('--bar-h', real + 'px');
  document.documentElement.style.setProperty('--side-w', side + 'px');
  // раскрытый компактный режим вырос — окно интерфейса растёт вместе с ним
  if (compact && compactOpen && !layerKind) send('overlay', { mode: 'sheet', height: compactExtra() + (sheetOpen ? sheetHeight : 0) });
  // страница начинается под вкладками: снимок под меню и библиотекой — оттуда же
  document.body.classList.toggle('island-float', !!float);
  if (float) document.documentElement.style.setProperty('--page-top', float.top + 'px');
  send('chrome', { height, side, float });
}
new ResizeObserver(reportChrome).observe(el.chrome);
addEventListener('resize', reportChrome);

// Прокрутка вниз прячет остров (страницу видно целиком), вверх — возвращает.
// Не прячем, пока в нём что-то делают: печатают адрес, открыто меню, идёт поиск.
let islandTimer = 0;
function setIslandHidden(hidden) {
  if (hidden && (document.activeElement === el.url || menuKind || document.body.classList.contains('finding'))) return;
  if (hidden === islandHidden) return;
  clearTimeout(islandTimer);
  if (hidden) {
    document.body.classList.add('island-hidden');
    // форму окна меняем, когда остров уже уехал, — иначе он обрезался бы на глазах
    islandTimer = setTimeout(() => { islandHidden = true; reportChrome(); }, 220);
  } else {
    islandHidden = false;
    reportChrome();
    document.body.classList.remove('island-hidden');
  }
}
function pageScroll(down, y) {
  if (!islandFloats()) return;
  setIslandHidden(down && y > 80);
}
el.titlebar.addEventListener('mouseenter', () => setIslandHidden(false));

// ---------- окно ----------

$('#win-min').addEventListener('click', () => send('minimize'));
$('#win-max').addEventListener('click', () => send('maximize'));
$('#win-close').addEventListener('click', () => send('close'));

// Пустое место в полосе вкладок двигает окно, двойной щелчок разворачивает.
// вкладки внутри острова и в одной строке с панелью листаются колесом мыши
function tabsScroll() { return ['island-tabs', 'bar-compact', 'bar-floating'].some((c) => document.body.classList.contains(c)); }
el.tabs.addEventListener('wheel', (event) => {
  if (!tabsScroll() || el.tabs.scrollWidth <= el.tabs.clientWidth) return;
  event.preventDefault();
  el.tabs.scrollLeft += event.deltaY || event.deltaX;
}, { passive: false });

// в «Острове» пустые поля по бокам — тоже место, за которое тянут окно
el.chrome.addEventListener('mousedown', (event) => {
  if (event.button !== 0 || event.target !== el.chrome) return;
  if (event.detail === 2) send('maximize');
  else send('drag');
});
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
    groupChips.forEach((chip) => chip.remove()); // метки групп встанут заново ниже
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
  renderGroups();
  // только что созданная группа: сразу предлагаем дать ей имя
  if (groupJustCreated && groupChips.has(groupJustCreated)) {
    const gid = groupJustCreated;
    groupJustCreated = 0;
    requestAnimationFrame(() => openGroupMenu(gid, groupChips.get(gid)));
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
    // вкладки свёрнутых групп спрятаны (нулевой размер) — местом для вставки они не считаются
    const shown = rects.filter((r) => axis.size(r) > 0);
    const min = axis.start(shown[0]) - axis.start(rects[from]);
    const max = axis.end(shown[shown.length - 1]) - axis.end(rects[from]);
    const shift = Math.max(min, Math.min(max, dx));
    node.style.transform = `${axis.move}(${shift}px)`;

    const center = axis.start(rects[from]) + axis.size(rects[from]) / 2 + shift;
    target = from;
    rects.forEach((r, i) => {
      if (!axis.size(r)) return;
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
  // перевод: страница не по-русски или уже переведённая (тогда — вернуть оригинал)
  const translated = /\.translate\.goog\//.test(tab.url);
  const foreign = !!tab.lang && !/^(ru|und)/.test(tab.lang) && /^https?:/.test(tab.url);
  const tb = $('#translatebtn');
  tb.classList.toggle('show', !tab.start && (translated || foreign));
  tb.classList.toggle('on', translated);
  tb.title = translated ? 'Показать оригинал' : 'Перевести страницу на русский';
  // «картинка в картинке» — когда на странице играет видео
  $('#pipbtn').classList.toggle('show', !tab.start && !!tab.audio && !!tab.video);
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
$('#panelbtn').addEventListener('click', () => send('toggle_panel'));
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
  if (!layerKind) send('overlay', compactOpen ? { mode: 'sheet', height: compactExtra() } : { mode: 'none' });
  if (islandFloats()) reportChrome();
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
    send('overlay', { mode: 'sheet', height: height + compactExtra() });
    // остров поверх страницы: список подсказок — карточка, вокруг неё видна страница
    if (islandFloats()) reportChrome();
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
let prewarmTimer = 0;
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
  // готовые живые обои играют так же, как своё видео
  const liveId = kind.startsWith('live:') ? kind.slice(5) : null;
  if (liveId && !(s.liveWalls || []).some((w) => w.id === liveId)) kind = 'aurora';
  const videoUrl = liveId ? `http://rb.localhost/live/${encodeURIComponent(liveId)}` : s.videoUrl;
  if (liveId && kind !== 'aurora') kind = 'video';
  const key = kind + '|' + s.wallpaperUrl + '|' + videoUrl + '|' + s.wallpaperDim;
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
    wall.classList.remove('show');
    video.src = videoUrl;
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
        // наведение — сайт начинает грузиться заранее, щелчок покажет его почти мгновенно
        onmouseenter: () => { clearTimeout(prewarmTimer); prewarmTimer = setTimeout(() => send('prewarm', { url: bookmark.url }), 140); },
        onmouseleave: () => clearTimeout(prewarmTimer),
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
  // с большой буквы только день недели: «Суббота, 14 ноября», а не «14 Ноября»
  const date = now.toLocaleDateString('ru-RU', { weekday: 'long', day: 'numeric', month: 'long' });
  el.date.textContent = date.charAt(0).toUpperCase() + date.slice(1);
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
  el.menu.style.left = el.menu.style.top = el.menu.style.right = el.menu.style.maxHeight = '';
  el.menu.classList.add('open');
  // настоящие размеры, а не getBoundingClientRect: меню появляется уменьшенным (scale .94),
  // и по рамке анимации оно вставало бы правым краем за окно
  place({ width: el.menu.offsetWidth, height: el.menu.offsetHeight });
  // в невысоком окне длинное меню не уходит за нижний край, а прокручивается
  el.menu.style.maxHeight = Math.max(120, innerHeight - el.menu.offsetTop - 8) + 'px';
  el.menu.focus();
}

function closeMenu() {
  if (!menuKind) return;
  const kind = menuKind;
  if (kind === 'ext') send('ext_popup_close');
  if (el.menu.classList.contains('extbox')) {
    el.menu.classList.remove('extbox');
    el.menu.style.width = el.menu.style.height = '';
  }
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
    item({ icon: 'translate', label: /\.translate\.goog\//.test(activeTab()?.url || '') ? 'Показать оригинал' : 'Перевести страницу',
      disabled: !page || (/^ru/.test(activeTab()?.lang || '') && !/\.translate\.goog\//.test(activeTab()?.url || '')),
      onClick: () => send('translate', { original: /\.translate\.goog\//.test(activeTab()?.url || '') }) }),
    item({ icon: 'pip', label: 'Картинка в картинке', kb: 'Alt+P', disabled: !page || !activeTab()?.video, onClick: () => send('pip') }),
    item({ icon: 'expand', label: 'Компактный режим', kb: 'Ctrl+Shift+K', check: !!S.settings.compact, onClick: () => set('compact', !S.settings.compact) }),
    item({ icon: 'split', label: S.split && [S.split.left, S.split.right].includes(S.active) ? 'Убрать разделение экрана' : 'Разделить экран', disabled: !page,
      onClick: () => send(S.split && [S.split.left, S.split.right].includes(S.active) ? 'unsplit' : 'split') }),
    item({ icon: 'moon2', label: 'Тёмная тема для этого сайта', check: !!activeTab()?.dark, disabled: !page, onClick: () => send('dark_site') }),
    item({ icon: 'bolt', label: 'Настроить этот сайт…', disabled: !page, onClick: () => setTimeout(openBoostMenu, 140) }),
    item({ icon: 'bar', label: 'Панель избранного', kb: 'Ctrl+Shift+B', check: S.settings.favbar,
      onClick: () => send('set', { key: 'favbar', value: !S.settings.favbar }) }),
    item({ icon: 'code', label: 'Инструменты разработчика', kb: 'F12', disabled: !page, onClick: () => send('devtools') }),
    sep(),
    S.install?.updateReady && item({ icon: 'reload', label: 'Перезапустить и обновиться', onClick: () => send('restart_update') }),
    item({ icon: 'gear', label: 'Параметры', onClick: () => openHub('settings') }),
  ].filter(Boolean);
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
    ...tabMenuExtras(tab),
    sep(),
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
  // пока параметры скрыты (окно ждёт снимка страницы), у кнопок нет размеров — двигаем «таблетку», когда они появятся
  new ResizeObserver(update).observe(root);
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
  const dimRow = setting('Затемнение фото и видео', 'Чтобы часы и поиск хорошо читались',
    dim, h('button', { class: 'text-btn outline photo-remove', onclick: () => send('remove_wallpaper') }, 'Убрать фото'));
  settingsUpdaters.push(() => {
    const s = S.settings;
    const current = s.wallpaper === 'image' && s.hasImage ? 'image' : s.wallpaper;
    tiles.forEach((tile, i) => tile.classList.toggle('on', WALLS[i][0] === current));
    video.classList.toggle('on', s.wallpaper === 'video' && s.hasVideo);
    dimRow.style.display = s.hasImage || s.wallpaper === 'video' || s.wallpaper.startsWith('live:') ? '' : 'none';
    videoRow.style.display = s.hasVideo ? '' : 'none';
    photo.classList.toggle('on', current === 'image');
    photo.querySelector('i').style.backgroundImage = s.hasImage ? `url("${s.wallpaperUrl}")` : '';
    photo.querySelector('span').textContent = s.hasImage ? 'Своё фото' : 'Загрузить фото';
    dim.value = s.wallpaperDim ?? 35;
    dimRow.querySelector('.photo-remove').style.display = s.hasImage ? '' : 'none';
  });
  return h('div', {},
    h('div', { class: 'set wallset' },
      h('div', { class: 'label' }, 'Обои новой вкладки', h('small', {}, 'Готовый узор, живые обои, своё фото, GIF или видео')),
      h('div', { class: 'walls' }, ...tiles, photo, video)),
    dimRow, videoRow, always);
}

/** Галерея готовых живых обоев: видео с Wikimedia Commons со свободной лицензией. */
function wallGalleryCard() {
  const walls = S.settings.liveWalls || [];
  if (!walls.length) return null;
  const tiles = walls.map((w) => {
    const url = `http://rb.localhost/live/${encodeURIComponent(w.id)}`;
    // кадр из середины ролика вместо картинки; на наведении ролик оживает
    const preview = h('video', { muted: true, loop: true, playsinline: true, preload: 'metadata', src: url + '#t=2' });
    preview.muted = true;
    const tile = h('button', { class: 'walltile livetile', title: w.name, onclick: () => set('wallpaper', 'live:' + w.id) },
      preview, h('em', {}, 'видео'), h('span', {}, w.name));
    tile.addEventListener('mouseenter', () => preview.play().catch(() => {}));
    tile.addEventListener('mouseleave', () => preview.pause());
    settingsUpdaters.push(() => tile.classList.toggle('on', S.settings.wallpaper === 'live:' + w.id));
    return tile;
  });
  const credit = h('small', {});
  const creditRow = h('div', { class: 'wallcredit' }, credit);
  settingsUpdaters.push(() => {
    const w = walls.find((x) => S.settings.wallpaper === 'live:' + x.id);
    creditRow.style.display = w ? '' : 'none';
    if (!w) return;
    credit.replaceChildren(`«${w.name}» — ${w.author || 'автор не указан'}, ${w.license}, Wikimedia Commons · `,
      h('a', { href: '#', onclick: (e) => { e.preventDefault(); send('open', { url: w.source, newTab: true }); closeHub(); } }, 'источник'));
  });
  return h('div', { class: 'card' }, h('h2', {}, 'Живые обои из интернета'),
    h('div', { class: 'set wallset' },
      h('div', { class: 'label' }, 'Видеообои', h('small', {}, 'Космос, природа и погода: съёмки NASA и фотографов со свободной лицензией. Звук выключен, на паузе, когда вкладку не видно')),
      h('div', { class: 'walls' }, ...tiles)),
    creditRow);
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

function proxyCard() {
  let mode = S.settings.proxyMode || 'off';
  const modeSeg = segmented([['off', 'Без прокси'], ['http', 'HTTP'], ['socks5', 'SOCKS5']],
    () => mode, (value) => { mode = value; settingsUpdaters.forEach((u) => u()); });
  const host = h('input', { type: 'text', class: 'field', placeholder: '127.0.0.1', spellcheck: 'false', value: S.settings.proxyHost || '' });
  const port = h('input', { type: 'number', class: 'field port', placeholder: '1080', min: 1, max: 65535, value: S.settings.proxyPort || '' });
  const apply = h('button', {
    class: 'text-btn primary',
    onclick: () => send('set_proxy', { mode, host: host.value, port: Number(port.value) || 0 }),
  }, 'Применить и перезапустить');
  const addressRow = setting('Адрес прокси', 'Логин и пароль прокси не поддерживаются', host, port);
  settingsUpdaters.push(() => { addressRow.style.display = mode === 'off' ? 'none' : ''; });
  return h('div', { class: 'card' }, h('h2', {}, 'Сеть'),
    setting('Прокси', 'Весь трафик браузера пойдёт через этот сервер. Нужен перезапуск — вкладки вернутся сами; другие окна браузера закройте', modeSeg),
    addressRow,
    h('div', { class: 'set' }, h('div', { class: 'label' }), h('div', { class: 'controls' }, apply)));
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

// Параметры разбиты по разделам: полоса разделов сверху, ниже — карточки выбранного.
// Поиск в шапке ищет по всем разделам сразу.
let settingsCat = 'look';

function darkSitesRow() {
  const chips = h('div', { class: 'pw-never' });
  const row = h('div', {}, chips);
  settingsUpdaters.push(() => {
    const sites = S.settings.darkSites || [];
    chips.replaceChildren(...(sites.length ? [h('small', {}, 'Всегда тёмные: '), ...sites.map((site) =>
      h('span', { class: 'pw-chip' }, site, h('button', { title: 'Убрать', onclick: () => send('dark_site_remove', { host: site }) }, '×')))] : []));
  });
  return row;
}

function glassSetting() {
  const range = h('input', { type: 'range', min: 0, max: 70, step: 5, class: 'range' });
  const value = h('span', { class: 'rangeval' });
  range.addEventListener('input', () => { value.textContent = range.value + ' %'; set('glass', Number(range.value)); });
  settingsUpdaters.push(() => {
    if (document.activeElement !== range) range.value = S.settings.glass ?? 0;
    value.textContent = range.value + ' %';
  });
  return setting('Прозрачность панелей', 'Полоса вкладок, адресная строка, меню и параметры становятся матовым стеклом: сквозь них видно обои',
    range, value);
}

const SETTINGS_CATS = [
  ['account', 'Аккаунт', () => accountCards()],
  ['look', 'Оформление', () => [
    h('div', { class: 'card' }, h('h2', {}, 'Тема и цвет'),
      setting('Тема', 'Как в Windows, светлая, тёмная или AMOLED — чистый чёрный, бережёт заряд на OLED-экранах',
        segmented([['system', 'Как в системе'], ['light', 'Светлая'], ['dark', 'Тёмная'], ['amoled', 'AMOLED']],
          () => S.settings.theme, (value) => set('theme', value))),
      colorSetting(),
      setting('Подкрасить рамку окна', 'Полоса вкладок и панель слегка окрашиваются в цвет браузера',
        toggle(() => S.settings.tintFrame, (value) => set('tint_frame', value))),
      glassSetting(),
      setting('Тёмная тема для всех сайтов', 'Светлые сайты становятся тёмными, картинки и видео не меняются. Для одного сайта — меню «⋯» → «Тёмная тема для этого сайта»',
        toggle(() => S.settings.darkAll, (value) => set('dark_all', value))),
      darkSitesRow()),
    h('div', { class: 'card' }, h('h2', {}, 'Движение'),
      setting('Анимации', 'Как быстро и с каким характером всё появляется',
        segmented([['smooth', 'Плавные'], ['fast', 'Быстрые'], ['bouncy', 'Пружинистые'], ['off', 'Без анимаций']],
          () => S.settings.motion, (value) => set('motion', value))),
      typeof splashSetting === 'function' && splashSetting()),
  ]],
  ['walls', 'Обои', () => [
    h('div', { class: 'card' }, h('h2', {}, 'Узоры и свои обои'), wallpaperSetting()),
    typeof wallGalleryCard === 'function' && wallGalleryCard(),
  ]],
  ['layout', 'Панели и вкладки', () => [
    h('div', { class: 'card' }, h('h2', {}, 'Панели'),
      setting('Вид панели', 'Классика, всё в одну строку, парящая панель или вкладки слева',
        segmented([['classic', 'Классика'], ['compact', 'Компакт'], ['floating', 'Плавающая'], ['island', 'Остров'], ['vertical', 'Вертикальные вкладки']],
          () => S.settings.barStyle, (value) => set('bar_style', value))),
      ...islandSettings(),
      setting('Компактный режим', 'Вкладки и адресная строка свёрнуты в тонкую полоску сверху и выезжают при наведении мыши — почти весь экран для сайта · Ctrl+Shift+K',
        toggle(() => S.settings.compact, (value) => set('compact', value))),
      setting('Панель избранного', 'Сайты из избранного под адресной строкой · Ctrl+Shift+B',
        toggle(() => S.settings.favbar, (value) => set('favbar', value))),
      setting('Боковая панель', 'Заметки, калькулятор и мессенджеры справа · Ctrl+Shift+E',
        toggle(() => S.panelOpen, () => send('toggle_panel')))),
    myButtonsCard(),
    h('div', { class: 'card' }, h('h2', {}, 'Новая вкладка'),
      setting('Что показывать', 'Часы с поиском, приветствие, плитки сайтов или почти ничего',
        segmented([['clock', 'Часы и поиск'], ['greeting', 'Приветствие'], ['tiles', 'Плитки'], ['minimal', 'Минимализм']],
          () => S.settings.newtabLayout, (value) => set('newtab_layout', value)))),
  ]],
  ['search', 'Поиск', () => [
    h('div', { class: 'card' }, h('h2', {}, 'Поиск'),
      setting('Поисковая система', 'Для адресной строки и строки поиска на новой вкладке',
        segmented(S.engines.map((e) => [e.id, e.name]), () => S.search, (value) => set('search', value)))),
  ]],
  ['privacy', 'Пароли и безопасность', () => [
    passwordsCard(),
    autofillCard(),
    h('div', { class: 'card' }, h('h2', {}, 'Конфиденциальность'),
      setting('Блокировка рекламы и трекеров',
        () => S.ubolId
          ? 'uBlock Origin Lite: фильтры uBlock, EasyList, EasyPrivacy и другие'
          : `Около 3 600 рекламных доменов · заблокировано запросов: ${S.blockedTotal.toLocaleString('ru-RU')}`,
        ubolButton(),
        toggle(() => S.adblock, (value) => set('adblock', value))),
      setting('Журнал посещений', 'Все страницы из раздела «Журнал»',
        confirmButton('Очистить', 'Точно очистить?', () => { send('history_clear'); toast('Журнал очищен'); })),
      setting('Данные сайтов', 'Файлы cookie и кэш — вы выйдете из аккаунтов',
        confirmButton('Удалить', 'Точно удалить?', () => send('clear_data')))),
  ]],
  ['perf', 'Производительность', () => [
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
      setting('Процессы движка', () => {
          const want = S.settings.processLimit || 0, now = S.settings.processLimitActive || 0;
          const base = 'Меньше процессов — меньше памяти, но тяжёлая вкладка может притормозить соседние с того же процесса';
          return want !== now ? base + '. Изменение заработает после перезапуска браузера' : base;
        },
        segmented([[0, 'Авто'], [2, '2'], [4, '4'], [8, '8']], () => S.settings.processLimit || 0, (value) => set('process_limit', value)),
        (() => {
          const b = h('button', { class: 'text-btn outline', onclick: () => send('restart_update') }, 'Перезапустить');
          settingsUpdaters.push(() => { b.style.display = (S.settings.processLimit || 0) !== (S.settings.processLimitActive || 0) ? '' : 'none'; });
          return b;
        })())),
  ]],
  ['downloads', 'Загрузки', () => [
    h('div', { class: 'card' }, h('h2', {}, 'Загрузки'),
      setting('Папка для загрузок', () => S.settings.downloadDir || `${S.settings.defaultDownloadDir} (по умолчанию)`,
        h('button', { class: 'text-btn outline', onclick: () => send('open_download_dir') }, 'Открыть'),
        h('button', { class: 'text-btn outline', onclick: () => send('reset_download_dir') }, 'По умолчанию'),
        h('button', { class: 'text-btn primary', onclick: () => send('pick_download_dir') }, 'Изменить'))),
  ]],
  ['ext', 'Расширения', () => extensionsCards()],
  ['network', 'Сеть', () => [proxyCard()]],
  ['startup', 'Запуск', () => [
    h('div', { class: 'card' }, h('h2', {}, 'Запуск'),
      defaultBrowserSetting(),
      setting('При запуске', BOOT.inprivate ? 'В окне InPrivate вкладки не сохраняются' : 'Что открывать при старте браузера',
        segmented([[true, 'Вкладки прошлого сеанса'], [false, 'Новую вкладку']],
          () => S.settings.restoreSession, (value) => set('restore_session', value)))),
    importCard(),
    installCard(),
  ]],
  ['keys', 'Клавиши', () => keysCards()],
];

function openSettingsCat(id) {
  settingsCat = id;
  el.hubq.value = '';
  hubKey = null;
  renderHubList(true);
}

function settingsPage() {
  settingsUpdaters = [];
  const query = el.hubq.value.trim().toLowerCase();
  const nav = h('div', { class: 'setcats' }, ...SETTINGS_CATS.map(([id, name]) =>
    h('button', { class: 'setcat' + (!query && id === settingsCat ? ' on' : ''), onclick: () => openSettingsCat(id) }, name)));
  let cards;
  if (query) {
    // поиск: из всех разделов оставляем строки с запросом и карточки, где что-то осталось
    cards = SETTINGS_CATS.flatMap(([, , build]) => build()).filter(Boolean).filter((card) => {
      if (card.querySelector('h2')?.textContent.toLowerCase().includes(query)) return true;
      let any = false;
      for (const row of card.querySelectorAll('.set')) {
        if (row.textContent.toLowerCase().includes(query)) any = true;
        else row.remove();
      }
      // сочетания клавиш — не строки параметров, а пары «что — клавиши»: ищем и по ним
      for (const what of card.querySelectorAll('.keys > span')) {
        const keys = what.nextElementSibling;
        if ((what.textContent + ' ' + keys.textContent).toLowerCase().includes(query)) any = true;
        else { what.remove(); keys.remove(); }
      }
      return any;
    });
    if (!cards.length) cards = [emptyState('settings')];
  } else {
    cards = SETTINGS_CATS.find(([id]) => id === settingsCat)[2]().filter(Boolean);
  }
  // значения выставляем после вставки в документ, когда известны размеры кнопок
  requestAnimationFrame(() => {
    settingsUpdaters.forEach((update) => update());
    // в узком окне разделы листаются вбок: выбранный должен быть на виду
    const on = nav.querySelector('.on');
    if (on) nav.scrollLeft = Math.max(0, on.offsetLeft - nav.clientWidth / 2 + on.offsetWidth / 2);
  });
  return [nav, ...cards.map((card, i) => { card.style.setProperty('--i', i + 1); return card; })];
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
    // вход и выход из аккаунта меняют сам вид раздела, а не только значения
    const key = 'settings|' + settingsCat + '|' + el.hubq.value + '|' + ((S.account || {}).name || '');
    if (hubKey === key) {
      settingsUpdaters.forEach((update) => update());
      return;
    }
    hubKey = key;
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
      menuItems[menuSel]?.scrollIntoView({ block: 'nearest' });
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
  toast,
  passwordOffer,
  updateReady,
  groupCreated,
  pageScroll,
  iconsUpdated: () => { iconsVersion++; tilesKey = null; renderStart(); },
  accountError,
  extPopupBox,
  extensionInstalled: (name) => toast(`Расширение «${name}» установлено — оно в меню «пазла» на панели`),
  passwordRevealed,
  passwordsImported: (count) => toast(count ? `Перенесено паролей: ${count}. Не забудьте удалить CSV-файл` : 'Новых паролей в файле не нашлось'),
  pasteText,
  showSiteInfo,
  showTasks,
  proxyInvalid: () => toast('Проверьте адрес и порт прокси'),
  cookiesCleared: (count) => { closeMenu(); toast(`Удалено файлов cookie: ${count}`); },
  imported: (count) => toast(count ? `Перенесено в избранное: ${count}` : 'Новых сайтов для избранного не нашлось'),
};


// ---------- пароли ----------
// Интерфейс знает только сайт и логин. Сам пароль приходит сюда лишь после Windows Hello
// (показать/скопировать), а подставляет его в страницу Rust — по щелчку здесь.

const pwkey = $('#pwkey');
let pwOffer = null;

function renderPasswordKey() {
  const here = S.pwHere || [];
  const show = !!pwOffer || here.length > 0;
  pwkey.classList.toggle('show', show);
  pwkey.classList.toggle('offer', !!pwOffer);
  pwkey.title = pwOffer ? 'Сохранить пароль' : 'Войти с сохранённым паролем';
}

function placeAtKey() {
  const anchor = pwkey.getBoundingClientRect();
  el.menu.style.left = Math.max(8, Math.min(anchor.right - 330, innerWidth - 340)) + 'px';
  el.menu.style.top = anchor.bottom + 10 + 'px';
  el.menu.style.transformOrigin = 'top right';
}

function showPasswordOffer() {
  if (!pwOffer) return;
  const offer = pwOffer;
  const nodes = [
    h('div', { class: 'pw-head' },
      h('span', { class: 'pw-badge' }, icon('key', 20)),
      h('div', {},
        h('b', {}, offer.update ? 'Обновить пароль?' : 'Сохранить пароль?'),
        h('small', {}, offer.site))),
    h('div', { class: 'pw-fields' },
      h('div', { class: 'pw-field' }, h('span', {}, 'Логин'), h('b', {}, offer.username || 'без логина')),
      h('div', { class: 'pw-field' }, h('span', {}, 'Пароль'), h('b', {}, '••••••••••'))),
    h('div', { class: 'pw-note' }, icon('lock', 13), 'Шифруется Windows: прочитать сможет только ваша учётная запись'),
    h('div', { class: 'pw-acts' },
      h('button', { class: 'text-btn', onclick: () => { send('pw_never'); pwOffer = null; closeMenu(); renderPasswordKey(); } }, 'Никогда для сайта'),
      h('button', { class: 'text-btn outline', onclick: () => { send('pw_dismiss'); pwOffer = null; closeMenu(); renderPasswordKey(); } }, 'Не сейчас'),
      h('button', { class: 'text-btn primary', onclick: () => { send('pw_save'); pwOffer = null; closeMenu(); renderPasswordKey(); } }, 'Сохранить')),
  ];
  showMenu('pwsave', nodes, placeAtKey);
}

function showPasswordPicker() {
  const here = S.pwHere || [];
  const nodes = [
    h('div', { class: 'menu-title' }, 'Войти как'),
    ...here.map(([index, username]) => item({
      icon: 'key', label: username || 'без логина', onClick: () => send('pw_fill', { index }),
    })),
    sep(),
    item({ icon: 'gear', label: 'Управление паролями', onClick: () => { settingsCat = 'privacy'; openHub('settings'); } }),
  ];
  showMenu('pwpick', nodes, placeAtKey);
}

$('#translatebtn').addEventListener('click', () => send('translate', { original: /\.translate\.goog\//.test(activeTab()?.url || '') }));
$('#pipbtn').addEventListener('click', () => send('pip'));

pwkey.addEventListener('click', () => {
  if (menuKind === 'pwsave' || menuKind === 'pwpick') return closeMenu();
  if (pwOffer) showPasswordOffer();
  else showPasswordPicker();
});

function passwordOffer(offer) {
  pwOffer = offer;
  renderPasswordKey();
  showPasswordOffer();
}

// показанные пароли: номер → текст; через 20 секунд снова скрываются
const revealed = new Map();
function passwordRevealed({ index, password, copy }) {
  if (copy) {
    navigator.clipboard.writeText(password).then(() => toast('Пароль скопирован'), () => toast('Не удалось скопировать'));
    return;
  }
  revealed.set(index, password);
  settingsUpdaters.forEach((update) => update());
  setTimeout(() => { revealed.delete(index); settingsUpdaters.forEach((update) => update()); }, 20000);
}

function passwordsCard() {
  const query = h('input', { type: 'text', class: 'field', placeholder: 'Найти сайт или логин', spellcheck: 'false' });
  const list = h('div', { class: 'pw-list' });
  const never = h('div', { class: 'pw-never' });
  const draw = () => {
    const q = query.value.trim().toLowerCase();
    const rows = (S.logins || []).map((login, index) => ({ ...login, index }))
      .filter((l) => !q || l.site.includes(q) || l.username.toLowerCase().includes(q))
      .sort((a, b) => a.site.localeCompare(b.site));
    list.replaceChildren(...(rows.length ? rows.map((l) => h('div', { class: 'pw-row' },
      h('span', { class: 'pw-fav' }, l.site.replace(/^www\./, '').charAt(0).toUpperCase()),
      h('div', { class: 'pw-who' }, h('b', {}, l.site), h('small', {}, l.username || 'без логина')),
      healthBadge(l.index),
      h('code', { class: 'pw-secret' + (revealed.has(l.index) ? ' open' : '') }, revealed.get(l.index) ?? '••••••••••'),
      actionButton(revealed.has(l.index) ? 'eyeoff' : 'eye', revealed.has(l.index) ? 'Скрыть' : 'Показать (Windows Hello)', () => {
        if (revealed.has(l.index)) { revealed.delete(l.index); draw(); } else send('pw_reveal', { index: l.index });
      }),
      actionButton('copy', 'Скопировать пароль (Windows Hello)', () => send('pw_reveal', { index: l.index, copy: true })),
      actionButton('trash', 'Удалить', () => send('pw_delete', { index: l.index }))))
      : [h('div', { class: 'pw-empty' }, (S.logins || []).length ? 'Ничего не найдено' : 'Сохранённых паролей пока нет')]));
    const blocked = S.pwNever || [];
    never.replaceChildren(...(blocked.length ? [h('small', {}, 'Не предлагать для: '), ...blocked.map(([origin, site]) =>
      h('span', { class: 'pw-chip' }, site, h('button', { title: 'Снова предлагать', onclick: () => send('pw_unblock', { origin }) }, '×')))] : []));
  };
  query.addEventListener('input', draw);
  settingsUpdaters.push(() => {
    const key = JSON.stringify([S.logins, S.pwHealth, [...revealed]]);
    if (list.dataset.key === key) return;
    list.dataset.key = key;
    draw();
  });
  return h('div', { class: 'card' }, h('h2', {}, 'Пароли'),
    setting('Предлагать сохранять пароли', 'Пароли шифруются Windows (DPAPI) и хранятся только на этом компьютере. На сайт подставляются по щелчку на ключ в адресной строке и только по HTTPS',
      toggle(() => S.settings.passwords, (value) => set('passwords', value))),
    setting('Проверить пароли', () => S.pwChecking
        ? 'Проверяю… Наружу уходят только первые 5 знаков отпечатка пароля, сам пароль никуда не отправляется'
        : 'Ищет пароли в известных утечках (Have I Been Pwned), а ещё повторы и слишком простые пароли',
      (() => {
        const b = h('button', { class: 'text-btn outline', onclick: () => send('pw_check') }, 'Проверить');
        settingsUpdaters.push(() => { b.disabled = !!S.pwChecking || !(S.logins || []).length; });
        return b;
      })()),
    setting('Перенести из другого браузера', 'В Chrome, Edge или Firefox выгрузите пароли в CSV-файл и выберите его здесь. Потом удалите этот файл — в нём пароли открытым текстом',
      h('button', { class: 'text-btn outline', onclick: () => send('pw_import') }, 'Импорт из CSV')),
    h('div', { class: 'set pw-search' }, h('div', { class: 'label' }, 'Сохранённые пароли',
      h('small', {}, 'Посмотреть или скопировать пароль можно после Windows Hello — ПИН-кода, отпечатка или лица')), query),
    list, never);
}


// ---------- расширения ----------
// WebView2 не рисует кнопки расширений, поэтому у браузера своя: «пазл» на панели.
// Окошко расширения — отдельный webview, который Rust ставит под кнопкой.

const extbtn = $('#extbtn');
const STORE_URL = 'https://chromewebstore.google.com/category/extensions?hl=ru';

function extIcon(ext, size = 18) {
  if (!ext.icon) return h('span', { class: 'exticon none' }, icon('puzzle', size - 2));
  return h('img', { class: 'exticon', src: `http://rb.localhost/exticon/${ext.id}`, width: size, height: size, alt: '' });
}

function renderExtButton() {
  extbtn.classList.toggle('show', (S.extensions || []).length > 0);
}

function openExtPopup(ext) {
  if (!ext.popup) {
    if (ext.options) send('ext_options', { id: ext.id });
    return;
  }
  const box = h('div', { class: 'extframe' });
  showMenu('ext', [box], () => {
    const anchor = extbtn.getBoundingClientRect();
    const right = Math.min(innerWidth - 8, anchor.right + 6);
    const top = anchor.bottom + 8;
    el.menu.classList.add('extbox');
    Object.assign(el.menu.style, { left: right - 360 + 'px', top: top + 'px', width: '360px', height: '120px' });
    send('ext_popup', { id: ext.id, right, top });
  });
}

function extPopupBox(r) {
  if (menuKind !== 'ext') return;
  Object.assign(el.menu.style, { left: r.left + 'px', top: r.top + 'px', width: r.width + 'px', height: r.height + 'px' });
}

function showExtMenu() {
  const list = S.extensions || [];
  const nodes = [
    h('div', { class: 'menu-title' }, 'Расширения'),
    ...list.map((ext) => {
      const node = h('button', { class: 'mi ext-mi' + (ext.enabled ? '' : ' off'), disabled: !ext.enabled },
        h('span', { class: 'ic' }, extIcon(ext, 18)),
        h('span', { class: 'lbl' }, ext.name),
        !ext.popup && ext.options ? h('span', { class: 'kb' }, 'настройки') : null);
      node.addEventListener('click', () => {
        if (ext.popup) { closeMenu(); setTimeout(() => openExtPopup(ext), 140); }
        else { closeMenu(); if (ext.options) send('ext_options', { id: ext.id }); }
      });
      return node;
    }),
    sep(),
    item({ icon: 'open', label: 'Chrome Web Store', onClick: () => send('open', { url: STORE_URL, newTab: true }) }),
    item({ icon: 'gear', label: 'Управление расширениями', onClick: () => { settingsCat = 'ext'; openHub('settings'); } }),
  ];
  showMenu('extlist', nodes, () => {
    const anchor = extbtn.getBoundingClientRect();
    el.menu.style.left = Math.max(8, Math.min(anchor.right - 300, innerWidth - 310)) + 'px';
    el.menu.style.top = anchor.bottom + 10 + 'px';
    el.menu.style.transformOrigin = 'top right';
  });
}

extbtn.addEventListener('click', () => {
  if (menuKind === 'extlist' || menuKind === 'ext') return closeMenu();
  showExtMenu();
});

let extScanned = false;
function extensionsCards() {
  if (!extScanned) { extScanned = true; send('ext_scan'); }
  const list = h('div', { class: 'ext-list' });
  const browsers = h('div', {});
  settingsUpdaters.push(() => {
    const exts = S.extensions || [];
    list.replaceChildren(...(exts.length ? exts.map((ext) => h('div', { class: 'ext-row' + (ext.enabled ? '' : ' off') },
      extIcon(ext, 32),
      h('div', { class: 'ext-who' }, h('b', {}, ext.name),
        h('small', {}, [ext.version && `версия ${ext.version}`, ext.source].filter(Boolean).join(' · '))),
      ext.options && h('button', { class: 'text-btn outline', onclick: () => { send('ext_options', { id: ext.id }); closeHub(); } }, 'Настройки'),
      ext.builtin
        ? h('small', { class: 'ext-note' }, 'вкл./выкл. — «Блокировка рекламы»')
        : h('label', { class: 'switch' },
          (() => { const i = h('input', { type: 'checkbox' }); i.checked = ext.enabled; i.addEventListener('change', () => send('ext_enable', { id: ext.id, on: i.checked })); return i; })(),
          h('i')),
      !ext.builtin && actionButton('trash', 'Удалить', () => send('ext_remove', { id: ext.id }))))
      : [h('div', { class: 'pw-empty' }, 'Расширений пока нет')]));
    const found = S.extBrowsers || [];
    browsers.replaceChildren(...(found.length
      ? found.map(([name, count]) => setting(name,
        count ? `Можно перенести: ${count}` : 'Всё уже перенесено или переносить нечего',
        h('button', { class: 'text-btn ' + (count ? 'primary' : 'outline'), disabled: !count, onclick: () => send('ext_import', { browser: name }) }, 'Перенести')))
      : [h('div', { class: 'pw-empty' }, 'Других браузеров с расширениями не нашлось')]));
  });
  return [
    h('div', { class: 'card' }, h('h2', {}, 'Установленные расширения'), list),
    h('div', { class: 'card' }, h('h2', {}, 'Добавить'),
      setting('Chrome Web Store', 'Откройте страницу расширения в магазине и нажмите «Установить в Rust Browser» внизу справа. Пакет проверяется по подписи магазина',
        h('button', { class: 'text-btn primary', onclick: () => { send('open', { url: STORE_URL, newTab: true }); closeHub(); } }, 'Открыть магазин')),
      h('div', { class: 'ext-warn' }, icon('alert', 14),
        'Движок Chromium 157 поддерживает только расширения нового формата (Manifest V3). Старые расширения MV2 не установятся — так же, как в Chrome и Edge')),
    h('div', { class: 'card' }, h('h2', {}, 'Перенести из другого браузера'),
      h('div', { class: 'ext-hint' }, 'Копируются сами расширения. Их настройки и вход остаются в том браузере'),
      browsers),
  ];
}


// ---------- аккаунт ----------

let accountMode = 'login';
let accountErrorText = '';

function accountError(text) {
  accountErrorText = text;
  settingsUpdaters.forEach((update) => update());
  toast(text);
}

function timeAgo(seconds) {
  if (!seconds) return 'ещё не было';
  const diff = Math.max(0, Date.now() / 1000 - seconds);
  if (diff < 60) return 'только что';
  if (diff < 3600) return `${Math.round(diff / 60)} мин назад`;
  if (diff < 86400) return `${Math.round(diff / 3600)} ч назад`;
  return new Date(seconds * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'long', hour: '2-digit', minute: '2-digit' });
}

const SYNCED = [
  ['gear', 'Параметры, темы, цвета и обои'], ['star', 'Избранное'], ['clock', 'Журнал'],
  ['key', 'Пароли'], ['puzzle', 'Расширения'], ['zoom', 'Масштаб сайтов'], ['file', 'Заметки'],
];

function accountCards() {
  const a = S.account || {};
  if (a.name) {
    const status = h('small', {});
    const error = h('div', { class: 'acc-error' });
    settingsUpdaters.push(() => {
      const acc = S.account || {};
      status.textContent = `Синхронизация: ${timeAgo(acc.synced)} · папка ${acc.folder}`;
      error.textContent = acc.error ? `Ошибка: ${acc.error}` : '';
      error.style.display = acc.error ? '' : 'none';
    });
    return [
      h('div', { class: 'card' },
        h('div', { class: 'acc-head' },
          h('span', { class: 'acc-avatar' }, a.name.charAt(0).toUpperCase()),
          h('div', { class: 'acc-who' }, h('b', {}, a.name), status),
          h('button', { class: 'text-btn primary', onclick: () => send('account_sync') }, 'Синхронизировать'),
          confirmButton('Выйти', 'Точно выйти?', () => send('account_logout'))),
        error,
        h('div', { class: 'acc-synced' }, ...SYNCED.map(([name, text]) => h('span', {}, icon(name, 14), text))),
        h('div', { class: 'ext-hint acc-note' },
          'Данные зашифрованы вашим паролем ещё на этом компьютере (AES-256). В облачной папке лежит только шифр. Синхронизация — при запуске, раз в 5 минут и при закрытии. Чтобы войти на другом компьютере, откройте там ту же облачную папку и войдите с этим именем и паролем')),
    ];
  }
  const name = h('input', { type: 'text', class: 'field', placeholder: 'например, misha', spellcheck: 'false', autocomplete: 'off' });
  const pass = h('input', { type: 'password', class: 'field', placeholder: 'не меньше 8 символов', autocomplete: 'new-password' });
  const pass2 = h('input', { type: 'password', class: 'field', placeholder: 'ещё раз', autocomplete: 'new-password' });
  const folderText = h('small', {});
  const repeatRow = setting('Пароль ещё раз', 'Пароль не восстановить: без него данные в облаке не расшифровать никому, даже нам', pass2);
  const modeSeg = segmented([['login', 'Войти'], ['create', 'Создать аккаунт']], () => accountMode, (value) => {
    accountMode = value; accountErrorText = ''; settingsUpdaters.forEach((u) => u());
  });
  const error = h('div', { class: 'acc-error' });
  const go = h('button', { class: 'text-btn primary', onclick: () => {
    if (accountMode === 'create' && pass.value !== pass2.value) return accountError('Пароли не совпадают');
    accountErrorText = '';
    send(accountMode === 'create' ? 'account_create' : 'account_login',
      { name: name.value, password: pass.value, folder: (S.account || {}).picked || '' });
  } });
  [name, pass, pass2].forEach((input) => input.addEventListener('keydown', (e) => { if (e.key === 'Enter') go.click(); }));
  settingsUpdaters.push(() => {
    const acc = S.account || {};
    const folder = acc.picked || acc.defaultFolder;
    folderText.textContent = folder ? folder : 'Папка не выбрана';
    repeatRow.style.display = accountMode === 'create' ? '' : 'none';
    go.textContent = acc.busy ? 'Проверяю…' : accountMode === 'create' ? 'Создать и синхронизировать' : 'Войти';
    go.disabled = !!acc.busy;
    error.textContent = accountErrorText;
    error.style.display = accountErrorText ? '' : 'none';
  });
  return [
    h('div', { class: 'card' }, h('h2', {}, 'Аккаунт Rust Browser'),
      h('div', { class: 'ext-hint acc-intro' },
        'Пароли, журнал, избранное, расширения, параметры и темы — одинаковые на всех ваших компьютерах. Всё шифруется вашим паролем и хранится в вашей же облачной папке (OneDrive, Яндекс Диск, Google Drive): своих серверов у браузера нет, и прочитать данные не сможет никто, кроме вас'),
      setting('', '', modeSeg),
      setting('Имя аккаунта', 'Любое: почта или ник', name),
      setting('Пароль', 'Шифрует данные. Используйте надёжный и нигде больше не повторяйте', pass),
      repeatRow,
      h('div', { class: 'set' }, h('div', { class: 'label' }, 'Облачная папка', folderText),
        h('div', { class: 'controls' }, h('button', { class: 'text-btn outline', onclick: () => send('account_pick_folder') }, 'Выбрать'))),
      error,
      h('div', { class: 'set' }, h('div', { class: 'label' }), h('div', { class: 'controls' }, go))),
  ];
}


// ---------- свои кнопки на панели ----------
// Кнопка открывает сайт одним нажатием, а если он уже открыт — переключает на его вкладку.

const MYBTN_PRESETS = [
  ['Яндекс Музыка', 'https://music.yandex.ru/'], ['VK Музыка', 'https://vk.com/audio'],
  ['YouTube Music', 'https://music.youtube.com/'], ['YouTube', 'https://www.youtube.com/'],
  ['Telegram', 'https://web.telegram.org/k/'], ['ChatGPT', 'https://chatgpt.com/'],
  ['Кинопоиск', 'https://www.kinopoisk.ru/'], ['Яндекс Почта', 'https://mail.yandex.ru/'],
  ['Gmail', 'https://mail.google.com/'], ['Переводчик', 'https://translate.yandex.ru/'],
];

let mybtnsKey = null;
function renderMyButtons() {
  const list = S.settings.buttons || [];
  const key = JSON.stringify(list);
  if (key === mybtnsKey) return;
  mybtnsKey = key;
  $('#mybtns').replaceChildren(...list.map((b, index) => {
    const button = h('button', { class: 'icon-btn mybtn', title: `${b.name} — ${hostOf(b.url)}` }, favicon(b.url, true));
    button.addEventListener('click', (event) => send('my_button', { url: b.url, newTab: event.ctrlKey }));
    button.addEventListener('auxclick', (event) => { if (event.button === 1) send('open', { url: b.url, newTab: true }); });
    button.addEventListener('contextmenu', (event) => {
      event.preventDefault();
      showMenu('mybtn', [
        h('div', { class: 'menu-title' }, b.name),
        item({ icon: 'open', label: 'Открыть в новой вкладке', onClick: () => send('open', { url: b.url, newTab: true }) }),
        item({ icon: 'gear', label: 'Настроить кнопки', onClick: () => { settingsCat = 'layout'; openHub('settings'); } }),
        item({ icon: 'trash', label: 'Убрать кнопку', danger: true, onClick: () => setButtons(list.filter((_, i) => i !== index)) }),
      ], () => {
        const anchor = button.getBoundingClientRect();
        el.menu.style.left = Math.max(8, Math.min(anchor.left - 10, innerWidth - 280)) + 'px';
        el.menu.style.top = anchor.bottom + 10 + 'px';
      });
    });
    return button;
  }));
}

const setButtons = (list) => set('buttons', list);

function myButtonsCard() {
  const list = h('div', { class: 'mybtns-list' });
  const name = h('input', { type: 'text', class: 'field name', placeholder: 'Название', spellcheck: 'false' });
  const url = h('input', { type: 'text', class: 'field url', placeholder: 'music.yandex.ru', spellcheck: 'false' });
  const add = (title, address) => {
    let link = address.trim();
    if (!link) return toast('Укажите адрес сайта');
    if (!/^https?:\/\//i.test(link)) link = 'https://' + link;
    const buttons = S.settings.buttons || [];
    if (buttons.length >= 12) return toast('Не больше 12 кнопок');
    if (buttons.some((b) => b.url === link)) return toast('Такая кнопка уже есть');
    setButtons([...buttons, { name: title.trim() || hostOf(link), url: link }]);
    name.value = url.value = '';
  };
  url.addEventListener('keydown', (e) => { if (e.key === 'Enter') add(name.value, url.value); });
  const move = (index, delta) => {
    const buttons = [...(S.settings.buttons || [])];
    const [b] = buttons.splice(index, 1);
    buttons.splice(Math.max(0, Math.min(buttons.length, index + delta)), 0, b);
    setButtons(buttons);
  };
  settingsUpdaters.push(() => {
    const buttons = S.settings.buttons || [];
    const key = JSON.stringify(buttons);
    if (list.dataset.key === key) return;
    list.dataset.key = key;
    list.replaceChildren(...(buttons.length ? buttons.map((b, i) => h('div', { class: 'mybtn-row' },
      favicon(b.url, true),
      h('div', { class: 'ext-who' }, h('b', {}, b.name), h('small', {}, b.url)),
      i > 0 && actionButton('up', 'Левее', () => move(i, -1)),
      i < buttons.length - 1 && actionButton('down', 'Правее', () => move(i, 1)),
      actionButton('trash', 'Убрать', () => setButtons(buttons.filter((_, j) => j !== i)))))
      : [h('div', { class: 'pw-empty' }, 'Своих кнопок пока нет — выберите готовую или добавьте любой сайт')]));
  });
  const current = activeTab();
  return h('div', { class: 'card' }, h('h2', {}, 'Свои кнопки на панели'),
    h('div', { class: 'ext-hint acc-intro' }, 'Сайт одним нажатием. Если он уже открыт — кнопка переключит на его вкладку. Колёсико или Ctrl — в новой вкладке, правая кнопка — меню'),
    list,
    h('div', { class: 'mybtn-presets' }, ...MYBTN_PRESETS.map(([title, link]) =>
      h('button', { onclick: () => add(title, link) }, favicon(link, true), title))),
    h('div', { class: 'set mybtn-add' }, h('div', { class: 'label' }, 'Свой сайт', h('small', {}, 'Название и адрес')),
      h('div', { class: 'controls' }, name, url,
        current && current.url.startsWith('http') && h('button', { class: 'text-btn outline', onclick: () => add(current.title || '', current.url) }, 'Текущий сайт'),
        h('button', { class: 'text-btn primary', onclick: () => add(name.value, url.value) }, 'Добавить'))));
}

function islandSettings() {
  const width = h('input', { type: 'range', min: 40, max: 100, step: 2, class: 'range' });
  const widthText = h('span', { class: 'rangeval' });
  width.addEventListener('input', () => { widthText.textContent = width.value + ' %'; set('island_width', Number(width.value)); });
  const round = h('input', { type: 'range', min: 0, max: 100, step: 5, class: 'range' });
  const roundText = h('span', { class: 'rangeval' });
  round.addEventListener('input', () => { roundText.textContent = round.value + ' %'; set('island_radius', Number(round.value)); });
  const rows = [
    setting('Ширина острова', 'Сколько окна занимает остров', width, widthText),
    setting('Скругление', '100 % — капсула, 0 % — почти прямоугольник', round, roundText),
    setting('Вкладки внутри острова', 'Одна строка: слева кнопки и адрес, справа вкладки — больше места для страниц',
      toggle(() => S.settings.islandTabs, (value) => set('island_tabs', value))),
  ];
  settingsUpdaters.push(() => {
    const island = S.settings.barStyle === 'island';
    rows.forEach((row) => { row.style.display = island ? '' : 'none'; });
    if (document.activeElement !== width) width.value = S.settings.islandWidth ?? 62;
    if (document.activeElement !== round) round.value = S.settings.islandRadius ?? 100;
    widthText.textContent = width.value + ' %';
    roundText.textContent = round.value + ' %';
  });
  return rows;
}


// ---------- автозаполнение адресов и карт ----------

const afbtn = $('#afbtn');
function renderAutofillButton() {
  const here = S.afHere;
  afbtn.classList.toggle('show', !!(here && (here.card || here.address)));
}
afbtn.addEventListener('click', () => {
  if (menuKind === 'autofill') return closeMenu();
  const here = S.afHere || {};
  const data = S.autofill || { addresses: [], cards: [] };
  const nodes = [];
  if (here.card) {
    nodes.push(h('div', { class: 'menu-title' }, 'Карта'));
    data.cards.forEach((c, index) => nodes.push(item({ icon: 'card', label: `${c.label} · ${c.expires}`, onClick: () => send('af_fill', { kind: 'card', index }) })));
    if (!data.cards.length) nodes.push(h('div', { class: 'pw-empty' }, 'Карт пока нет'));
  }
  if (here.address) {
    nodes.push(h('div', { class: 'menu-title' }, 'Адрес и контакты'));
    data.addresses.forEach((a, index) => nodes.push(item({ icon: 'home', label: a.label || 'Адрес', onClick: () => send('af_fill', { kind: 'address', index }) })));
    if (!data.addresses.length) nodes.push(h('div', { class: 'pw-empty' }, 'Адресов пока нет'));
  }
  nodes.push(sep(), item({ icon: 'gear', label: 'Адреса и карты…', onClick: () => { settingsCat = 'privacy'; openHub('settings'); } }));
  showMenu('autofill', nodes, () => {
    const anchor = afbtn.getBoundingClientRect();
    el.menu.style.left = Math.max(8, Math.min(anchor.right - 320, innerWidth - 330)) + 'px';
    el.menu.style.top = anchor.bottom + 10 + 'px';
  });
});

function autofillCard() {
  const list = h('div', { class: 'mybtns-list' });
  const f = (placeholder, cls = '') => h('input', { type: 'text', class: 'field ' + cls, placeholder, spellcheck: 'false' });
  const addr = { name: f('ФИО'), phone: f('Телефон'), email: f('Почта'), country: f('Страна'), city: f('Город'), street: f('Улица, дом, кв.'), zip: f('Индекс', 'port') };
  const card = { holder: f('Имя на карте'), number: f('Номер карты'), month: f('ММ', 'port'), year: f('ГГ', 'port') };
  card.number.inputMode = 'numeric';
  const addAddress = h('button', { class: 'text-btn primary', onclick: () => {
    const value = Object.fromEntries(Object.entries(addr).map(([k, i]) => [k, i.value.trim()]));
    if (!Object.values(value).some(Boolean)) return toast('Заполните хотя бы одно поле');
    send('af_save', { kind: 'address', value });
    Object.values(addr).forEach((i) => { i.value = ''; });
  } }, 'Сохранить адрес');
  const addCard = h('button', { class: 'text-btn primary', onclick: () => {
    const value = Object.fromEntries(Object.entries(card).map(([k, i]) => [k, i.value.trim()]));
    if (!value.number) return toast('Укажите номер карты');
    send('af_save', { kind: 'card', value });
    Object.values(card).forEach((i) => { i.value = ''; });
  } }, 'Сохранить карту');
  settingsUpdaters.push(() => {
    const data = S.autofill || { addresses: [], cards: [] };
    const key = JSON.stringify(data);
    if (list.dataset.key === key) return;
    list.dataset.key = key;
    const rows = [
      ...data.addresses.map((a, index) => h('div', { class: 'mybtn-row' },
        h('span', { class: 'pw-fav' }, icon('home', 16)),
        h('div', { class: 'ext-who' }, h('b', {}, a.label || 'Адрес'), h('small', {}, [a.address.phone, a.address.email].filter(Boolean).join(' · '))),
        actionButton('trash', 'Удалить', () => send('af_delete', { kind: 'address', index })))),
      ...data.cards.map((c, index) => h('div', { class: 'mybtn-row' },
        h('span', { class: 'pw-fav' }, icon('card', 16)),
        h('div', { class: 'ext-who' }, h('b', {}, c.label), h('small', {}, `${c.holder || ''} · до ${c.expires}`)),
        actionButton('trash', 'Удалить', () => send('af_delete', { kind: 'card', index })))),
    ];
    list.replaceChildren(...(rows.length ? rows : [h('div', { class: 'pw-empty' }, 'Сохранённых адресов и карт пока нет')]));
  });
  return h('div', { class: 'card' }, h('h2', {}, 'Адреса и карты'),
    h('div', { class: 'ext-hint acc-intro' }, 'Шифруются Windows и хранятся только у вас. На сайте нажмите значок карты в адресной строке — поля заполнятся сами. Карта подставляется после Windows Hello и только на защищённые (HTTPS) сайты. Код CVC не сохраняется никогда'),
    list,
    h('div', { class: 'set af-form' }, h('div', { class: 'label' }, 'Новый адрес'),
      h('div', { class: 'controls af-grid' }, ...Object.values(addr), addAddress)),
    h('div', { class: 'set af-form' }, h('div', { class: 'label' }, 'Новая карта'),
      h('div', { class: 'controls af-grid' }, card.holder, card.number, card.month, card.year, addCard)));
}

/** Отметка у пароля после проверки. */
function healthBadge(index) {
  const health = (S.pwHealth || [])[index];
  if (!health) return null;
  if (health.leaked > 0) return h('span', { class: 'pw-badge-bad', title: `Встречался в утечках ${health.leaked.toLocaleString('ru-RU')} раз` }, 'в утечке');
  if (health.reused) return h('span', { class: 'pw-badge-warn', title: 'Тот же пароль стоит на другом сайте' }, 'повторяется');
  if (health.weak) return h('span', { class: 'pw-badge-warn', title: 'Короткий или простой' }, 'простой');
  return h('span', { class: 'pw-badge-ok' }, 'надёжный');
}



// ---------- группы вкладок ----------

const GROUP_COLORS = {
  blue: '#3b82f6', purple: '#8b5cf6', pink: '#ec4899', red: '#ef4444',
  orange: '#f97316', yellow: '#eab308', green: '#22c55e', cyan: '#06b6d4', grey: '#8b8fa3',
};
const groupChips = new Map();
const groupOf = (gid) => (S.groups || []).find((g) => g.id === gid);

/** Метки групп перед первой вкладкой каждой группы и цветная полоска у вкладок. */
function renderGroups() {
  const groups = S.groups || [];
  for (const [gid, chip] of groupChips) {
    if (!groupOf(gid)) { chip.remove(); groupChips.delete(gid); }
  }
  for (const tab of S.tabs) {
    const node = tabEls.get(tab.id);
    if (!node) continue;
    const group = groupOf(tab.group);
    node.classList.toggle('grouped', !!group);
    node.classList.toggle('gcollapsed', !!group?.collapsed && tab.id !== S.active);
    if (group) node.style.setProperty('--gc', GROUP_COLORS[group.color] || group.color);
    else node.style.removeProperty('--gc');
    const split = S.split && (S.split.left === tab.id || S.split.right === tab.id);
    node.classList.toggle('in-split', !!split);
  }
  for (const group of groups) {
    const first = S.tabs.find((t) => t.group === group.id);
    if (!first) continue;
    let chip = groupChips.get(group.id);
    if (!chip) {
      chip = h('button', { class: 'gchip' });
      const gid = group.id;
      chip.addEventListener('click', () => send('group_edit', { group: gid, collapsed: !groupOf(gid)?.collapsed }));
      chip.addEventListener('contextmenu', (event) => { event.preventDefault(); openGroupMenu(gid, chip); });
      groupChips.set(group.id, chip);
    }
    chip.style.setProperty('--gc', GROUP_COLORS[group.color] || group.color);
    chip.classList.toggle('collapsed', !!group.collapsed);
    const count = S.tabs.filter((t) => t.group === group.id).length;
    chip.textContent = group.name || (group.collapsed ? String(count) : '');
    chip.classList.toggle('empty', !chip.textContent);
    chip.title = (group.name || 'Группа') + ` · вкладок: ${count}\nЩелчок — свернуть или развернуть, правая кнопка — меню`;
    const firstNode = tabEls.get(first.id);
    if (firstNode && chip.nextSibling !== firstNode) el.tabs.insertBefore(chip, firstNode);
  }
}

function openGroupMenu(gid, anchor) {
  const group = groupOf(gid);
  if (!group) return;
  const name = h('input', { type: 'text', class: 'field gname', placeholder: 'Название группы', value: group.name, spellcheck: 'false' });
  name.addEventListener('keydown', (event) => {
    event.stopPropagation();
    if (event.key === 'Enter') { send('group_edit', { group: gid, name: name.value }); closeMenu(); }
  });
  name.addEventListener('change', () => send('group_edit', { group: gid, name: name.value }));
  const colors = h('div', { class: 'gcolors' }, ...Object.entries(GROUP_COLORS).map(([key, value]) =>
    h('button', { class: 'swatch' + (group.color === key ? ' on' : ''), style: `--c:${value}`, title: key,
      onclick: () => { send('group_edit', { group: gid, color: key, name: name.value }); closeMenu(); } })));
  const nodes = [
    h('div', { class: 'gedit' }, name, colors),
    sep(),
    item({ icon: group.collapsed ? 'down' : 'up', label: group.collapsed ? 'Развернуть' : 'Свернуть',
      onClick: () => send('group_edit', { group: gid, collapsed: !group.collapsed }) }),
    item({ icon: 'plus', label: 'Новая вкладка в группе', onClick: () => send('group_new_tab', { group: gid }) }),
    item({ icon: 'x', label: 'Разгруппировать', onClick: () => send('group_ungroup', { group: gid }) }),
    item({ icon: 'trash', label: 'Закрыть группу', danger: true, onClick: () => send('group_close', { group: gid }) }),
  ];
  showMenu('group', nodes, () => {
    const r = anchor.getBoundingClientRect();
    el.menu.style.left = Math.max(8, Math.min(r.left, innerWidth - 300)) + 'px';
    el.menu.style.top = r.bottom + 8 + 'px';
  });
  setTimeout(() => name.focus(), 60);
}

/** Пункты групп и разделённого экрана для меню вкладки. */
function tabMenuExtras(tab) {
  const items = [];
  const others = (S.groups || []).filter((g) => g.id !== tab.group);
  items.push(item({ icon: 'plus', label: 'Добавить в новую группу', onClick: () => send('group_new', { id: tab.id }) }));
  for (const g of others) {
    items.push(item({ icon: 'folder', label: `В группу «${g.name || 'без названия'}»`, onClick: () => send('group_add', { id: tab.id, group: g.id }) }));
  }
  if (tab.group) items.push(item({ icon: 'x', label: 'Убрать из группы', onClick: () => send('group_remove', { id: tab.id }) }));
  items.push(sep());
  const inSplit = S.split && (S.split.left === tab.id || S.split.right === tab.id);
  if (inSplit) {
    items.push(item({ icon: 'split', label: 'Поменять половины местами', onClick: () => send('split_swap') }));
    items.push(item({ icon: 'x', label: 'Убрать разделение экрана', onClick: () => send('unsplit') }));
  } else if (tab.id !== S.active && !tab.start) {
    items.push(item({ icon: 'split', label: 'Открыть рядом с текущей', onClick: () => send('split', { id: tab.id }) }));
  }
  return items;
}

let groupJustCreated = 0;
function groupCreated(gid) {
  groupJustCreated = gid;
}

// ---------- разделённый экран ----------

$('#splitbtn').addEventListener('click', () => {
  const inSplit = S.split && (S.split.left === S.active || S.split.right === S.active);
  send(inSplit ? 'unsplit' : 'split');
});
function renderSplitButton() {
  const inSplit = !!S.split && (S.split.left === S.active || S.split.right === S.active);
  $('#splitbtn').classList.toggle('on', inSplit);
  $('#splitbtn').title = inSplit ? 'Убрать разделение экрана' : 'Разделить экран: эта вкладка и предыдущая рядом';
}


// ---------- установка и обновления ----------

function installCard() {
  const status = h('small', {});
  const installBtn = h('button', { class: 'text-btn primary', onclick: () => send('install') }, 'Установить в систему');
  const ready = h('div', { class: 'upd-ready' }, icon('check', 16), h('span', {}, 'Новая версия уже установлена — перезапустите браузер, вкладки вернутся'),
    h('button', { class: 'text-btn primary', onclick: () => send('restart_update') }, 'Перезапустить'));
  const source = h('small', {});
  settingsUpdaters.push(() => {
    const i = S.install || {};
    status.textContent = i.isInstalledCopy
      ? `Установлен в ${i.dir} · версия ${i.version}`
      : i.installed ? `Есть установленная копия в ${i.dir}, а эта запущена из ${i.exe}` : `Запущен из ${i.exe} · версия ${i.version}`;
    installBtn.style.display = i.isInstalledCopy ? 'none' : '';
    installBtn.textContent = i.installed ? 'Обновить установленную копию' : 'Установить в систему';
    ready.style.display = i.updateReady ? '' : 'none';
    source.textContent = S.settings.updateSource ? `Новые сборки берутся из ${S.settings.updateSource}` : 'Источник не выбран: укажите exe новой сборки (например, из папки проекта)';
  });
  return h('div', { class: 'card' }, h('h2', {}, 'Установка и обновления'),
    h('div', { class: 'set' }, h('div', { class: 'label' }, 'Rust Browser', status), h('div', { class: 'controls' }, installBtn)),
    ready,
    setting('Обновляться автоматически', 'Раз в час браузер проверяет источник и сам ставит новую версию — без закрытия, она начнёт работать со следующего запуска',
      toggle(() => S.settings.autoUpdate, (value) => set('auto_update', value))),
    h('div', { class: 'set' }, h('div', { class: 'label' }, 'Источник обновлений', source),
      h('div', { class: 'controls' },
        h('button', { class: 'text-btn outline', onclick: () => send('pick_update_source') }, 'Выбрать файл'),
        h('button', { class: 'text-btn outline', onclick: () => send('check_update') }, 'Проверить сейчас'))));
}

function updateReady() {
  toast('Новая версия установлена — перезапустите браузер, когда будет удобно');
}


// ---------- редактор сочетаний клавиш ----------

const KEY_ACTIONS = [
  ['new_tab', 'Новая вкладка', 'Ctrl+T'], ['close_tab', 'Закрыть вкладку', 'Ctrl+W'],
  ['reopen_tab', 'Вернуть закрытую вкладку', 'Ctrl+Shift+T'], ['new_window', 'Новое окно', 'Ctrl+N'],
  ['inprivate', 'Окно InPrivate', 'Ctrl+Shift+N'], ['focus_url', 'Адресная строка', 'Ctrl+L, Alt+D, F6'],
  ['next_tab', 'Следующая вкладка', 'Ctrl+Tab'], ['prev_tab', 'Предыдущая вкладка', 'Ctrl+Shift+Tab'],
  ['reload', 'Обновить', 'Ctrl+R, F5'], ['back', 'Назад', 'Alt+←'], ['forward', 'Вперёд', 'Alt+→'],
  ['bookmark', 'В избранное', 'Ctrl+D'], ['bookmarks', 'Избранное', 'Ctrl+Shift+O'],
  ['history', 'Журнал', 'Ctrl+H'], ['downloads', 'Загрузки', 'Ctrl+J'], ['settings', 'Очистить данные', 'Ctrl+Shift+Del'],
  ['find', 'Найти на странице', 'Ctrl+F'], ['zoom_in', 'Крупнее', 'Ctrl+='], ['zoom_out', 'Мельче', 'Ctrl+−'],
  ['print', 'Печать', 'Ctrl+P'], ['fullscreen', 'Во весь экран', 'F11'], ['devtools', 'Инструменты разработчика', 'F12'],
  ['favbar', 'Панель избранного', 'Ctrl+Shift+B'], ['task_manager', 'Диспетчер вкладок', 'Shift+Esc'],
  ['panel', 'Боковая панель', 'Ctrl+Shift+E'], ['pip', 'Картинка в картинке', 'Alt+P'],
  ['compact', 'Компактный режим', 'Ctrl+Shift+K'], ['split', 'Разделить экран', '—'],
  ['paste_go', 'Вставить и перейти', 'Ctrl+Shift+L'], ['menu', 'Меню', 'Alt+F'],
];
const KEY_NAMES = { 37: '←', 38: '↑', 39: '→', 40: '↓', 32: 'Пробел', 13: 'Enter', 9: 'Tab', 27: 'Esc', 46: 'Del', 8: 'Backspace',
  33: 'PageUp', 34: 'PageDown', 36: 'Home', 35: 'End', 45: 'Insert', 187: '=', 189: '−', 188: ',', 190: '.', 191: '/', 186: ';', 222: "'", 219: '[', 221: ']', 220: '\\', 192: '`' };
let recordingAction = null;

function comboLabel(event) {
  const key = event.keyCode;
  const name = KEY_NAMES[key] || (key >= 112 && key <= 123 ? 'F' + (key - 111) : (key >= 48 && key <= 90 ? String.fromCharCode(key) : event.key.toUpperCase()));
  return [event.ctrlKey && 'Ctrl', event.shiftKey && 'Shift', event.altKey && 'Alt', name].filter(Boolean).join('+');
}

function stopRecording() {
  recordingAction = null;
  send('keys_record', { on: false });
  removeEventListener('keydown', recordKey, true);
  settingsUpdaters.forEach((u) => u());
}

function recordKey(event) {
  event.preventDefault();
  event.stopPropagation();
  if (['Control', 'Shift', 'Alt', 'Meta'].includes(event.key)) return;
  if (event.key === 'Escape' && !event.ctrlKey && !event.altKey && !event.shiftKey) return stopRecording();
  const isF = event.keyCode >= 112 && event.keyCode <= 123;
  if (!event.ctrlKey && !event.altKey && !isF) {
    toast('Добавьте Ctrl или Alt — одиночные клавиши нужны сайтам');
    return;
  }
  const mods = (event.ctrlKey ? 1 : 0) | (event.shiftKey ? 2 : 0) | (event.altKey ? 4 : 0);
  const label = comboLabel(event);
  const list = (S.settings.shortcuts || []).filter((s) => s.action !== recordingAction);
  const taken = list.find((s) => s.key === event.keyCode && s.mods === mods)
    || KEY_ACTIONS.find(([id, , def]) => id !== recordingAction && !list.some((s) => s.action === id) && def.split(', ').includes(label));
  if (taken) {
    const name = KEY_ACTIONS.find(([id]) => id === (taken.action || taken[0]))?.[1];
    toast(`${label} уже занято: «${name}». Сначала поменяйте его там`);
    return;
  }
  set('shortcuts', [...list, { action: recordingAction, key: event.keyCode, mods, label }]);
  stopRecording();
}

function keysCards() {
  const rows = KEY_ACTIONS.map(([id, name, def]) => {
    // подпись сразу — по ней ищет поиск в параметрах
    const kbd = h('kbd', {}, (S.settings.shortcuts || []).find((s) => s.action === id)?.label || def);
    const change = h('button', { class: 'text-btn outline', onclick: () => {
      if (recordingAction) stopRecording();
      recordingAction = id;
      send('keys_record', { on: true });
      addEventListener('keydown', recordKey, true);
      settingsUpdaters.forEach((u) => u());
    } }, 'Изменить');
    const reset = h('button', { class: 'text-btn', onclick: () => set('shortcuts', (S.settings.shortcuts || []).filter((s) => s.action !== id)) }, 'Сбросить');
    settingsUpdaters.push(() => {
      const custom = (S.settings.shortcuts || []).find((s) => s.action === id);
      const recording = recordingAction === id;
      kbd.textContent = recording ? 'Нажмите сочетание… (Esc — отмена)' : custom ? custom.label : def;
      kbd.classList.toggle('rec', recording);
      kbd.classList.toggle('custom', !!custom && !recording);
      reset.style.display = custom ? '' : 'none';
    });
    return h('div', { class: 'set keyrow' }, h('div', { class: 'label' }, name), h('div', { class: 'controls' }, kbd, reset, change));
  });
  return [
    h('div', { class: 'card' }, h('h2', {}, 'Сочетания клавиш'),
      h('div', { class: 'ext-hint acc-intro' }, 'Нажмите «Изменить» и затем новое сочетание. Своё сочетание заменяет стандартное для этого действия. Ctrl+1…9 — вкладка по номеру'),
      ...rows,
      h('div', { class: 'set' }, h('div', { class: 'label' }), h('div', { class: 'controls' },
        h('button', { class: 'text-btn outline', onclick: () => set('shortcuts', []) }, 'Вернуть все стандартные')))),
  ];
}

// ---------- компактный режим ----------
// Панели свёрнуты в тонкую полоску сверху; наведение раскрывает их поверх страницы,
// а сама страница не сдвигается.

const compactOn = () => !!S.settings.compact && !activeTab()?.start && !vertical() && !document.body.classList.contains('fullscreen');
let compactOpen = false;
let compactTimer = 0;
const compactExtra = () => (compactOpen ? Math.max(0, Math.round(el.chrome.getBoundingClientRect().height) - 6) : 0);

function setCompactOpen(open) {
  clearTimeout(compactTimer);
  if (open === compactOpen) return;
  compactOpen = open;
  document.body.classList.toggle('compact-open', open);
  if (open) {
    requestAnimationFrame(() => { if (compactOpen && !layerKind) send('overlay', { mode: 'sheet', height: compactExtra() + (sheetOpen ? sheetHeight : 0) }); });
  } else if (!layerKind && !sheetOpen) {
    send('overlay', { mode: 'none' });
  }
}
function scheduleCompactClose() {
  clearTimeout(compactTimer);
  compactTimer = setTimeout(() => {
    if (document.activeElement === el.url || menuKind || sheetOpen || layerKind) return scheduleCompactClose();
    setCompactOpen(false);
  }, 600);
}
el.chrome.addEventListener('mouseenter', () => { if (compactOn()) setCompactOpen(true); });
el.chrome.addEventListener('mouseleave', () => { if (compactOn() && compactOpen) scheduleCompactClose(); });
el.url.addEventListener('focus', () => { if (compactOn()) setCompactOpen(true); });
el.url.addEventListener('blur', () => { if (compactOn() && compactOpen) scheduleCompactClose(); });

function renderCompact() {
  const on = compactOn();
  if (document.body.classList.contains('compact-mode') !== on) {
    document.body.classList.toggle('compact-mode', on);
    if (!on && compactOpen) setCompactOpen(false);
    requestAnimationFrame(reportChrome);
  }
}


// ---------- свой вид сайта («бусты») ----------

const BOOST_FONTS = ['Segoe UI Variable Text', 'Georgia', 'Cambria', 'Calibri', 'Bahnschrift', 'Consolas', 'Comic Sans MS', 'Verdana'];
const BOOST_TINTS = ['#3b82f6', '#8b5cf6', '#ec4899', '#ef4444', '#f97316', '#eab308', '#22c55e', '#06b6d4'];

function openBoostMenu() {
  const tab = activeTab();
  const host = hostOf(tab?.url || '').replace(/^www\./, '');
  if (!host || !/^https?:/.test(tab.url)) return toast('Свой вид можно задать только для сайта');
  const draw = () => {
    const b = (S.settings.boosts || {})[host] || { font: '', tint: '', hidden: [], css: '' };
    const fonts = h('div', { class: 'boost-chips' },
      h('button', { class: 'boost-chip' + (!b.font ? ' on' : ''), onclick: () => send('boost_set', { font: '' }) }, 'Как на сайте'),
      ...BOOST_FONTS.map((f) => h('button', { class: 'boost-chip' + (b.font === f ? ' on' : ''), style: `font-family:"${f}"`, onclick: () => send('boost_set', { font: f }) }, f.replace(' Variable Text', ''))));
    const tints = h('div', { class: 'gcolors' },
      h('button', { class: 'swatch none' + (!b.tint ? ' on' : ''), title: 'Без оттенка', onclick: () => send('boost_set', { tint: '' }) }),
      ...BOOST_TINTS.map((c) => h('button', { class: 'swatch' + (b.tint === c ? ' on' : ''), style: `--c:${c}`, onclick: () => send('boost_set', { tint: c }) })));
    const css = h('textarea', { class: 'field boost-css', placeholder: 'Свой CSS для сайта, например: body { font-size: 18px }', spellcheck: 'false' });
    css.value = b.css || '';
    css.addEventListener('keydown', (e) => e.stopPropagation());
    const hidden = b.hidden.map((s, index) => h('div', { class: 'boost-hidden' }, h('code', {}, s),
      h('button', { title: 'Показать снова', onclick: () => send('boost_set', { unhide: index }) }, '×')));
    return [
      h('div', { class: 'menu-title' }, `Свой вид: ${host}`),
      h('div', { class: 'boost-sec' }, h('small', {}, 'Шрифт'), fonts),
      h('div', { class: 'boost-sec' }, h('small', {}, 'Оттенок'), tints),
      h('div', { class: 'boost-sec' }, h('small', {}, `Скрытые элементы: ${b.hidden.length}`), ...hidden,
        h('button', { class: 'text-btn outline', onclick: () => { closeMenu(); setTimeout(() => send('boost_pick'), 150); } }, 'Скрыть элемент на странице')),
      h('div', { class: 'boost-sec' }, h('small', {}, 'Свой CSS'), css,
        h('div', { class: 'boost-row' },
          h('button', { class: 'text-btn primary', onclick: () => send('boost_set', { css: css.value }) }, 'Применить CSS'),
          h('button', { class: 'text-btn', onclick: () => send('boost_set', { reset: true }) }, 'Сбросить всё'))),
    ];
  };
  boostRedraw = () => { if (menuKind === 'boost') el.menu.replaceChildren(...draw()); };
  showMenu('boost', draw(), () => {
    const anchor = el.menubtn.getBoundingClientRect();
    el.menu.style.left = Math.max(8, Math.min(anchor.right - 360, innerWidth - 370)) + 'px';
    el.menu.style.top = anchor.bottom + 8 + 'px';
  });
}
let boostRedraw = () => {};
let boostKey = '';
function renderBoostMenu() {
  const key = JSON.stringify(S.settings.boosts || {});
  if (key !== boostKey) { boostKey = key; boostRedraw(); }
}


// ---------- адресная строка-кнопка ----------
// Если места мало (узкий остров, окно), строка сворачивается в круглую кнопку-лупу
// вместо обрезанного «Адр…». Щелчок или Ctrl+L — растягивается на всю панель.

const urlbox = $('#urlbox');
const toolbarEl = $('#toolbar');

/** Сколько места досталось бы строке, если бы она была обычной. */
function urlRoom() {
  let used = 0;
  const walk = (parent) => {
    for (const c of parent.children) {
      if (c === urlbox) continue;
      const cs = getComputedStyle(c);
      if (cs.display === 'contents') { walk(c); continue; }
      if (cs.display === 'none' || cs.position === 'absolute') continue;
      used += c.offsetWidth + parseFloat(cs.marginLeft) + parseFloat(cs.marginRight);
    }
  };
  walk(toolbarEl);
  const cs = getComputedStyle(toolbarEl);
  const gap = parseFloat(cs.columnGap) || 0;
  return toolbarEl.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight) - used - gap * toolbarEl.children.length - 12;
}

function measureUrl() {
  if (urlbox.classList.contains('expanded')) return;
  // в одной строке с вкладками: колонка панели растёт за счёт вкладок, пока строке не хватит ~260px
  if (document.body.classList.contains('bar-compact')) {
    el.chrome.style.setProperty('--tool-need', Math.round(toolbarEl.clientWidth - urlRoom() + 260) + 'px');
  }
  const collapse = !vertical() && toolbarEl.clientWidth > 0 && urlRoom() < 200;
  document.body.classList.toggle('url-collapsed', collapse);
}
// размер острова поменялся (ширина в параметрах, окно) — пересчитать и строку, и форму окна
new ResizeObserver(() => requestAnimationFrame(() => { measureUrl(); reportChrome(); })).observe(toolbarEl);

function expandUrl() {
  if (!document.body.classList.contains('url-collapsed') || urlbox.classList.contains('expanded')) return;
  // строка вырастает из кнопки: запоминаем, где кнопка стоит, — оттуда и раскрытие
  const bar = toolbarEl.getBoundingClientRect();
  const btn = urlbox.getBoundingClientRect();
  urlbox.style.setProperty('--from-l', Math.max(0, btn.left - bar.left - 6) + 'px');
  urlbox.style.setProperty('--from-r', Math.max(0, bar.right - btn.right - 6) + 'px');
  urlbox.classList.add('expanded');
}
function collapseUrl() {
  if (!urlbox.classList.contains('expanded')) return;
  urlbox.classList.remove('expanded');
  requestAnimationFrame(measureUrl);
}
urlbox.addEventListener('mousedown', (event) => {
  if (!document.body.classList.contains('url-collapsed') || urlbox.classList.contains('expanded')) return;
  event.preventDefault();
  expandUrl();
  el.url.focus();
  el.url.select();
  urlJustExpanded = true;
});
// щелчок по свёрнутой строке только раскрывает её, а не открывает сведения о сайте
let urlJustExpanded = false;
urlbox.addEventListener('click', (event) => {
  if (!urlJustExpanded) return;
  urlJustExpanded = false;
  event.stopPropagation();
}, true);
el.url.addEventListener('focus', expandUrl);
el.url.addEventListener('blur', () => setTimeout(() => {
  if (document.activeElement !== el.url && !menuKind) collapseUrl();
}, 150));

// ---------- заставка запуска ----------

const LOGO = `<svg class="logo" viewBox="0 0 512 512" aria-hidden="true"> <defs> <linearGradient id="rb-bg" x1="0" y1="0" x2="1" y2="1"> <stop offset="0" stop-color="#ff7a3d"/> <stop offset=".5" stop-color="#e2338f"/> <stop offset="1" stop-color="#6a3cff"/> </linearGradient> <linearGradient id="rb-trail1" x1="0" y1="1" x2="1" y2="0"> <stop offset="0" stop-color="#ffffff" stop-opacity="0"/> <stop offset="1" stop-color="#ffffff" stop-opacity=".95"/> </linearGradient> <linearGradient id="rb-trail2" x1="0" y1="1" x2="1" y2="0"> <stop offset="0" stop-color="#ffd27a" stop-opacity="0"/> <stop offset="1" stop-color="#ffd27a" stop-opacity=".9"/> </linearGradient> <linearGradient id="rb-trail3" x1="0" y1="1" x2="1" y2="0"> <stop offset="0" stop-color="#9fd8ff" stop-opacity="0"/> <stop offset="1" stop-color="#9fd8ff" stop-opacity=".9"/> </linearGradient> <linearGradient id="rb-cursor" x1="250.97731" y1="60.506091" x2="250.97731" y2="376.03415" gradientTransform="scale(0.80246313,1.2461632)" gradientUnits="userSpaceOnUse"> <stop offset="0" stop-color="#ffffff"/> <stop offset="1" stop-color="#f1e9ff"/> </linearGradient> <filter id="rb-shadow" x="-0.21" y="-0.12" width="1.44" height="1.29" style="color-interpolation-filters:sRGB"> <feFlood flood-opacity="0.5" flood-color="rgb(43,10,92)" result="flood"/> <feGaussianBlur in="SourceGraphic" stdDeviation="14" result="blur"/> <feOffset in="blur" dx="4" dy="14" result="offset"/> <feComposite in="flood" in2="offset" operator="in" result="comp1"/> <feComposite in="SourceGraphic" in2="comp1" operator="over"/> </filter> <filter id="rb-glow" x="-0.9" y="-1.4" width="2.8" height="3.7" style="color-interpolation-filters:sRGB"> <feGaussianBlur stdDeviation="124.8"/> </filter> <clipPath id="rb-iconclip"><rect x="20" y="20" width="472" height="472" rx="128"/></clipPath> </defs> <rect class="sq" x="20" y="20" width="472" height="472" rx="128" fill="url(#rb-bg)"/> <g clip-path="url(#rb-iconclip)"> <ellipse cx="215.4" cy="151.5" rx="169.9" ry="113.4" fill="#fff" opacity=".16" filter="url(#rb-glow)"/> </g> <g class="trails"> <path class="trail t1" pathLength="100" d="M92 236 L262 174" stroke="url(#rb-trail2)" stroke-width="22" stroke-linecap="round" fill="none"/> <path class="trail t2" pathLength="100" d="M70 330 L292 262" stroke="url(#rb-trail1)" stroke-width="30" stroke-linecap="round" fill="none"/> <path class="trail t3" pathLength="100" d="M128 410 L300 290" stroke="url(#rb-trail3)" stroke-width="22" stroke-linecap="round" fill="none"/> </g> <g class="arrow"> <path d="m 250,112 164,188 -82,6 48,90 -42,22 -46,-92 -54,58 z" fill="url(#rb-cursor)" stroke="#2b1a5c" stroke-opacity="0.18" stroke-width="6" stroke-linejoin="round" filter="url(#rb-shadow)" transform="matrix(0.28729692,0.7893418,-0.7893418,0.28729692,432.46762,-59.048619)"/> </g> </svg>`;

const SPLASHES = {
  compass: { name: 'Комета', ms: 1350 },
  aurora: { name: 'Сияние', ms: 1450 },
  warp: { name: 'Гиперпрыжок', ms: 1300 },
  minimal: { name: 'Минимализм', ms: 750 },
};

/** Звёзды летят навстречу — рисуем на холсте, пока идёт заставка. */
function warpStars(canvas, until) {
  const ctx = canvas.getContext('2d');
  const dpr = devicePixelRatio || 1;
  const w = (canvas.width = innerWidth * dpr);
  const hgt = (canvas.height = innerHeight * dpr);
  const stars = Array.from({ length: 420 }, () => ({ x: Math.random() * 2 - 1, y: Math.random() * 2 - 1, z: Math.random() }));
  const start = performance.now();
  const frame = (now) => {
    if (now > until || !canvas.isConnected) return;
    const t = (now - start) / 1000;
    const speed = 0.004 + Math.min(t, 1) ** 2 * 0.05; // разгон
    ctx.fillStyle = 'rgba(3, 4, 12, .32)';
    ctx.fillRect(0, 0, w, hgt);
    ctx.lineCap = 'round';
    for (const s of stars) {
      const pz = s.z;
      s.z -= speed;
      if (s.z <= 0.02) { s.x = Math.random() * 2 - 1; s.y = Math.random() * 2 - 1; s.z = 1; continue; }
      const k = 0.5 / s.z, pk = 0.5 / pz;
      const x = w / 2 + s.x * k * w / 2, y = hgt / 2 + s.y * k * hgt / 2;
      const px = w / 2 + s.x * pk * w / 2, py = hgt / 2 + s.y * pk * hgt / 2;
      ctx.strokeStyle = `hsla(${230 + s.x * 60}, 90%, ${70 + (1 - s.z) * 30}%, ${1 - s.z})`;
      ctx.lineWidth = (1 - s.z) * 3 * dpr;
      ctx.beginPath(); ctx.moveTo(px, py); ctx.lineTo(x, y); ctx.stroke();
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

let splashRun = 0;
/** Играет заставку; preview — показ из параметров, тогда Rust не трогаем. */
function playSplash(kind, preview = false) {
  const box = $('#splash');
  const info = SPLASHES[kind];
  if (!info) { if (!preview) send('splash_done'); return; }
  box.className = 'show s-' + kind;
  const title = h('div', { class: 'stitle' }, ...[...'Rust Browser'].map((ch, i) =>
    h('span', { style: `--i:${i}` }, ch === ' ' ? ' ' : ch)));
  box.replaceChildren(
    h('div', { class: 'sbg' }, h('i'), h('i'), h('i')),
    kind === 'warp' ? h('canvas', { class: 'swarp' }) : '',
    h('div', { class: 'scenter' }, h('div', { class: 'slogo' }), title, h('div', { class: 'sbar' }, h('i'))));
  box.querySelector('.slogo').innerHTML = LOGO;
  if (kind === 'warp') warpStars(box.querySelector('canvas'), performance.now() + info.ms + 600);
  let done = false;
  const run = ++splashRun;
  const finish = () => {
    if (done) return;
    done = true;
    removeEventListener('keydown', finish, true);
    // поверх уже играет следующая заставка (в параметрах щёлкнули другую) — её не обрываем
    if (run !== splashRun) { if (!preview) send('splash_done'); return; }
    box.classList.add('leave');
    setTimeout(() => {
      if (!preview) send('splash_done');
      if (run !== splashRun) return;
      box.className = '';
      box.replaceChildren();
    }, 480);
  };
  box.onmousedown = finish;
  addEventListener('keydown', finish, true);
  setTimeout(finish, info.ms);
}

function splashSetting() {
  const show = h('button', { class: 'text-btn outline', onclick: () => playSplash(S.settings.splash, true) }, 'Показать');
  // без заставки показывать нечего
  settingsUpdaters.push(() => { show.disabled = !SPLASHES[S.settings.splash]; });
  return setting('Заставка при запуске', 'Короткая анимация, пока открываются вкладки. Щелчок или клавиша пропускают её',
    segmented([...Object.entries(SPLASHES).map(([id, s]) => [id, s.name]), ['off', 'Без заставки']],
      () => S.settings.splash, (value) => { set('splash', value); if (value !== 'off') playSplash(value, true); }),
    show);
}

if (BOOT.splash && BOOT.splash !== 'off') {
  try { playSplash(BOOT.splash); } catch (e) { $('#splash').className = ''; send('splash_done'); }
}

send('ready');
