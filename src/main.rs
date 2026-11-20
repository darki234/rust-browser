#![recursion_limit = "256"]
// В release-сборке не открываем консольное окно.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adblock;
mod efficiency;
mod extension;
mod platform;
mod pages;
mod passwords;
mod webstore;
mod spell;
mod sync;
mod icons;
mod autofill;
mod installer;
mod store;
mod system;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::{json, Value};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::windows::{IconExtWindows, WindowBuilderExtWindows, WindowExtWindows},
    window::{Fullscreen, Icon, ResizeDirection, Theme as WindowTheme, Window, WindowBuilder},
};
use wry::{
    dpi::{LogicalPosition, LogicalSize},
    http::Request,
    NewWindowResponse, PageLoadEvent, Rect, Theme, WebContext, WebView, WebViewBuilder,
    WebViewBuilderExtWindows, WebViewExtWindows,
};

use platform::{Hotkey, KeyState};
use store::{Session, SessionTab, Store};

const APP_NAME: &str = "Rust Browser";
/// Высота панели до того, как интерфейс сообщит настоящую.
const DEFAULT_CHROME: f64 = 86.0;
/// Минимальный промежуток между обновлениями интерфейса. На страницах с рекламой
/// блокировщик срабатывает сотни раз в секунду — перерисовывать панель на каждый раз незачем.
const PUSH_INTERVAL: Duration = Duration::from_millis(40);
/// Как часто проверять, не пора ли усыпить фоновые вкладки.
const SLEEP_CHECK: Duration = Duration::from_secs(30);
const TASKS_EVERY: Duration = Duration::from_secs(2);

/// Поисковые системы: идентификатор, название, адрес запроса. Первая — по умолчанию.
const ENGINES: [(&str, &str, &str); 4] = [
    ("google", "Google", "https://www.google.com/search?q="),
    ("bing", "Bing", "https://www.bing.com/search?q="),
    ("yandex", "Яндекс", "https://yandex.ru/search/?text="),
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q="),
];

/// Цвет под страницами, пока они не отрисовались: тёмный в тёмной теме.
/// Без него при переключении вкладок мелькает белое окно.
fn backdrop_color(dark: bool) -> (u8, u8, u8, u8) {
    if !dark {
        return (255, 255, 255, 255);
    }
    let rgb = THEME_BACKDROP.load(Ordering::Relaxed);
    ((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

/// Темы оформления. Кроме «как в системе», светлой, тёмной и AMOLED — плоские тёмные
/// палитры в стиле Lumi (Lumi, Ember, Moss, Steel) и Ryoku: без градиентов, с тонкими рамками.
const THEMES: [&str; 9] = ["system", "light", "dark", "amoled", "lumi", "ember", "moss", "steel", "ryoku"];

/// Тёмная ли тема: Some(true/false) или None — «как в системе».
fn theme_dark(theme: &str) -> Option<bool> {
    match theme {
        "light" => Some(false),
        "system" | "" => None,
        _ => Some(true),
    }
}

/// Фон окна под страницами для темы (0xRRGGBB) — совпадает с фоном интерфейса.
fn theme_backdrop(theme: &str) -> u32 {
    match theme {
        "amoled" | "lumi" | "ryoku" => 0x000000,
        "ember" => 0x0e0b0a,
        "moss" => 0x0a100e,
        "steel" => 0x0c0e12,
        _ => 0x121318,
    }
}

/// Фон под страницами для текущей тёмной темы.
static THEME_BACKDROP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x121318);

/// Тема AMOLED: под страницами чистый чёрный, а не тёмно-серый.
static AMOLED: AtomicBool = AtomicBool::new(false);
/// Последний снимок страницы под меню и библиотекой (JPEG) — его отдаёт rb.localhost.
static BACKDROP: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static BACKDROP_VERSION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Снимки вкладок для предпросмотра при наведении: номер вкладки → JPEG.
static TAB_SHOTS: std::sync::LazyLock<Mutex<HashMap<u32, Vec<u8>>>> = std::sync::LazyLock::new(Default::default);
/// Ссылки сайтов «в новой вкладке» — в фоне (настройка; читается из обработчика WebView2).
static LEFT_CLICK_BACKGROUND: AtomicBool = AtomicBool::new(false);

/// Щель между половинами разделённого экрана — за неё тянут разделитель.
const SPLIT_GAP: f64 = 6.0;

/// Ширина боковой панели и высота её шапки.
const PANEL_WIDTH: f64 = 380.0;
const PANEL_HEADER: f64 = 44.0;
/// Приложения боковой панели: заметки и калькулятор — сама панель, мессенджеры — сайты.
const PANEL_APPS: [(&str, &str); 4] = [
    ("notes", ""),
    ("calc", ""),
    ("telegram", "https://web.telegram.org/k/"),
    ("whatsapp", "https://web.whatsapp.com/"),
];

/// Ступени масштаба, как в Edge.
const ZOOM_LEVELS: [f64; 15] =
    [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

/// Адрес запроса выбранной системы; неизвестный идентификатор даёт систему по умолчанию.
fn search_url(engine: &str) -> &'static str {
    ENGINES.iter().find(|(id, ..)| *id == engine).unwrap_or(&ENGINES[0]).2
}

/// Всё, что приходит в главный цикл от интерфейса и страниц.
pub enum UserEvent {
    /// JSON-команда от интерфейса.
    Ui(String),
    UrlChanged(u32, String),
    TitleChanged(u32, String),
    Loading(u32, bool),
    HistoryChanged(u32),
    /// Ссылка «в новом окне»; true — открыть в фоне (Ctrl или колёсико).
    NewTabRequested(String, bool),
    Blocked(u32),
    Hotkey(Hotkey),
    Download(DownloadEvent),
    Favicon(u32, String),
    Audio(u32, bool),
    Zoom(u32, f64),
    PageFullscreen(u32, bool),
    /// (вкладка, номер текущего совпадения с единицы или -1, всего совпадений)
    Find(u32, i32, i32),
    /// Снимок страницы для фона меню и библиотеки.
    Captured(Option<Vec<u8>>),
    /// JSON-команда от боковой панели.
    Panel(String),
    /// uBlock Origin Lite распакован в эту папку.
    ExtensionReady(PathBuf),
    /// uBlock Origin Lite установлен в профиль; id нужен для включения и настроек.
    ExtensionInstalled(Option<String>),
    /// Другой запуск браузера передал адреса (пусто — просьба открыть новое окно).
    Remote(Vec<String>),
    /// Прокрутка вкладки в момент ухода в фон.
    Scrolled(u32, f64),
    /// Сообщение от страницы вкладки: (вкладка, настоящий адрес страницы, JSON).
    Page(u32, String, String),
    /// Windows Hello ответил: (номер пароля, скопировать, итог).
    Verified(usize, bool, passwords::Verdict),
    /// Расширение распаковано (или ошибка: id, текст); вкладка магазина, откуда ставили.
    ExtReady(Result<webstore::Installed, (String, String)>, Option<u32>),
    /// Профиль ответил на установку расширения.
    ExtAdded(webstore::Installed, Result<String, String>, Option<u32>),
    /// Список расширений профиля.
    ExtList(Vec<platform::ProfileExtension>),
    /// Расширение удалено или включено — перечитать список.
    ExtChanged,
    ExtPopupShow,
    /// Снимок вкладки, с которой ушли, — для предпросмотра при наведении.
    TabShot(u32, Option<Vec<u8>>),
    /// Снять вкладку, если она всё ещё на экране.
    ShotActive(u32),
    /// Перевод выделенного текста готов: (вкладка, номер запроса, итог).
    SelTranslated(u32, u64, Result<(String, String, String), String>),
    /// Исправленный запрос адресной строки (номер запроса, исправление).
    SpellFix(u64, Option<(String, spell::Fix)>),
    ExtPopupMeasure,
    ExtPopupSize(f64, f64),
    /// Итог проверки паролей на утечки, повторы и простоту.
    PwHealth(Vec<autofill::Health>),
    /// Windows Hello ответил на просьбу подставить карту (номер карты, сайт на экране в тот миг, итог).
    CardVerified(usize, String, passwords::Verdict),
    /// Щелчок внутри вкладки — при разделённом экране она становится активной.
    PageFocused(u32),
    /// Найдена и поставлена новая версия браузера (или ошибка).
    Updated(Result<bool, String>),
    /// Упал весь движок WebView2 — перезапуск браузера.
    EngineCrashed,
    /// Язык открытой страницы (lang у html).
    PageLang(u32, String),
    /// Появился новый значок сайта — перерисовать плитки.
    IconSaved,
    /// Телефон пришёл по Wi‑Fi со своим пакетом — ответ уходит обратно в канал.
    LanSync(Vec<u8>, std::sync::mpsc::Sender<Option<Vec<u8>>>),
    /// С диска WebDAV скачан файл связки (или его там ещё нет, или ошибка); bool — по кнопке.
    DavFetched(Result<(Option<sync::Bundle>, String), String>, bool),
    /// Файл записан на диск: новая метка версии; отпечатки, ручная ли, сколько разделов поменялось.
    DavStored(Result<String, String>, std::collections::BTreeMap<String, String>, bool, usize),
    /// Метка версии файла на диске — поменялась ли.
    DavEtag(Option<String>),
    /// Проверка диска при подключении.
    DavChecked(Result<(), String>, sync::WebDav),
    /// Процессы WebView2; true — показать их в диспетчере вкладок.
    Processes(Vec<efficiency::ProcessInfo>, bool),
    /// Страница не загрузилась: (вкладка, вид ошибки для своей страницы ошибки).
    LoadFailed(u32, &'static str),
    /// Картинка значка вкладки в PNG — от самого движка, без повторной загрузки с сайта.
    FaviconImage(u32, Vec<u8>),
    /// Тихий повтор загрузки после обрыва связи: (вкладка, адрес).
    RetryLoad(u32, String),
}

pub enum DownloadEvent {
    Started { id: u64, url: String, path: String },
    Progress { id: u64, received: i64, total: i64 },
    /// "progress" | "paused" | "done" | "canceled" | "failed"
    State { id: u64, state: &'static str, received: i64 },
}

struct PasswordOffer {
    tab: u32,
    origin: String,
    username: String,
    password: String,
}

struct Tab {
    id: u32,
    /// None и пустой url — стартовая страница. None и непустой url — вкладка
    /// из прошлого сеанса или открытая в фоне: загрузится, когда её откроют.
    webview: Option<WebView>,
    url: String,
    title: String,
    favicon: String,
    loading: bool,
    blocked: u32,
    can_back: bool,
    can_forward: bool,
    pinned: bool,
    /// Из какой вкладки открыта в фоне — чтобы ставить такие вкладки подряд.
    opener: Option<u32>,
    zoom: f64,
    audio: bool,
    muted: bool,
    sleeping: bool,
    /// Когда вкладка ушла в фон — для усыпления.
    hidden_since: Option<Instant>,
    /// Прокрутка на момент ухода в фон: вернём её, если вкладку выгрузят.
    scroll: Option<f64>,
    /// Прокрутить сюда после повторной загрузки выгруженной вкладки.
    restore_scroll: Option<f64>,
    /// Показана своя страница ошибки: в журнал такое не пишем.
    failed: bool,
    /// На странице есть поле пароля — показываем ключ, если для сайта есть пароли.
    pw_form: bool,
    /// На странице есть видео — для «картинки в картинке».
    video: bool,
    /// Сколько раз подряд адрес уже перезапрашивали после обрыва и когда в последний раз.
    retries: (String, u8, Option<Instant>),
    /// Язык страницы — чтобы предложить перевод.
    lang: String,
    /// На странице поля карты / адреса — у адреса появляется кнопка автозаполнения.
    af_card: bool,
    af_address: bool,
    /// id скрипта тёмной темы для сайтов в этой вкладке (его снимают при смене настройки).
    dark_script: Arc<Mutex<Option<String>>>,
    /// Скрипт с уровнями усиления звука по сайтам (меняется на лету).
    boost_script: Arc<Mutex<Option<String>>>,
    /// Группа вкладки (0 — без группы).
    group: u32,
}

impl Tab {
    fn new(id: u32, url: &str, title: &str, pinned: bool) -> Self {
        Self {
            id,
            webview: None,
            url: url.to_string(),
            title: title.to_string(),
            favicon: String::new(),
            loading: false,
            blocked: 0,
            can_back: false,
            can_forward: false,
            pinned,
            opener: None,
            zoom: 1.0,
            audio: false,
            muted: false,
            sleeping: false,
            hidden_since: None,
            scroll: None,
            restore_scroll: None,
            failed: false,
            pw_form: false,
            video: false,
            retries: (String::new(), 0, None),
            lang: String::new(),
            dark_script: Default::default(),
            boost_script: Default::default(),
            af_card: false,
            af_address: false,
            group: 0,
        }
    }

    fn is_start(&self) -> bool {
        self.webview.is_none() && self.url.is_empty()
    }

    fn snapshot(&self) -> SessionTab {
        SessionTab { url: self.url.clone(), title: self.title.clone(), pinned: self.pinned, group: self.group }
    }
}

#[derive(Serialize, serde::Deserialize, Clone)]
struct Download {
    id: u64,
    url: String,
    name: String,
    path: String,
    received: i64,
    total: i64,
    /// "progress" | "paused" | "done" | "canceled" | "failed"
    state: String,
}

const MAX_SAVED_DOWNLOADS: usize = 100;

/// Сколько места под панелью интерфейс занимает поверх страницы.
#[derive(Clone, Copy, PartialEq)]
enum Overlay {
    None,
    /// Выпадающий список подсказок такой высоты.
    Sheet(f64),
    /// Меню или библиотека на всё окно, поверх снимка страницы.
    Full,
}

struct App {
    window: Window,
    ui: WebView,
    proxy: EventLoopProxy<UserEvent>,
    /// Общий профиль WebView2 (куки, кэш, пароли) для всех вкладок.
    web_context: WebContext,
    store: Store,
    inprivate: bool,
    /// Основное окно: восстанавливает и сохраняет сеанс, принимает ссылки от Windows.
    primary: bool,
    /// Идёт заставка запуска: интерфейс на всё окно, пока он не скажет «splash_done».
    splash: bool,
    is_default: bool,
    /// Режим эффективности, уже выставленный процессам: pid → включён.
    eco: HashMap<u32, bool>,
    processes: Vec<efficiency::ProcessInfo>,
    on_battery: bool,
    /// Браузеры, из которых можно перенести избранное.
    import_sources: Vec<(&'static str, &'static str)>,
    /// Ссылка на загрузку файла тоже «не загружается как страница» — в этот момент
    /// страницу ошибки показывать не нужно.
    last_download: Option<Instant>,
    /// id установленного uBlock Origin Lite.
    ubol_id: Option<String>,
    ubol_folder: Option<PathBuf>,
    /// Поставленные нами расширения (папка, окошко, страница настроек) — extensions.json.
    extensions: Vec<webstore::Installed>,
    /// Что на самом деле стоит в профиле: (id, имя, включено).
    ext_list: Vec<platform::ProfileExtension>,
    ext_queue: std::collections::VecDeque<(webstore::Installed, Option<u32>)>,
    ext_installing: bool,
    ext_busy: std::collections::HashSet<String>,
    /// Открытое окошко расширения: webview, правый край и верх кнопки.
    ext_popup: Option<(WebView, f64, f64)>,
    /// Закрытое окошко расширения не уничтожается: создание WebView подвешивает окно
    /// на полсекунды, а готовое открывается мгновенно.
    ext_popup_spare: Option<WebView>,
    /// Карточка предпросмотра вкладки (x, y, ширина, высота) — окно интерфейса её не обрежет.
    peek: Option<[f64; 4]>,
    /// Высота видимой части окна интерфейса (для проверок).
    shape_height: std::cell::Cell<f64>,
    /// Исправление опечаток и раскладки — в своём потоке.
    spell: spell::Worker,
    /// Версии снимков вкладок: интерфейс по ним понимает, что картинка обновилась.
    shot_versions: HashMap<u32, u32>,
    /// Найденные браузеры с расширениями: (название, сколько можно перенести).
    ext_browsers: Vec<(String, usize)>,
    /// Расширения, которые ставит синхронизация: об ошибках не сообщаем.
    ext_quiet: std::collections::HashSet<String>,
    /// Предзагрузка с плитки: (вкладка, адрес, скрытый webview, который уже грузит сайт).
    prewarm: Option<(u32, String, WebView)>,
    /// Группы вкладок.
    groups: Vec<store::TabGroup>,
    /// Разделённый экран: (левая вкладка, правая вкладка, доля левой половины).
    split: Option<(u32, u32, f64)>,
    /// Тянут разделитель половин экрана.
    split_drag: bool,
    /// Последнее положение мыши в окне (физические пиксели) — для разделителя.
    cursor: (f64, f64),
    /// Обновление: поставлено, ждёт перезапуска; когда проверяли.
    update_ready: bool,
    last_update_check: Instant,
    /// Адреса и карты для автозаполнения (на диске — под DPAPI).
    autofill: autofill::Profile,
    /// Результат проверки паролей (по номерам в хранилище) и идёт ли проверка.
    pw_health: Vec<autofill::Health>,
    /// autofill.json есть, но не расшифровывается — не перезаписываем его.
    autofill_locked: bool,
    pw_checking: bool,
    /// Синхронизация (None — выключена).
    account: Option<sync::Account>,
    sync_error: Option<String>,
    last_sync: Instant,
    /// Порт сервера в домашней сети (None — ещё не запущен).
    lan_port: Option<u16>,
    lan_group: Arc<Mutex<String>>,
    lan_rev: Arc<std::sync::atomic::AtomicU64>,
    /// Отпечаток того, что нужно телефону, — вырос, значит телефону пора прийти.
    lan_fp: String,
    /// Идёт обмен с диском WebDAV / проверка метки.
    dav_busy: bool,
    dav_checked: Instant,
    /// Вкладки, отправленные между компьютером и телефоном (раздел синхронизации sent_tabs).
    sent_tabs: Vec<Value>,
    /// Какие из пришедших сюда уже открыты — второй раз не откроются.
    sent_handled: std::collections::HashSet<String>,
    /// Сохранённые пароли (зашифрованы) и сайты «никогда не сохранять».
    vault: passwords::Vault,
    /// Пароль, который только что ввели на сайте: ждёт ответа «Сохранить?».
    pw_offer: Option<PasswordOffer>,
    /// Когда последний раз подтверждали личность: минуту пароли можно смотреть без повтора.
    pw_verified: Option<Instant>,
    tabs: Vec<Tab>,
    active: u32,
    next_id: u32,
    /// Закрытые вкладки для Ctrl+Shift+T.
    closed: Vec<SessionTab>,
    /// Значки вкладок, сохранённые на диск: хост → (версия, размер файла).
    tab_icons: HashMap<String, (u32, usize)>,
    /// Группы, опустевшие после закрытия вкладок, — для восстановления.
    closed_groups: Vec<store::TabGroup>,
    downloads: Vec<Download>,
    adblock: Arc<AtomicBool>,
    download_dir: Arc<Mutex<String>>,
    keys: Arc<KeyState>,
    overlay: Overlay,
    /// Ждём снимок страницы, чтобы показать меню поверх него.
    overlay_pending: bool,
    /// Когда запросили снимок: если он не пришёл быстро, меню открывается без него.
    overlay_requested: Instant,
    chrome_height: f64,
    /// Ширина боковой колонки при вертикальных вкладках; 0 — панель сверху.
    chrome_side: f64,
    /// «Остров» поверх страницы: страница начинается с этой высоты (под вкладками),
    /// а интерфейс виден только в этих местах (x, y, ширина, высота, радиус — в CSS-пикселях).
    chrome_float: Option<(f64, Vec<[f64; 5]>)>,
    /// Размер показанного уведомления (ширина, высота) — окно интерфейса его не обрежет.
    toast: Option<(f64, f64)>,
    /// Боковая панель справа (создаётся при первом открытии).
    panel: Option<WebView>,
    /// Мессенджеры в боковой панели: живут, пока окно открыто, чтобы не входить заново.
    panel_apps: HashMap<&'static str, WebView>,
    /// Полноэкранный режим по F11.
    fullscreen: bool,
    /// Полноэкранный режим, который запросила страница (видео).
    page_fullscreen: bool,
    maximized: bool,
    /// Состояние изменилось, интерфейс надо обновить.
    dirty: bool,
    /// Когда интерфейс обновляли последний раз: чаще 25 раз в секунду не нужно.
    last_push: Instant,
    /// Последние отправленные редко меняющиеся части состояния (закладки, загрузки…):
    /// без изменений их не пересылаем и интерфейс их не перерисовывает.
    sent: std::collections::HashMap<&'static str, String>,
    history_dirty: bool,
    session_dirty: bool,
    last_save: Instant,
    last_sleep_check: Instant,
    /// Диспетчер задач открыт: раз в 2 секунды обновляем список, а интерфейсу
    /// отправляем, только если что-то поменялось.
    tasks_watch: bool,
    /// Страница боковой панели заморожена, пока панель закрыта.
    panel_sleeping: bool,
    /// Окно свёрнуто: видимые вкладки и интерфейс в режиме низкой памяти.
    minimized: bool,
    /// «Только сайт»: панели спрятаны до наведения на верх окна (на время сеанса).
    site_only: bool,
    last_tasks: Instant,
    tasks_sent: std::cell::RefCell<String>,
    quit: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // ссылка из Windows идёт после --single-argument и флагом стать не может
    let (args, addresses) = system::split_args(&args);
    let inprivate = args.iter().any(|arg| arg == "--inprivate");
    let new_window = args.iter().any(|arg| arg == "--new-window");
    // «Параметры → Приложения → Удалить»
    if args.iter().any(|arg| arg == "--uninstall") {
        installer::uninstall();
        return Ok(());
    }
    installer::cleanup_old();
    // перезапуск: сначала дождаться, пока прежний процесс закроется
    if let Some(list) = args.iter().find_map(|a| a.strip_prefix("--wait-pid=")) {
        for pid in list.split(',').filter_map(|p| p.parse::<u32>().ok()) {
            system::wait_for_exit(pid, Duration::from_secs(15));
        }
    }
    let urls: Vec<String> = addresses.iter().map(|arg| system::arg_to_address(arg)).collect();

    // Браузер уже открыт — отдаём ему адреса (ссылку из Telegram и т. п.) и выходим.
    let pipe = system::pipe_name(&data_dir());
    if !inprivate && !new_window && system::forward_to_running(&pipe, &urls) {
        return Ok(());
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let remote = proxy.clone();
    let primary = !inprivate
        && !new_window
        && system::serve(&pipe, move |urls| {
            let _ = remote.send_event(UserEvent::Remote(urls));
        });

    let title = if inprivate { format!("InPrivate — {APP_NAME}") } else { APP_NAME.to_string() };
    let window = WindowBuilder::new()
        .with_title(title)
        // Значок из ресурсов exe (номер 1, см. build.rs) — для заголовка и панели задач.
        .with_window_icon(Icon::from_resource(1, None).ok())
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(560.0, 380.0))
        // Своя полоса заголовка: вкладки прямо в рамке окна, как в Edge.
        .with_decorations(false)
        .with_undecorated_shadow(true)
        // фон самого окна виден в щелях между вкладками при переключении — не белый
        .with_background_color(backdrop_color(window_dark_hint()))
        .build(&event_loop)?;

    let store = Store::load(data_dir());
    let dark = theme_dark(&store.settings.theme).unwrap_or(window.theme() == WindowTheme::Dark);
    let keys = Arc::new(KeyState::default());
    apply_shortcuts(&keys, &store.settings.shortcuts);

    // Профиль лежит в папке данных, а не рядом с exe: программу можно
    // переносить и запускать откуда угодно. У InPrivate профиль отдельный.
    let profile = if inprivate { "WebView2-InPrivate" } else { "WebView2" };
    let mut web_context = WebContext::new(Some(data_dir().join(profile)));
    let network = proxy_config(&store.settings);
    PROCESS_LIMIT.store(store.settings.process_limit, Ordering::Relaxed);
    // прокси меняется только с перезапуском — значкам сайтов хватает значения при старте
    icons::set_proxy(match store.settings.proxy_mode.as_str() {
        "http" | "socks5" if valid_proxy_host(&store.settings.proxy_host) && store.settings.proxy_port > 0 => Some(format!(
            "{}://{}:{}",
            if store.settings.proxy_mode == "http" { "http" } else { "socks5h" },
            store.settings.proxy_host,
            store.settings.proxy_port
        )),
        _ => None,
    });
    // Заставка — только при первом запуске браузера, не для каждого нового окна.
    let splash = if primary { store.settings.splash.clone() } else { "off".to_string() };
    let spell = {
        let proxy = proxy.clone();
        spell::Worker::start(move |seq, fix| {
            let _ = proxy.send_event(UserEvent::SpellFix(seq, fix));
        })
    };
    let ui = build_ui(&window, proxy.clone(), &mut web_context, keys.clone(), inprivate, dark, &splash, network.clone())?;
    let vault: passwords::Vault = store.read_named("passwords.json");

    let mut app = App {
        adblock: Arc::new(AtomicBool::new(store.settings.adblock)),
        download_dir: Arc::new(Mutex::new(store.settings.download_dir.clone())),
        window,
        ui,
        proxy,
        web_context,
        store,
        inprivate,
        primary,
        is_default: system::is_default(),
        eco: HashMap::new(),
        processes: Vec::new(),
        on_battery: efficiency::on_battery(),
        import_sources: pages::import_sources(),
        last_download: None,
        ubol_id: None,
        ubol_folder: None,
        extensions: Vec::new(),
        ext_list: Vec::new(),
        ext_queue: Default::default(),
        ext_installing: false,
        ext_busy: Default::default(),
        ext_popup: None,
        ext_popup_spare: None,
        peek: None,
        shape_height: std::cell::Cell::new(-1.0),
        spell,
        shot_versions: HashMap::new(),
        ext_browsers: Vec::new(),
        ext_quiet: Default::default(),
        prewarm: None,
        update_ready: false,
        last_update_check: Instant::now() - Duration::from_secs(3600) + Duration::from_secs(20),
        groups: Vec::new(),
        split: None,
        split_drag: false,
        cursor: (0.0, 0.0),
        autofill: Default::default(),
        pw_health: Vec::new(),
        autofill_locked: false,
        pw_checking: false,
        account: None,
        sync_error: None,
        last_sync: Instant::now(),
        lan_port: None,
        lan_group: Default::default(),
        lan_rev: Default::default(),
        lan_fp: String::new(),
        dav_busy: false,
        dav_checked: Instant::now(),
        sent_tabs: Vec::new(),
        sent_handled: Default::default(),
        vault,
        pw_offer: None,
        pw_verified: None,
        tabs: Vec::new(),
        active: 0,
        next_id: 1,
        closed: Vec::new(),
        closed_groups: Vec::new(),
        tab_icons: load_tab_icons(),
        downloads: Vec::new(),
        keys,
        overlay: Overlay::None,
        splash: splash != "off",
        overlay_pending: false,
        overlay_requested: Instant::now(),
        chrome_height: DEFAULT_CHROME,
        chrome_side: 0.0,
        chrome_float: None,
        toast: None,
        panel: None,
        panel_apps: HashMap::new(),
        fullscreen: false,
        page_fullscreen: false,
        maximized: false,
        dirty: true,
        last_push: Instant::now(),
        sent: Default::default(),
        history_dirty: false,
        session_dirty: false,
        last_save: Instant::now(),
        last_sleep_check: Instant::now(),
        tasks_watch: false,
        panel_sleeping: false,
        minimized: false,
        site_only: false,
        last_tasks: Instant::now(),
        tasks_sent: Default::default(),
        quit: false,
    };
    app.apply_theme();
    app.load_downloads();
    if app.store.settings.sidebar_open {
        app.store.settings.sidebar_open = false;
        app.toggle_panel();
    }
    // Пустую новую вкладку открываем, только если не пришло ни сеанса, ни адресов.
    app.restore_session(urls.is_empty());
    // Распаковка uBlock Origin Lite (31 МБ при первом запуске) — в фоне, чтобы не тормозить старт.
    let unpacked = app.proxy.clone();
    std::thread::spawn(move || match extension::ensure_unpacked(&data_dir()) {
        Ok(path) => {
            let _ = unpacked.send_event(UserEvent::ExtensionReady(path));
        }
        Err(error) => eprintln!("не удалось распаковать uBlock Origin Lite: {error}"),
    });
    // Адреса из командной строки (например, ссылка, открытая этим браузером) — новыми вкладками.
    for url in urls {
        let url = to_url(&url, search_url(&app.store.settings.search));
        app.new_tab(Some(url));
    }
    app.extensions = app.store.read_named("extensions.json");
    let stored_autofill: autofill::Stored = app.store.read_named("autofill.json");
    // не расшифровалось (другая учётная запись Windows, повреждение) — файл не трогаем,
    // иначе первое же сохранение стёрло бы адреса и карты
    app.autofill_locked = !stored_autofill.data.is_empty() && passwords::unprotect(&stored_autofill.data).is_none();
    app.autofill = autofill::Profile::load(&stored_autofill);
    // аккаунт: первая синхронизация — сразу после запуска, когда окно уже на экране
    if !app.inprivate {
        let account: sync::Account = app.store.read_named("account.json");
        app.sent_tabs = app.store.read_named("sent_tabs.json");
        app.sent_handled = app.store.read_named("sent_handled.json");
        // старый аккаунт через папку OneDrive (без номера связки) больше не используется
        if !account.group.is_empty() && !account.key.is_empty() {
            app.account = Some(account);
            app.last_sync = Instant::now() - SYNC_EVERY + Duration::from_secs(4);
            app.start_lan();
        }
    }
    register_icons(&app.extensions, None);
    app.refresh_extensions();
    if app.primary && system::is_registered() {
        // exe могли перенести — обновляем путь в регистрации Windows
        std::thread::spawn(system::register);
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + SLEEP_CHECK);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => app.quit = true,
                WindowEvent::CursorMoved { position, .. } => app.split_cursor(position.x, position.y),
                WindowEvent::MouseInput { state, button: tao::event::MouseButton::Left, .. } => {
                    app.split_press(state == tao::event::ElementState::Pressed)
                }
                WindowEvent::Resized(_) => {
                    app.layout();
                    app.apply_minimized();
                    let maximized = app.window.is_maximized();
                    if maximized != app.maximized {
                        app.maximized = maximized;
                        app.dirty = true;
                        app.push_panel_state();
                    }
                }
                WindowEvent::Focused(true) => {
                    app.focus_content();
                    // пользователь мог вернуться из Параметров Windows
                    let is_default = system::is_default();
                    if is_default != app.is_default {
                        app.is_default = is_default;
                        app.dirty = true;
                    }
                }
                WindowEvent::ThemeChanged(_) => app.apply_theme(),
                _ => {}
            },
            Event::UserEvent(event) => app.handle(event),
            Event::MainEventsCleared => app.flush(),
            _ => {}
        }
        if app.dirty {
            // отложенное обновление интерфейса — проснуться, когда оно разрешено
            *control_flow = ControlFlow::WaitUntil(app.last_push + PUSH_INTERVAL);
        }
        if app.tasks_watch {
            let next = app.last_tasks + TASKS_EVERY;
            if let ControlFlow::WaitUntil(at) = *control_flow {
                *control_flow = ControlFlow::WaitUntil(at.min(next));
            }
        }
        if app.overlay_pending {
            // проснуться вовремя, чтобы открыть меню и без снимка
            *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
        }

        if app.quit {
            app.save_session();
            app.store.save_all();
            app.sync_now(false); // последние изменения — в облако
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// Тёмная ли тема, ещё до создания окна: по настройке, а «как в системе» — по Windows.
fn window_dark_hint() -> bool {
    let settings: store::Settings = Store::load(data_dir()).settings;
    AMOLED.store(settings.theme == "amoled", Ordering::Relaxed);
    THEME_BACKDROP.store(theme_backdrop(&settings.theme), Ordering::Relaxed);
    LEFT_CLICK_BACKGROUND.store(settings.left_click_background, Ordering::Relaxed);
    theme_dark(&settings.theme).unwrap_or_else(system::system_prefers_dark)
}

/// Папка с закладками, историей, настройками и профилем.
/// Переменная RUST_BROWSER_DATA задаёт другую папку — для тестов и переносной версии.
fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("RUST_BROWSER_DATA") {
        return PathBuf::from(dir);
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("RustBrowser")
}

fn default_download_dir() -> String {
    std::env::var_os("USERPROFILE")
        .map(|home| PathBuf::from(home).join("Downloads").to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Отправленные вкладки живут в облаке неделю, а дальше удаляются при слиянии.
fn fresh_sent_tabs(list: &[Value]) -> Vec<Value> {
    let week_ago = unix_now().saturating_sub(7 * 24 * 3600);
    list.iter().filter(|t| t["time"].as_u64().unwrap_or(0) > week_ago).take(200).cloned().collect()
}

/// Хост для файла значка вкладки (без www., только безопасные символы).
fn tab_icon_host(url: &str) -> Option<String> {
    let host = adblock::host_of(url)?.to_ascii_lowercase();
    let host = host.trim_start_matches("www.").to_string();
    icons::valid_host(&host).then_some(host)
}

/// Значки вкладок, сохранённые в прошлые запуски: спящие вкладки сразу со значком.
fn load_tab_icons() -> HashMap<String, (u32, usize)> {
    let Ok(entries) = std::fs::read_dir(data_dir().join("tab-icons")) else { return HashMap::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let host = name.strip_suffix(".png")?.to_string();
            let size = e.metadata().ok()?.len() as usize;
            icons::valid_host(&host).then_some((host, (1, size)))
        })
        .collect()
}

/// Где на окне стоит уведомление: по центру снизу (как в style.css: bottom 28px),
/// с запасом под тень и анимацию появления.
/// Вырез точно по уведомлению: вокруг него окно интерфейса показало бы свой фон, а не страницу.
fn toast_part(width: f64, height: f64, w: f64, h: f64) -> [f64; 5] {
    let x = ((width - w) / 2.0).max(0.0);
    let y = (height - 28.0 - h).max(0.0);
    [x, y, w.min(width), h, 12.0]
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(width.max(0.0), height.max(0.0)).into(),
    }
}

fn window_size(window: &Window) -> (f64, f64) {
    let scale = window.scale_factor();
    let size = window.inner_size();
    (size.width as f64 / scale, size.height as f64 / scale)
}

/// Интерфейс: вкладки, панели, стартовая страница, меню и библиотека в одном webview.
fn build_ui(
    window: &Window,
    proxy: EventLoopProxy<UserEvent>,
    web_context: &mut WebContext,
    keys: Arc<KeyState>,
    inprivate: bool,
    dark: bool,
    splash: &str,
    network: Option<wry::ProxyConfig>,
) -> wry::Result<WebView> {
    let html = include_str!("ui/index.html")
        .replace("/*__CSS__*/", include_str!("ui/style.css"))
        .replace("/*__JS__*/", include_str!("ui/app.js"));
    let engine = wry::webview_version().unwrap_or_default();
    let boot = json!({
        "inprivate": inprivate,
        "version": env!("CARGO_PKG_VERSION"),
        "engine": engine,
        "splash": splash,
    });
    let (width, height) = window_size(window);

    let ipc = proxy.clone();
    let ui = WebViewBuilder::new_with_web_context(web_context)
        .with_bounds(rect(0.0, 0.0, width, height))
        // Своё фото для обоев: интерфейс берёт его по адресу http://rb.localhost/wallpaper.
        // Значки сайтов качаются в фоне: интерфейс получит ответ, когда он будет готов.
        .with_asynchronous_custom_protocol("rb".into(), |_, request, responder| {
            let path = request.uri().path().to_string();
            if let Some(host) = path.strip_prefix("/siteicon/").map(str::to_string) {
                std::thread::spawn(move || {
                    let response = match icons::get(&icons::cache_dir(&data_dir()), &host) {
                        Some((bytes, mime)) => wry::http::Response::builder()
                            .header("Content-Type", mime)
                            .header("Cache-Control", "no-cache")
                            .body(std::borrow::Cow::Owned(bytes)),
                        None => wry::http::Response::builder()
                            .status(404)
                            .header("Cache-Control", "no-store")
                            .body(std::borrow::Cow::Owned(Vec::new())),
                    };
                    if let Ok(response) = response {
                        responder.respond(response);
                    }
                });
                return;
            }
            if let Some(id) = path.strip_prefix("/tabshot/").and_then(|s| s.parse::<u32>().ok()) {
                let bytes = TAB_SHOTS.lock().ok().and_then(|s| s.get(&id).cloned());
                let response = match bytes {
                    Some(bytes) => wry::http::Response::builder().header("Content-Type", "image/jpeg").header("Cache-Control", "max-age=600").body(std::borrow::Cow::Owned(bytes)),
                    None => wry::http::Response::builder().status(404).body(std::borrow::Cow::Owned(Vec::new())),
                };
                if let Ok(response) = response {
                    responder.respond(response);
                }
                return;
            }
            if path == "/backdrop.jpg" {
                let bytes = BACKDROP.lock().map(|b| b.clone()).unwrap_or_default();
                if let Ok(response) = wry::http::Response::builder()
                    .header("Content-Type", "image/jpeg")
                    .header("Cache-Control", "no-store")
                    .body(std::borrow::Cow::Owned(bytes))
                {
                    responder.respond(response);
                }
                return;
            }
            // значок вкладки с диска — сохранён из движка, сеть не нужна
            if let Some(host) = path.strip_prefix("/tabicon/") {
                let file = icons::valid_host(host).then(|| data_dir().join("tab-icons").join(format!("{host}.png")));
                let response = match file.and_then(|f| std::fs::read(f).ok()) {
                    Some(bytes) => wry::http::Response::builder()
                        .header("Content-Type", "image/png")
                        .header("Cache-Control", "max-age=31536000")
                        .body(std::borrow::Cow::Owned(bytes)),
                    None => wry::http::Response::builder().status(404).body(std::borrow::Cow::Owned(Vec::new())),
                };
                if let Ok(response) = response {
                    responder.respond(response);
                }
                return;
            }
            let range = request.headers().get("range").and_then(|v| v.to_str().ok());
            responder.respond(serve_local(&path, range));
        })
        .with_initialization_script(format!("window.__BOOT__ = {boot};"))
        .with_html(html)
        // Цвет до первой отрисовки, чтобы окно не вспыхивало белым в тёмной теме.
        .with_background_color(backdrop_color(dark))
        .with_incognito(inprivate)
        .with_browser_accelerator_keys(false)
        .with_default_context_menus(false)
        .with_hotkeys_zoom(false)
        // Расширения включаются на уровне всего профиля: у всех webview одинаково.
        .with_browser_extensions_enabled(true)
        // прокси задаётся на весь движок сразу — у всех webview одинаково
        .with_network(network)
        // Интерфейс не должен уходить на сайты, например от перетаскивания ссылки.
        .with_navigation_handler(|url| !url.starts_with("http"))
        .with_ipc_handler(move |request: Request<String>| {
            let _ = ipc.send_event(UserEvent::Ui(request.into_body()));
        })
        .build_as_child(window)?;
    platform::hook_hotkeys(&ui, keys, proxy);
    Ok(ui)
}

/// Боковая панель: своя страница с заметками, калькулятором и кнопками окна.
fn build_panel(app: &mut App) -> wry::Result<WebView> {
    let (width, height) = window_size(&app.window);
    let proxy = app.proxy.clone();
    let network = proxy_config(&app.store.settings);
    let (dark, inprivate) = (app.dark(), app.inprivate);
    let panel = WebViewBuilder::new_with_web_context(&mut app.web_context)
        .with_bounds(rect(width - PANEL_WIDTH, 0.0, PANEL_WIDTH, height))
        .with_html(include_str!("ui/panel.html"))
        .with_background_color(backdrop_color(dark))
        .with_incognito(inprivate)
        .with_browser_extensions_enabled(true)
        .with_network(network)
        .with_browser_accelerator_keys(false)
        .with_hotkeys_zoom(false)
        .with_navigation_handler(|url| !url.starts_with("http"))
        .with_ipc_handler(move |request: Request<String>| {
            let _ = proxy.send_event(UserEvent::Panel(request.into_body()));
        })
        .build_as_child(&app.window)?;
    platform::hook_hotkeys(&panel, app.keys.clone(), app.proxy.clone());
    Ok(panel)
}

/// Мессенджер в боковой панели — обычный сайт в своём окошке под шапкой панели.
fn build_panel_app(app: &mut App, url: &str) -> wry::Result<WebView> {
    let (width, height) = window_size(&app.window);
    let popup = app.proxy.clone();
    let network = proxy_config(&app.store.settings);
    let (dark, inprivate, theme) = (app.dark(), app.inprivate, app.page_theme());
    let webview = WebViewBuilder::new_with_web_context(&mut app.web_context)
        .with_bounds(rect(width - PANEL_WIDTH, PANEL_HEADER, PANEL_WIDTH, height - PANEL_HEADER))
        .with_url(url)
        .with_background_color(backdrop_color(dark))
        .with_incognito(inprivate)
        .with_browser_extensions_enabled(true)
        .with_network(network)
        .with_theme(theme)
        // ссылки из чатов открываются вкладками браузера
        .with_new_window_req_handler(move |url, _| {
            let _ = popup.send_event(UserEvent::NewTabRequested(url, false));
            NewWindowResponse::Deny
        })
        .build_as_child(&app.window)?;
    platform::hook_hotkeys(&webview, app.keys.clone(), app.proxy.clone());
    Ok(webview)
}

/// Webview страницы. Адрес загружается позже, когда все перехватчики уже стоят.
fn build_tab(app: &mut App, id: u32, bounds: Rect) -> wry::Result<WebView> {
    let proxy = app.proxy.clone();
    let (nav, title, load, popup) = (proxy.clone(), proxy.clone(), proxy.clone(), proxy.clone());
    let theme = app.page_theme();
    // Пока страница не отрисовалась, под ней цвет темы, а не белая вспышка.
    let background = backdrop_color(app.dark());
    let webview = WebViewBuilder::new_with_web_context(&mut app.web_context)
        .with_bounds(bounds)
        .with_background_color(background)
        .with_incognito(app.inprivate)
        .with_browser_extensions_enabled(true)
        .with_network(proxy_config(&app.store.settings))
        .with_devtools(true)
        .with_theme(theme)
        // менеджер паролей: замечает формы входа (сам пароль — только при отправке)
        .with_initialization_script(passwords::PAGE_SCRIPT)
        .with_initialization_script(webstore::STORE_SCRIPT)
        .with_initialization_script(SCROLL_SCRIPT)
        .with_initialization_script(SELECTION_SCRIPT)
        .with_initialization_script(PREFETCH_SCRIPT)
        .with_initialization_script(BOOST_SCRIPT)
        .with_initialization_script(autofill::DETECT_SCRIPT)
        .with_navigation_handler(move |url| {
            let _ = nav.send_event(UserEvent::UrlChanged(id, url));
            true
        })
        .with_document_title_changed_handler(move |text| {
            let _ = title.send_event(UserEvent::TitleChanged(id, text));
        })
        .with_on_page_load_handler(move |event, _| {
            let started = matches!(event, PageLoadEvent::Started);
            let _ = load.send_event(UserEvent::Loading(id, started));
        })
        // Ссылки «в новом окне» открываем новой вкладкой: колёсиком и Ctrl+щелчком — в фоне,
        // обычным щелчком — с переходом на неё (если в настройках не выбрано «всегда в фоне»).
        .with_new_window_req_handler(move |url, _| {
            let background = platform::background_click();
            let background = background || LEFT_CLICK_BACKGROUND.load(Ordering::Relaxed);
            let _ = popup.send_event(UserEvent::NewTabRequested(url, background));
            NewWindowResponse::Deny
        })
        .build_as_child(&app.window)?;

    platform::hook_hotkeys(&webview, app.keys.clone(), proxy.clone());
    platform::hook_adblock(&webview, id, app.adblock.clone(), proxy.clone());
    platform::set_adblock_filter(&webview, app.ubol_id.is_none());
    platform::hook_history(&webview, id, proxy.clone());
    platform::hook_messages(&webview, id, proxy.clone());
    platform::hook_crashes(&webview, id, proxy.clone());
    platform::hook_context_menu(&webview);
    platform::hook_focus(&webview, id, proxy.clone());
    platform::hook_page_state(&webview, id, proxy.clone());
    platform::hook_find(&webview, id, proxy.clone());
    platform::hook_load_errors(&webview, id, proxy.clone());
    platform::hook_downloads(&webview, app.download_dir.clone(), proxy);
    if let Some(tab) = app.tab(id) {
        platform::set_document_script(&webview, &tab.dark_script, Some(&dark_sites_script(&app.store.settings)));
        platform::set_document_script(&webview, &tab.boost_script, Some(&boost_sites_script(&app.store.settings)));
    }
    // встроенное сохранение паролей WebView2 выключено: пароли хранит свой менеджер
    platform::set_passwords(&webview, false);
    Ok(webview)
}

impl App {
    fn tab(&self, id: u32) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    fn tab_mut(&mut self, id: u32) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    fn index_of(&self, id: u32) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// Значок вкладки: сохранённый с диска (без сети), иначе адрес значка сайта.
    fn tab_favicon(&self, tab: &Tab) -> String {
        match tab_icon_host(&tab.url).and_then(|h| self.tab_icons.get(&h).map(|(v, _)| (h, *v))) {
            Some((host, version)) => format!("http://rb.localhost/tabicon/{host}?v={version}"),
            None => tab.favicon.clone(),
        }
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.tab(self.active)
    }

    fn active_webview(&self) -> Option<&WebView> {
        self.active_tab().and_then(|t| t.webview.as_ref())
    }

    fn pinned_count(&self) -> usize {
        self.tabs.iter().filter(|t| t.pinned).count()
    }

    fn is_fullscreen(&self) -> bool {
        self.fullscreen || self.page_fullscreen
    }

    // ---------- раскладка и фокус ----------

    /// Расставляет webview: интерфейс сверху, под ним активная страница.
    /// Где страница: под панелью или справа от боковой колонки.
    /// Ширина боковой панели справа (0 — закрыта или полный экран).
    fn panel_width(&self) -> f64 {
        if self.panel.is_none() || !self.store.settings.sidebar_open || self.is_fullscreen() || self.site_only {
            return 0.0;
        }
        let (width, _) = window_size(&self.window);
        PANEL_WIDTH.min(width * 0.5)
    }

    /// Окно без боковой панели: в этой части живут интерфейс и страницы.
    fn content_size(&self) -> (f64, f64) {
        let (width, height) = window_size(&self.window);
        (width - self.panel_width(), height)
    }

    fn page_rect(&self) -> Rect {
        let (width, height) = self.content_size();
        if self.is_fullscreen() {
            rect(0.0, 0.0, width, height)
        } else if self.chrome_side > 0.0 {
            rect(self.chrome_side, 0.0, width - self.chrome_side, height)
        } else if let Some((top, _)) = &self.chrome_float {
            rect(0.0, *top, width, height - top)
        } else {
            rect(0.0, self.chrome_height, width, height - self.chrome_height)
        }
    }

    fn layout(&self) {
        let (width, height) = self.content_size();
        self.layout_panel();
        let chrome = if self.is_fullscreen() { 0.0 } else { self.chrome_height };
        let side = if self.is_fullscreen() { 0.0 } else { self.chrome_side };
        let on_start_page = self.active_webview().is_none();

        let (bx, by, bw, bh) = match self.overlay {
            _ if on_start_page || self.splash => (0.0, 0.0, width, height),
            Overlay::Full => (0.0, 0.0, width, height),
            // боковая колонка и так на всю высоту — подсказки помещаются в ней
            _ if side > 0.0 => (0.0, 0.0, side, height),
            Overlay::Sheet(extra) => (0.0, 0.0, width, (chrome + extra).min(height)),
            Overlay::None => (0.0, 0.0, width, chrome),
        };
        // Всплывающее уведомление внизу окна: окно интерфейса растягивается на всё окно,
        // но видны в нём только панели и само уведомление — остальное пропускает страницу.
        let toast = self.toast.filter(|_| {
            matches!(self.overlay, Overlay::None | Overlay::Sheet(_)) && !on_start_page && !self.splash && !self.is_fullscreen()
        });
        let peek = self.peek.filter(|_| matches!(self.overlay, Overlay::None) && !on_start_page && !self.splash && !self.is_fullscreen());
        // Окно интерфейса всегда на всё окно, а видимое — вырез (SetWindowRgn). Раньше оно
        // меняло размер: при открытии меню движок на кадр показывал новую область пустой —
        // страница мигала чёрным. Вырез меняется мгновенно и рисовать ничего не заставляет.
        let _ = self.ui.set_bounds(rect(0.0, 0.0, width, height));
        let whole = (bx, by, bw, bh) == (0.0, 0.0, width, height);
        // «Остров» поверх страницы: окно интерфейса обрезается по полосе вкладок и острову.
        // Меню, подсказки, стартовая страница и полный экран — снова целиком.
        let floating = matches!(self.overlay, Overlay::None | Overlay::Sheet(_))
            && !on_start_page
            && !self.splash
            && !self.is_fullscreen()
            && side == 0.0;
        let mut parts: Option<Vec<[f64; 5]>> = match (&self.chrome_float, floating) {
            (Some((_, parts)), true) => Some(parts.clone()),
            _ if whole => None,
            _ => Some(vec![[bx, by, bw, bh, 0.0]]),
        };
        if let Some((w, h)) = toast {
            let mut list = parts.take().unwrap_or_else(|| vec![[bx, by, bw, bh, 0.0]]);
            list.push(toast_part(width, height, w, h));
            parts = Some(list);
        }
        if let Some([x, y, w, h]) = peek {
            let mut list = parts.take().unwrap_or_else(|| vec![[bx, by, bw, bh, 0.0]]);
            list.push([x, y, w, h, 14.0]);
            parts = Some(list);
        }
        // высота видимой части — для проверок (e2e): само окно интерфейса всегда во весь рост
        let shown = parts.as_ref().map_or(height, |p| p.iter().map(|r| r[1] + r[3]).fold(0.0, f64::max));
        if (shown - self.shape_height.get()).abs() > 0.5 {
            self.shape_height.set(shown);
            let _ = self.ui.evaluate_script(&format!("window.__rbShape = {}", shown.round()));
        }
        match parts {
            Some(parts) => {
                let scale = self.window.scale_factor();
                let shape: Vec<[i32; 5]> = parts
                    .iter()
                    .map(|p| p.map(|v| (v * scale).round() as i32))
                    .collect();
                platform::set_shape(&self.ui, Some(&shape));
                platform::raise(&self.ui);
            }
            None => {
                platform::set_shape(&self.ui, None);
                platform::raise(&self.ui); // окно интерфейса теперь всегда над страницей
            }
        }

        // Сначала показываем новую вкладку и только потом прячем остальные: иначе
        // на миг не видно ни одной, и сквозь щель мелькает фон окна.
        let mut visible = self.visible_with(self.active);
        match self.split_rects() {
            Some((left_rect, right_rect, left, right)) if visible.len() == 2 => {
                for (id, bounds) in [(left, left_rect), (right, right_rect)] {
                    if let Some(webview) = self.tab(id).and_then(|t| t.webview.as_ref()) {
                        let _ = webview.set_bounds(bounds);
                        let _ = webview.set_visible(true);
                    }
                }
            }
            _ => {
                // на полном экране видна только активная половина: вторая не должна висеть поверх
                visible = vec![self.active];
                if let Some(webview) = self.active_webview() {
                    let _ = webview.set_bounds(self.page_rect());
                    let _ = webview.set_visible(true);
                }
            }
        }
        for tab in &self.tabs {
            if visible.contains(&tab.id) {
                continue;
            }
            if let Some(webview) = &tab.webview {
                let _ = webview.set_visible(false);
            }
        }
    }

    /// Колонка справа: сама панель, а под её шапкой — окно мессенджера.
    fn layout_panel(&self) {
        let panel_width = self.panel_width();
        let (width, height) = window_size(&self.window);
        let left = width - panel_width;
        let app = self.store.settings.sidebar_app.as_str();
        if let Some(panel) = &self.panel {
            if panel_width > 0.0 {
                let _ = panel.set_bounds(rect(left, 0.0, panel_width, height));
            }
            let _ = panel.set_visible(panel_width > 0.0);
        }
        for (name, webview) in &self.panel_apps {
            let shown = panel_width > 0.0 && *name == app;
            if shown {
                let _ = webview.set_bounds(rect(left, PANEL_HEADER, panel_width, height - PANEL_HEADER));
            }
            let _ = webview.set_visible(shown);
            // спрятанный мессенджер — меньше памяти; скрипты работают, уведомления приходят
            platform::set_background(webview, !shown || self.minimized);
        }
    }

    fn toggle_site_only(&mut self) {
        self.site_only = !self.site_only;
        self.layout(); // боковая панель прячется вместе со всем остальным
        self.dirty = true;
    }

    fn toggle_panel(&mut self) {
        if self.panel.is_none() {
            match build_panel(self) {
                Ok(panel) => self.panel = Some(panel),
                Err(error) => {
                    eprintln!("не удалось открыть боковую панель: {error}");
                    return;
                }
            }
        }
        let settings = &mut self.store.settings;
        settings.sidebar_open = !settings.sidebar_open;
        self.store.save_settings();
        let app = self.store.settings.sidebar_app.clone();
        self.set_panel_app(&app);
        self.sleep_panel();
        self.dirty = true;
    }

    /// Переключает приложение в панели; мессенджер создаётся при первом открытии.
    fn set_panel_app(&mut self, app: &str) {
        let Some((name, url)) = PANEL_APPS.iter().find(|(id, _)| *id == app).copied() else { return };
        if !url.is_empty() && !self.panel_apps.contains_key(name) && self.store.settings.sidebar_open {
            match build_panel_app(self, url) {
                Ok(webview) => {
                    self.panel_apps.insert(name, webview);
                }
                Err(error) => eprintln!("не удалось открыть {name}: {error}"),
            }
        }
        self.store.settings.sidebar_app = name.to_string();
        self.store.save_settings();
        self.layout();
        self.push_panel_state();
        if let Some(webview) = self.panel_apps.get(name).filter(|_| self.store.settings.sidebar_open) {
            let _ = webview.focus();
        }
    }

    fn push_panel_state(&self) {
        let Some(panel) = &self.panel else { return };
        let notes: String = self.store.read_named("notes.json");
        let state = json!({
            "app": self.store.settings.sidebar_app,
            "notes": notes,
            "dark": self.dark(),
            "amoled": self.store.settings.theme == "amoled",
            "accent": self.store.settings.accent,
            "maximized": self.maximized,
        });
        let _ = panel.evaluate_script(&format!("panel.setState({state})"));
    }

    fn handle_panel(&mut self, raw: &str) {
        let Ok(message) = serde_json::from_str::<Value>(raw) else { return };
        match message["cmd"].as_str().unwrap_or("") {
            "ready" => self.push_panel_state(),
            "app" => {
                let app = message["id"].as_str().unwrap_or("notes").to_string();
                self.set_panel_app(&app);
            }
            "close" => self.toggle_panel(),
            "notes" => {
                let text = message["text"].as_str().unwrap_or("").to_string();
                self.store.write_named("notes.json", &text);
            }
            "minimize" => self.window.set_minimized(true),
            "maximize" => self.window.set_maximized(!self.window.is_maximized()),
            "quit" => self.quit = true,
            "drag" => {
                let _ = self.window.drag_window();
            }
            "resize" => {
                let _ = self.window.drag_resize_window(ResizeDirection::North);
            }
            _ => {}
        }
    }

    fn focus_content(&self) {
        match self.active_webview() {
            Some(webview) if self.overlay != Overlay::Full => {
                let _ = webview.focus();
            }
            _ => {
                let _ = self.ui.focus();
            }
        }
    }

    fn focus_url(&mut self) {
        self.push_state(); // интерфейс должен знать активную вкладку до фокуса
        let _ = self.ui.focus();
        let _ = self.ui.evaluate_script("ui.focusUrl()");
    }

    fn update_title(&self) {
        let title = self.active_tab().map(|t| t.title.as_str()).unwrap_or("");
        let suffix = if self.inprivate { format!("InPrivate — {APP_NAME}") } else { APP_NAME.into() };
        if title.is_empty() {
            self.window.set_title(&suffix);
        } else {
            self.window.set_title(&format!("{title} — {suffix}"));
        }
    }

    /// Закрывает меню и библиотеку, если что-то сменило вкладку под ними.
    fn reset_overlay(&mut self) {
        if self.overlay != Overlay::None || self.overlay_pending {
            self.overlay = Overlay::None;
            self.overlay_pending = false;
            let _ = self.ui.evaluate_script("ui.closeOverlays()");
        }
    }

    // ---------- оформление ----------

    fn dark(&self) -> bool {
        theme_dark(&self.store.settings.theme).unwrap_or(self.window.theme() == WindowTheme::Dark)
    }

    fn page_theme(&self) -> Theme {
        match theme_dark(&self.store.settings.theme) {
            Some(true) => Theme::Dark,
            Some(false) => Theme::Light,
            None => Theme::Auto,
        }
    }

    fn apply_theme(&mut self) {
        self.ext_popup_spare = None; // запасное окошко расширения было в старой теме
        self.push_panel_state();
        let dark = self.dark();
        platform::style_window(self.window.hwnd(), dark);
        let backdrop = backdrop_color(dark);
        self.window.set_background_color(Some(backdrop));
        let _ = self.ui.set_background_color(backdrop);
        let theme = self.page_theme();
        for webview in self.tabs.iter().filter_map(|t| t.webview.as_ref()) {
            let _ = webview.set_theme(theme);
            let _ = webview.set_background_color(backdrop);
        }
        self.dirty = true;
    }

    // ---------- сеанс ----------

    fn restore_session(&mut self, open_start_tab: bool) {
        let session = if !self.primary || !self.store.settings.restore_session {
            Session::default()
        } else {
            self.store.load_session()
        };
        if session.tabs.is_empty() {
            if open_start_tab {
                self.new_tab(None);
            }
            return;
        }
        for saved in &session.tabs {
            let id = self.next_id;
            self.next_id += 1;
            let mut tab = Tab::new(id, &saved.url, &saved.title, saved.pinned);
            // группа, которой нет в сеансе, позже совпала бы по номеру с новой группой
            if session.groups.iter().any(|g| g.id == saved.group) {
                tab.group = saved.group;
            }
            self.tabs.push(tab);
        }
        self.groups = session.groups.clone();
        self.prune_groups();
        // Загружается только активная вкладка, остальные — когда их откроют.
        let index = session.active.min(self.tabs.len() - 1);
        self.activate(self.tabs[index].id);
    }

    fn save_session(&mut self) {
        self.session_dirty = false;
        if !self.primary {
            return;
        }
        let saved: Vec<&Tab> = self.tabs.iter().filter(|t| !t.url.is_empty()).collect();
        let session = Session {
            active: saved.iter().position(|t| t.id == self.active).unwrap_or(0),
            tabs: saved.iter().map(|t| t.snapshot()).collect(),
            // группа из одних пустых новых вкладок в сеанс не попадает
            groups: self.groups.iter().filter(|g| saved.iter().any(|t| t.group == g.id)).cloned().collect(),
        };
        self.store.save_session(&session);
    }

    // ---------- вкладки ----------

    fn new_tab(&mut self, url: Option<String>) {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab::new(id, "", "", false));
        self.reset_overlay();
        self.dirty = true;
        self.session_dirty = true;

        match url {
            Some(url) => {
                self.set_active(id);
                self.navigate(id, &url);
            }
            None => self.activate(id),
        }
    }

    /// Вкладка в фоне: загрузится, когда на неё переключатся.
    fn new_background_tab(&mut self, url: &str) {
        let id = self.next_id;
        self.next_id += 1;
        // Как в Edge: сразу за текущей вкладкой и за открытыми из неё раньше.
        let mut insert_at = self.index_of(self.active).map_or(self.tabs.len(), |i| i + 1);
        while self.tabs.get(insert_at).is_some_and(|t| t.opener == Some(self.active)) {
            insert_at += 1;
        }
        let title = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url);
        let mut tab = Tab::new(id, url, title, false);
        tab.opener = Some(self.active);
        self.tabs.insert(insert_at.max(self.pinned_count()), tab);
        self.dirty = true;
        self.session_dirty = true;
    }

    fn close_tab(&mut self, id: u32) {
        let Some(index) = self.index_of(id) else { return };
        let tab = self.tabs.remove(index);
        if let Ok(mut shots) = TAB_SHOTS.lock() {
            shots.remove(&id);
        }
        self.shot_versions.remove(&id);
        if !tab.url.is_empty() {
            self.closed.push(tab.snapshot());
        }
        if let Some(webview) = &tab.webview {
            platform::find_stop(webview);
        }
        drop(tab); // уничтожает webview вкладки
        self.dirty = true;
        self.session_dirty = true;
        // вторая половина разделённого экрана: после закрытия этой остаётся на виду
        let mut partner = None;
        if let Some((l, r, _)) = self.split.filter(|(l, r, _)| *l == id || *r == id) {
            partner = Some(if l == id { r } else { l });
            self.split = None;
            if self.active != id {
                self.layout(); // иначе раскладку обновит переход на другую вкладку ниже
            }
        }
        self.prune_groups();

        if self.tabs.is_empty() {
            self.quit = true;
            return;
        }
        if self.active == id {
            match partner.filter(|p| self.tab(*p).is_some()) {
                Some(partner) => self.activate(partner),
                None => {
                    let neighbour = index.min(self.tabs.len() - 1);
                    self.activate(self.tabs[neighbour].id);
                }
            }
        }
    }

    fn close_many(&mut self, ids: Vec<u32>) {
        // активную закрывают вместе с другими — сразу переходим на ту, что останется:
        // иначе по очереди открывались бы (и грузились) обречённые соседки
        if ids.contains(&self.active) {
            let index = self.index_of(self.active).unwrap_or(0);
            let staying = |i: &usize| self.tabs.get(*i).is_some_and(|t| !ids.contains(&t.id));
            let partner = self.visible_with(self.active).into_iter().filter_map(|t| self.index_of(t)).find(staying);
            let next = partner.or_else(|| (index + 1..self.tabs.len()).chain((0..index).rev()).find(staying));
            if let Some(next) = next {
                self.activate(self.tabs[next].id);
            }
        }
        for id in ids {
            self.close_tab(id);
        }
    }

    /// Делает вкладку активной без загрузки и фокуса.
    fn set_active(&mut self, id: u32) {
        let previous = self.active;
        let scroll_proxy = self.proxy.clone();
        // половины разделённого экрана видны обе: соседняя не уходит в фон
        let visible_after = self.visible_with(id);
        if let Some(partner) = self.visible_with(previous).into_iter().find(|t| *t != previous && !visible_after.contains(t)) {
            if let Some(tab) = self.tab_mut(partner) {
                tab.hidden_since = Some(Instant::now());
                if let Some(webview) = &tab.webview {
                    platform::set_background(webview, true);
                }
            }
        }
        if let Some(partner) = visible_after.iter().copied().find(|t| *t != id) {
            if let Some(tab) = self.tab_mut(partner) {
                tab.hidden_since = None;
                tab.sleeping = false;
                if let Some(webview) = &tab.webview {
                    platform::set_background(webview, false);
                }
            }
        }
        if let Some(old) = self.tab_mut(previous) {
            if previous != id && !visible_after.contains(&previous) {
                old.hidden_since = Some(Instant::now());
                if let Some(webview) = &old.webview {
                    platform::set_background(webview, true);
                    // запоминаем прокрутку: если вкладку потом выгрузят, вернём её
                    let proxy = scroll_proxy;
                    let _ = webview.evaluate_script_with_callback("Math.round(scrollY)", move |y| {
                        if let Ok(y) = y.trim_matches('"').parse::<f64>() {
                            let _ = proxy.send_event(UserEvent::Scrolled(previous, y));
                        }
                    });
                }
            }
        }
        if let Some(tab) = self.tab_mut(id) {
            tab.hidden_since = None;
            tab.sleeping = false; // видимая вкладка просыпается сама
            if let Some(webview) = &tab.webview {
                platform::set_background(webview, false);
            }
        }
        if previous != id {
            self.close_find();
        }
        self.active = id;
        self.session_dirty = true;
        self.refresh_processes(false);
    }

    fn activate(&mut self, id: u32) {
        // уходим с вкладки — её снимок для предпросмотра при наведении
        if self.active != id {
            if let Some(webview) = self.active_webview() {
                let (proxy, old) = (self.proxy.clone(), self.active);
                platform::capture(webview, move |bytes| {
                    let _ = proxy.send_event(UserEvent::TabShot(old, bytes));
                });
            }
        }
        let Some(tab) = self.tab(id) else { return };
        let lazy_url = (tab.webview.is_none() && !tab.url.is_empty()).then(|| tab.url.clone());
        self.set_active(id);
        self.reset_overlay();
        self.dirty = true;

        if let Some(url) = lazy_url {
            self.navigate(id, &url);
            return;
        }
        self.layout();
        self.update_title();
        if self.active_webview().is_some() {
            self.focus_content();
        } else {
            self.focus_url(); // на стартовой странице курсор сразу в поиске
        }
    }

    /// Снимок вкладки на экране для предпросмотра при наведении.
    fn shot_active(&self, id: u32) {
        if id != self.active || self.overlay != Overlay::None {
            return;
        }
        if let Some(webview) = self.active_webview() {
            let proxy = self.proxy.clone();
            platform::capture(webview, move |bytes| {
                let _ = proxy.send_event(UserEvent::TabShot(id, bytes));
            });
        }
    }

    /// Переключает на соседнюю вкладку по кругу.
    fn cycle_tab(&mut self, step: isize) {
        let Some(index) = self.index_of(self.active) else { return };
        let count = self.tabs.len() as isize;
        let next = (index as isize + step).rem_euclid(count) as usize;
        self.activate(self.tabs[next].id);
    }

    /// Перемещает вкладку, не смешивая закреплённые и обычные.
    fn move_tab(&mut self, id: u32, to: usize) {
        let Some(from) = self.index_of(id) else { return };
        let mut tab = self.tabs.remove(from);
        let pinned = self.pinned_count();
        let to = if tab.pinned { to.min(pinned) } else { to.clamp(pinned, self.tabs.len()) };
        if !tab.pinned {
            // группа держится вместе: вкладка, унесённая из группы, выходит из неё,
            // а брошенная внутрь чужой группы — вступает в неё
            let neighbour = |i: Option<usize>| i.and_then(|i| self.tabs.get(i)).filter(|t| !t.pinned).map(|t| t.group);
            let others = self.tabs.iter().any(|t| t.group == tab.group);
            let group = group_after_move(neighbour(to.checked_sub(1)), neighbour(Some(to)), tab.group, others);
            tab.group = group;
        }
        self.tabs.insert(to, tab);
        self.prune_groups();
        self.dirty = true;
        self.session_dirty = true;
    }

    fn toggle_pin(&mut self, id: u32) {
        let Some(index) = self.index_of(id) else { return };
        let mut tab = self.tabs.remove(index);
        tab.pinned = !tab.pinned;
        // Закреплённая встаёт в конец закреплённых, откреплённая — в начало обычных.
        let at = self.pinned_count();
        self.tabs.insert(at, tab);
        self.dirty = true;
        self.session_dirty = true;
    }

    fn navigate(&mut self, id: u32, url: &str) {
        let Some(index) = self.index_of(id) else { return };

        // наведение на плитку уже начало грузить этот сайт — просто показываем его
        if self.tabs[index].webview.is_none() {
            if let Some((new_id, _, webview)) = self.prewarm.take_if(|(_, address, _)| address == url) {
                // у заранее созданной вкладки свой номер (её события не путались с новой
                // вкладкой) — она просто встаёт на место новой вкладки
                // закрепление, группа и место в разделённом экране остаются за вкладкой
                let old = &self.tabs[index];
                let mut tab = Tab::new(new_id, url, "", old.pinned);
                tab.group = old.group;
                tab.opener = old.opener;
                tab.url = webview.url().unwrap_or_else(|_| url.to_string());
                tab.title = platform::document_title(&webview);
                tab.webview = Some(webview);
                self.tabs[index] = tab;
                if self.active == id {
                    self.active = new_id;
                }
                if let Some((l, r, _)) = &mut self.split {
                    for half in [l, r] {
                        if *half == id {
                            *half = new_id;
                        }
                    }
                }
                self.sync_location(new_id);
                self.detect_language(new_id);
                self.collect_site_icon(new_id);
                platform::raise(&self.ui);
                self.reset_overlay();
                self.dirty = true;
                self.session_dirty = true;
                self.layout();
                self.update_title();
                self.focus_content();
                return;
            }
        }
        self.prewarm = None; // заготовка была под другой сайт
        if self.tabs[index].webview.is_none() {
            let bounds = self.page_rect();
            match build_tab(self, id, bounds) {
                Ok(webview) => {
                    self.tabs[index].webview = Some(webview);
                    // Новое окно создаётся поверх всех — возвращаем интерфейс наверх.
                    platform::raise(&self.ui);
                }
                Err(error) => {
                    eprintln!("не удалось создать вкладку: {error}");
                    return;
                }
            }
        }

        let tab = &mut self.tabs[index];
        tab.url = url.to_string();
        tab.title.clear();
        tab.loading = true;
        if let Some(webview) = &tab.webview {
            let _ = webview.load_url(url);
        }
        self.reset_overlay();
        self.dirty = true;
        self.session_dirty = true;
        self.layout();
        self.update_title();
        self.focus_content();
    }

    // ---------- события ----------

    fn handle(&mut self, event: UserEvent) {
        match event {
            UserEvent::Ui(raw) => self.handle_ui(&raw),
            UserEvent::UrlChanged(id, url) => {
                if url == "about:blank" || url.starts_with("data:") {
                    return; // пустая страница до первой загрузки или своя страница ошибки
                }
                if let Some(tab) = self.tab_mut(id) {
                    tab.url = url;
                    tab.failed = false;
                    tab.blocked = 0;
                    tab.pw_form = false;
                    tab.video = false;
                    tab.af_card = false;
                    tab.af_address = false;
                    self.dirty = true;
                    self.session_dirty = true;
                }
            }
            UserEvent::TitleChanged(id, title) => {
                if title == "about:blank" {
                    return;
                }
                let Some(tab) = self.tab_mut(id) else { return };
                tab.title = title.clone();
                let url = tab.url.clone();
                if !self.inprivate {
                    self.store.set_title(&url, &title);
                    self.history_dirty = true;
                }
                self.dirty = true;
                self.session_dirty = true;
                if id == self.active {
                    self.update_title();
                }
            }
            UserEvent::Loading(id, loading) => {
                let Some(tab) = self.tab_mut(id) else { return };
                tab.loading = loading;
                self.dirty = true;
                if !loading {
                    self.sync_location(id);
                    self.apply_site_zoom(id);
                    self.restore_scroll(id);
                    let Some(tab) = self.tab(id) else { return };
                    if tab.url.starts_with("http") && !self.inprivate && !tab.failed {
                        let (url, title) = (tab.url.clone(), tab.title.clone());
                        self.store.add_visit(&url, &title);
                        self.history_dirty = true;
                    }
                    self.collect_site_icon(id);
                    self.detect_language(id);
                    // снимок для предпросмотра — пока вкладка на экране (скрытую WebView2 не снимет)
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(1500));
                        let _ = proxy.send_event(UserEvent::ShotActive(id));
                    });
                }
            }
            UserEvent::ShotActive(id) => self.shot_active(id),
            UserEvent::SelTranslated(id, seq, result) => {
                let reply = match result {
                    Ok((text, from, to)) => json!({ "seq": seq, "text": text, "from": from, "to": to }),
                    Err(error) => json!({ "seq": seq, "error": error }),
                };
                if let Some(webview) = self.tab(id).and_then(|t| t.webview.as_ref()) {
                    let _ = webview.evaluate_script(&format!("window.__rbTranslated && __rbTranslated({reply})"));
                }
            }
            UserEvent::SpellFix(seq, fix) => {
                let fix = fix.map_or(Value::Null, |(text, kind)| json!({ "text": text, "kind": kind.as_str() }));
                let _ = self.ui.evaluate_script(&format!("ui.setFix({seq}, {fix})"));
            }
            UserEvent::HistoryChanged(id) => self.sync_location(id),
            UserEvent::NewTabRequested(url, true) => self.new_background_tab(&url),
            UserEvent::NewTabRequested(url, false) => self.new_tab(Some(url)),
            UserEvent::Blocked(id) => {
                if let Some(tab) = self.tab_mut(id) {
                    tab.blocked += 1;
                }
                self.store.settings.blocked_total += 1;
                self.dirty = true;
            }
            UserEvent::Hotkey(hotkey) => self.handle_hotkey(hotkey),
            UserEvent::Download(event) => self.handle_download(event),
            UserEvent::FaviconImage(id, bytes) => {
                let Some(host) = self.tab(id).and_then(|t| tab_icon_host(&t.url)) else { return };
                if self.tab_icons.get(&host).is_some_and(|(_, old)| *old == bytes.len()) {
                    return; // тот же значок
                }
                let dir = data_dir().join("tab-icons");
                let _ = std::fs::create_dir_all(&dir);
                if std::fs::write(dir.join(format!("{host}.png")), &bytes).is_ok() {
                    let version = self.tab_icons.get(&host).map_or(1, |(v, _)| v + 1);
                    self.tab_icons.insert(host, (version, bytes.len()));
                    self.dirty = true;
                }
            }
            UserEvent::Favicon(id, uri) => {
                if let Some(tab) = self.tab_mut(id) {
                    tab.favicon = uri;
                    self.dirty = true;
                }
            }
            UserEvent::Audio(id, playing) => {
                if let Some(tab) = self.tab_mut(id) {
                    tab.audio = playing;
                    self.dirty = true;
                }
            }
            UserEvent::Zoom(id, factor) => self.remember_zoom(id, factor),
            UserEvent::PageFullscreen(id, full) => {
                if id == self.active {
                    self.page_fullscreen = full;
                    self.apply_fullscreen();
                }
            }
            UserEvent::Find(id, index, count) => {
                if id == self.active {
                    let script = format!("ui.setFind({index}, {count})");
                    let _ = self.ui.evaluate_script(&script);
                }
            }
            UserEvent::Captured(bytes) => self.finish_overlay(bytes),
            UserEvent::Panel(raw) => self.handle_panel(&raw),
            UserEvent::Page(id, url, raw) => self.handle_page(id, &url, &raw),
            UserEvent::ExtReady(result, tab) => self.extension_ready(result, tab),
            UserEvent::LanSync(body, reply) => {
                let _ = reply.send(self.lan_exchange(&body));
            }
            UserEvent::DavFetched(result, manual) => self.dav_fetched(result, manual),
            UserEvent::DavStored(result, hashes, manual, changed) => self.dav_stored(result, hashes, manual, changed),
            UserEvent::DavEtag(etag) => {
                self.dav_busy = false;
                let known = self.account.as_ref().and_then(|a| a.webdav.as_ref()).map(|d| d.etag.clone());
                if etag.is_some() && etag != known {
                    self.sync_now(false);
                }
            }
            UserEvent::DavChecked(result, dav) => {
                self.dav_busy = false;
                self.dirty = true;
                match result {
                    Ok(()) => {
                        if let Some(account) = &mut self.account {
                            account.bases.remove("webdav");
                            account.webdav = Some(dav);
                        }
                        self.save_account();
                        self.sync_error = None;
                        self.sync_now(true);
                        self.send_pair();
                    }
                    Err(error) => {
                        let _ = self.ui.evaluate_script(&format!("ui.accountError({error:?})"));
                    }
                }
            }
            UserEvent::PwHealth(health) => {
                self.pw_checking = false;
                let leaked = health.iter().filter(|h| h.leaked > 0).count();
                self.pw_health = health;
                self.dirty = true;
                let text = if self.pw_health.iter().any(|h| h.leaked < 0) {
                    "Сервис проверки утечек не ответил — проверьте интернет".to_string()
                } else if leaked > 0 {
                    format!("Найдено в утечках: {leaked}. Смените эти пароли")
                } else {
                    "Ни один пароль не найден в известных утечках".to_string()
                };
                let _ = self.ui.evaluate_script(&format!("ui.toast({text:?})"));
            }
            UserEvent::CardVerified(index, origin, verdict) => match verdict {
                passwords::Verdict::Verified => self.fill_autofill("card", index, &origin),
                passwords::Verdict::Canceled => {}
                passwords::Verdict::Unavailable => {
                    let _ = self.ui.evaluate_script("ui.toast('Включите ПИН-код Windows Hello — без него карты не подставляются')");
                }
            },
            UserEvent::PageFocused(id) => {
                // щёлкнули во вторую половину разделённого экрана — адрес и кнопки теперь её
                if id != self.active && self.visible_with(self.active).contains(&id) {
                    self.set_active(id);
                    self.dirty = true;
                    self.update_title();
                }
            }
            UserEvent::Updated(result) => {
                match result {
                    Ok(true) => {
                        self.update_ready = true;
                        let _ = self.ui.evaluate_script("ui.updateReady()");
                    }
                    Ok(false) => {}
                    Err(error) => {
                        let _ = self.ui.evaluate_script(&format!("ui.toast({:?})", format!("Обновление не удалось: {error}")));
                    }
                }
                self.dirty = true;
            }
            UserEvent::EngineCrashed => {
                // движок упал целиком: все вкладки мертвы — перезапускаемся, сеанс восстановится
                self.save_session();
                self.store.save_all();
                system::restart(None);
                self.quit = true;
            }
            UserEvent::PageLang(id, lang) => {
                if let Some(tab) = self.tab_mut(id) {
                    if tab.lang != lang {
                        tab.lang = lang;
                        self.dirty = true;
                    }
                }
            }
            UserEvent::IconSaved => {
                let _ = self.ui.evaluate_script("ui.iconsUpdated()");
            }
            UserEvent::ExtAdded(info, result, tab) => self.extension_added(info, result, tab),
            UserEvent::ExtList(list) => {
                self.ext_list = list;
                self.dirty = true;
            }
            UserEvent::ExtChanged => self.refresh_extensions(),
            UserEvent::ExtPopupMeasure => self.measure_ext_popup(),
            UserEvent::TabShot(id, bytes) => {
                if let (Some(bytes), true) = (bytes, self.tab(id).is_some()) {
                    if let Ok(mut shots) = TAB_SHOTS.lock() {
                        shots.insert(id, bytes);
                    }
                    *self.shot_versions.entry(id).or_insert(0) += 1;
                    self.dirty = true;
                }
            }
            UserEvent::ExtPopupShow => {
                // замер не пришёл — всё равно показать
                if let Some((webview, _, _)) = &self.ext_popup {
                    let _ = webview.set_visible(true);
                }
            }
            UserEvent::ExtPopupSize(w, h) => self.resize_ext_popup(w, h),
            UserEvent::Verified(index, copy, verdict) => self.password_verified(index, copy, verdict),
            UserEvent::Processes(list, report) => {
                self.processes = list;
                self.apply_efficiency();
                if report {
                    self.send_tasks();
                }
            }
            UserEvent::Scrolled(id, y) => {
                if let Some(tab) = self.tab_mut(id) {
                    tab.scroll = (y > 0.0).then_some(y);
                }
            }
            UserEvent::LoadFailed(id, kind) => {
                if self.last_download.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
                    return;
                }
                let dark = self.dark();
                let proxy = self.proxy.clone();
                let Some(tab) = self.tab_mut(id) else { return };
                let Some(webview) = &tab.webview else { return };
                // Обрыв связи часто случаен (VPN, смена сети) — как Chrome, сначала сами
                // пробуем ещё пару раз и только потом показываем страницу ошибки.
                if matches!(kind, "connection" | "timeout") {
                    let (url, count, at) = &mut tab.retries;
                    if *url != tab.url || at.is_none_or(|t| t.elapsed() > Duration::from_secs(30)) {
                        *url = tab.url.clone();
                        *count = 0;
                    }
                    if *count < 2 {
                        *count += 1;
                        *at = Some(Instant::now());
                        let (url, delay) = (url.clone(), Duration::from_millis(300 * *count as u64));
                        std::thread::spawn(move || {
                            std::thread::sleep(delay);
                            let _ = proxy.send_event(UserEvent::RetryLoad(id, url));
                        });
                        return;
                    }
                }
                tab.failed = true;
                let _ = webview.load_html(&pages::error_page(&tab.url, kind, dark));
                self.dirty = true;
            }
            UserEvent::RetryLoad(id, url) => {
                // за это время могли уйти на другой адрес — тогда повтор не нужен
                if let Some(webview) = self.tab(id).filter(|t| t.url == url).and_then(|t| t.webview.as_ref()) {
                    let _ = webview.load_url(&url);
                }
            }
            UserEvent::Remote(urls) => {
                if urls.is_empty() {
                    system::open_new_window(&[]); // повторный запуск ярлыком — новое окно
                    return;
                }
                for url in urls {
                    let url = to_url(&url, search_url(&self.store.settings.search));
                    self.new_tab(Some(url));
                }
                self.window.set_minimized(false);
                self.window.set_focus();
            }
            UserEvent::ExtensionReady(path) => {
                self.ubol_folder = Some(path.clone());
                let reinstall = self.store.settings.ubol_version != extension::VERSION;
                let proxy = self.proxy.clone();
                platform::install_extension(&self.ui, &path, "uBlock", reinstall, move |id| {
                    let _ = proxy.send_event(UserEvent::ExtensionInstalled(id));
                });
            }
            UserEvent::ExtensionInstalled(id) => {
                let Some(id) = id else {
                    eprintln!("uBlock Origin Lite не установился, работает только свой список доменов");
                    return;
                };
                if self.store.settings.ubol_version != extension::VERSION {
                    self.store.settings.ubol_version = extension::VERSION.to_string();
                    self.store.save_settings();
                }
                platform::enable_extension(&self.ui, &id, self.store.settings.adblock);
                self.ubol_id = Some(id);
                // дальше рекламу режет uBlock Origin Lite внутри движка — свой перехват не нужен
                for webview in self.tabs.iter().filter_map(|t| t.webview.as_ref()) {
                    platform::set_adblock_filter(webview, false);
                }
                let ubol = self.ubol_id.as_deref().zip(self.ubol_folder.as_deref());
                register_icons(&self.extensions, ubol);
                self.refresh_extensions();
                self.dirty = true;
            }
        }
    }

    /// Обновляет адрес и доступность «назад/вперёд» по данным самого webview.
    fn sync_location(&mut self, id: u32) {
        let Some(tab) = self.tab_mut(id) else { return };
        let Some(webview) = &tab.webview else { return };
        if let Ok(url) = webview.url() {
            if url != "about:blank" {
                tab.url = url;
            }
        }
        (tab.can_back, tab.can_forward) = platform::nav_state(webview);
        self.dirty = true;
    }

    fn handle_hotkey(&mut self, hotkey: Hotkey) {
        match hotkey {
            Hotkey::NewTab => self.new_tab(None),
            Hotkey::CloseTab => self.close_tab(self.active),
            Hotkey::ReopenTab => {
                if let Some(saved) = self.closed.pop() {
                    self.new_tab(Some(saved.url));
                    if saved.pinned {
                        self.toggle_pin(self.active);
                    } else if saved.group != 0 {
                        // группа ещё есть — в неё; закрылась целиком — возвращаем и её
                        if !self.groups.iter().any(|g| g.id == saved.group) {
                            if let Some(i) = self.closed_groups.iter().rposition(|g| g.id == saved.group) {
                                let group = self.closed_groups.remove(i);
                                self.groups.push(group);
                            }
                        }
                        if self.groups.iter().any(|g| g.id == saved.group) {
                            self.set_group(self.active, saved.group);
                        }
                    }
                }
            }
            Hotkey::InPrivate => platform::open_inprivate(),
            Hotkey::NewWindow => system::open_new_window(&[]),
            Hotkey::FocusUrl => self.focus_url(),
            Hotkey::NextTab => self.cycle_tab(1),
            Hotkey::PrevTab => self.cycle_tab(-1),
            Hotkey::SelectTab(number) => {
                let index = match number {
                    9 => self.tabs.len() - 1,
                    n => n as usize - 1,
                };
                if let Some(tab) = self.tabs.get(index) {
                    self.activate(tab.id);
                }
            }
            Hotkey::Reload => self.reload(),
            Hotkey::Back => {
                if let Some(webview) = self.active_webview() {
                    platform::go_back(webview);
                }
            }
            Hotkey::Forward => {
                if let Some(webview) = self.active_webview() {
                    platform::go_forward(webview);
                }
            }
            Hotkey::Bookmark => self.toggle_bookmark(),
            Hotkey::Bookmarks => self.open_hub("bookmarks"),
            Hotkey::History => self.open_hub("history"),
            Hotkey::Downloads => self.open_hub("downloads"),
            Hotkey::Settings => self.open_hub("settings"),
            Hotkey::Find => self.open_find(),
            Hotkey::PasteGo => self.paste_and_go(),
            Hotkey::TaskManager => self.open_hub("tasks"),
            Hotkey::Panel => self.toggle_panel(),
            Hotkey::SiteOnly => self.toggle_site_only(),
            Hotkey::Compact => {
                let on = !self.store.settings.compact;
                self.store.settings.compact = on;
                self.store.save_settings();
                self.dirty = true;
            }
            Hotkey::Split => {
                let inside = self.split.is_some_and(|(l, r, _)| self.active == l || self.active == r);
                let cmd = if inside { r#"{"cmd":"unsplit"}"# } else { r#"{"cmd":"split"}"# };
                self.handle_ui(cmd);
            }
            Hotkey::Pip => {
                if let Some(webview) = self.active_webview() {
                    platform::run_with_gesture(webview, PIP_SCRIPT);
                }
            }
            Hotkey::ZoomIn => self.zoom_step(1),
            Hotkey::ZoomOut => self.zoom_step(-1),
            Hotkey::ZoomReset => self.zoom_step(0),
            Hotkey::Print => {
                if let Some(webview) = self.active_webview() {
                    let _ = webview.print();
                }
            }
            Hotkey::DevTools => {
                if let Some(webview) = self.active_webview() {
                    webview.open_devtools();
                }
            }
            Hotkey::FavBar => self.set_setting("favbar", &json!(!self.store.settings.favbar)),
            Hotkey::Fullscreen => {
                self.fullscreen = !self.fullscreen;
                self.apply_fullscreen();
            }
            Hotkey::Menu => {
                let _ = self.ui.focus();
                let _ = self.ui.evaluate_script("ui.openMenu()");
            }
            Hotkey::Escape => {
                if self.keys.find_open.load(Ordering::Relaxed) {
                    self.close_find();
                    let _ = self.ui.evaluate_script("ui.closeFind()");
                } else if self.fullscreen {
                    self.fullscreen = false;
                    self.apply_fullscreen();
                }
            }
        }
    }

    fn apply_fullscreen(&mut self) {
        let full = self.is_fullscreen();
        self.keys.fullscreen.store(self.fullscreen, Ordering::Relaxed);
        self.window.set_fullscreen(full.then_some(Fullscreen::Borderless(None)));
        self.reset_overlay();
        self.dirty = true;
        self.layout();
        self.focus_content();
    }

    /// «Вставить и перейти»: текст из буфера — адрес или поисковый запрос.
    fn paste_and_go(&mut self) {
        let Some(text) = platform::clipboard_text() else { return };
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            return;
        }
        let url = to_url(&text, search_url(&self.store.settings.search));
        if self.active_tab().is_some() {
            self.navigate(self.active, &url);
        }
    }

    /// Масштаб, который выбрал человек (кнопки, Ctrl+колёсико): запоминается для
    /// сайта и сразу применяется к его другим вкладкам.
    fn remember_zoom(&mut self, id: u32, factor: f64) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.zoom = factor;
        let host = adblock::host_of(&tab.url).map(str::to_string);
        self.dirty = true;
        let Some(host) = host else { return };
        if !self.store.set_site_zoom(&host, factor) {
            return;
        }
        for other in &mut self.tabs {
            let same = other.id != id && adblock::host_of(&other.url) == Some(host.as_str());
            if let (true, Some(webview)) = (same, &other.webview) {
                let _ = webview.zoom(factor);
                other.zoom = factor;
            }
        }
    }

    /// После перехода на другой сайт — его масштаб (или 100 %), как в Edge.
    fn apply_site_zoom(&mut self, id: u32) {
        let Some(tab) = self.tab(id) else { return };
        let Some(host) = adblock::host_of(&tab.url) else { return };
        let target = self.store.zoom_for(host);
        if (tab.zoom - target).abs() > 0.005 {
            if let Some(webview) = &tab.webview {
                let _ = webview.zoom(target);
            }
            // сама программа меняет масштаб без события — запоминаем значение здесь
            if let Some(tab) = self.tab_mut(id) {
                tab.zoom = target;
            }
            self.dirty = true;
        }
    }

    fn restore_scroll(&mut self, id: u32) {
        let Some(tab) = self.tab_mut(id) else { return };
        let Some(y) = tab.restore_scroll.take() else { return };
        if let Some(webview) = &tab.webview {
            let _ = webview.evaluate_script(&format!("scrollTo(0, {y})"));
        }
    }

    fn reload(&self) {
        if let Some(webview) = self.active_webview() {
            let _ = webview.reload();
        }
    }

    fn zoom_step(&mut self, direction: i32) {
        let Some(tab) = self.active_tab() else { return };
        let Some(webview) = &tab.webview else { return };
        let current = tab.zoom;
        let next = match direction {
            0 => Some(1.0),
            d if d > 0 => ZOOM_LEVELS.iter().copied().find(|z| *z > current + 0.001),
            _ => ZOOM_LEVELS.iter().rev().copied().find(|z| *z < current - 0.001),
        }
        .unwrap_or(current);
        let _ = webview.zoom(next);
        let id = tab.id;
        self.remember_zoom(id, next);
        let _ = self.ui.evaluate_script("ui.zoomChanged()");
    }

    fn open_hub(&self, section: &str) {
        let _ = self.ui.focus();
        let _ = self.ui.evaluate_script(&format!("ui.openHub({section:?})"));
    }

    fn open_find(&mut self) {
        if self.active_webview().is_none() {
            return;
        }
        self.keys.find_open.store(true, Ordering::Relaxed);
        let _ = self.ui.focus();
        let _ = self.ui.evaluate_script("ui.openFind()");
    }

    fn close_find(&mut self) {
        if !self.keys.find_open.swap(false, Ordering::Relaxed) {
            return;
        }
        if let Some(webview) = self.active_webview() {
            platform::find_stop(webview);
        }
    }

    fn toggle_bookmark(&mut self) {
        let Some(tab) = self.active_tab() else { return };
        if tab.url.is_empty() {
            return;
        }
        let (url, title) = (tab.url.clone(), tab.title.clone());
        self.store.reload_bookmarks(); // их могло изменить другое окно
        self.store.toggle_bookmark(&url, &title);
        self.store.save_bookmarks();
        self.dirty = true;
    }

    fn handle_download(&mut self, event: DownloadEvent) {
        match event {
            DownloadEvent::Started { id, url, path } => {
                self.last_download = Some(Instant::now());
                let name = Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                self.downloads.push(Download {
                    id,
                    url,
                    name,
                    path,
                    received: 0,
                    total: 0,
                    state: "progress".into(),
                });
                let _ = self.ui.evaluate_script("ui.downloadStarted()");
            }
            DownloadEvent::Progress { id, received, total } => {
                if let Some(download) = self.downloads.iter_mut().find(|d| d.id == id) {
                    download.received = received;
                    download.total = total;
                }
            }
            DownloadEvent::State { id, state, received } => {
                if let Some(download) = self.downloads.iter_mut().find(|d| d.id == id) {
                    download.state = state.into();
                    download.received = received;
                    if state == "done" {
                        download.total = received;
                    }
                }
                if matches!(state, "done" | "canceled" | "failed") {
                    self.save_downloads();
                }
            }
        }
        self.dirty = true;
    }

    /// Список загрузок переживает перезапуск; незаконченные считаются прерванными.
    fn save_downloads(&self) {
        let finished: Vec<&Download> = self
            .downloads
            .iter()
            .filter(|d| !matches!(d.state.as_str(), "progress" | "paused"))
            .collect();
        let start = finished.len().saturating_sub(MAX_SAVED_DOWNLOADS);
        self.store.write_named("downloads.json", &finished[start..].to_vec());
    }

    fn load_downloads(&mut self) {
        let mut list: Vec<Download> = self.store.read_named("downloads.json");
        for download in &mut list {
            if matches!(download.state.as_str(), "progress" | "paused") {
                download.state = "failed".into();
            }
        }
        platform::continue_download_ids(list.iter().map(|d| d.id).max().unwrap_or(0));
        self.downloads = list;
    }

    // ---------- меню и библиотека поверх страницы ----------

    /// Меню рисуется в интерфейсе, а он — отдельное окно над страницей. Чтобы меню
    /// легло поверх страницы, интерфейс растягивается на всё окно и показывает под
    /// меню снимок страницы. Снимок делается асинхронно, поэтому растягиваем потом.
    fn request_full_overlay(&mut self) {
        if self.overlay == Overlay::Full || self.overlay_pending {
            let _ = self.ui.evaluate_script("ui.overlayReady()");
            return;
        }
        if self.active_webview().is_none() {
            self.finish_overlay(None); // стартовая страница и так на всё окно
            return;
        }
        self.overlay_pending = true;
        self.overlay_requested = Instant::now();
        let proxy = self.proxy.clone();
        if let Some(webview) = self.active_webview() {
            platform::capture(webview, move |bytes| {
                let _ = proxy.send_event(UserEvent::Captured(bytes));
            });
        }
    }

    fn finish_overlay(&mut self, bytes: Option<Vec<u8>>) {
        let has_page = self.active_webview().is_some();
        if has_page && !self.overlay_pending {
            return; // меню успели закрыть, пока делался снимок
        }
        self.overlay_pending = false;
        // снимок отдаётся картинкой по адресу rb.localhost, а не строкой base64 в сотни КБ
        let backdrop = match bytes {
            Some(bytes) if has_page => {
                let version = BACKDROP_VERSION.fetch_add(1, Ordering::Relaxed) + 1;
                if let Ok(mut slot) = BACKDROP.lock() {
                    *slot = bytes;
                }
                format!("'http://rb.localhost/backdrop.jpg?v={version}'")
            }
            _ => "null".to_string(),
        };
        let _ = self.ui.evaluate_script(&format!("ui.setBackdrop({backdrop})"));
        // Вырез раскроется по сигналу overlay_shown — когда слой уже нарисован.
        self.overlay = Overlay::Full;
        let _ = self.ui.focus();
        let _ = self.ui.evaluate_script("ui.overlayReady()");
    }

    // ---------- настройки ----------

    fn set_setting(&mut self, key: &str, value: &Value) {
        let settings = &mut self.store.settings;
        match key {
            "theme" => {
                if let Some(theme) = value.as_str().filter(|t| THEMES.contains(t)) {
                    settings.theme = theme.to_string();
                    AMOLED.store(theme == "amoled", Ordering::Relaxed);
                    THEME_BACKDROP.store(theme_backdrop(theme), Ordering::Relaxed);
                    // плоские темы — и стартовая без цветных разводов
                    if matches!(theme, "lumi" | "ember" | "moss" | "steel" | "ryoku")
                        && matches!(settings.wallpaper.as_str(), "aurora" | "sunset" | "ocean" | "forest" | "night" | "flow")
                    {
                        settings.wallpaper = "mono".into();
                    }
                    self.store.save_settings();
                    self.apply_theme();
                }
                return;
            }
            "search" => {
                if let Some(id) = value.as_str() {
                    if ENGINES.iter().any(|(engine, ..)| *engine == id) {
                        settings.search = id.to_string();
                    }
                }
            }
            "adblock" => {
                settings.adblock = value.as_bool().unwrap_or(settings.adblock);
                self.adblock.store(settings.adblock, Ordering::Relaxed);
                if let Some(id) = &self.ubol_id {
                    platform::enable_extension(&self.ui, id, settings.adblock);
                }
            }
            "restore_session" => {
                settings.restore_session = value.as_bool().unwrap_or(settings.restore_session)
            }
            "favbar" => settings.favbar = value.as_bool().unwrap_or(settings.favbar),
            "passwords" => {
                settings.passwords = value.as_bool().unwrap_or(settings.passwords);
            }
            "efficiency_mode" => {
                settings.efficiency_mode = value.as_bool().unwrap_or(settings.efficiency_mode);
                self.store.save_settings();
                self.apply_efficiency();
                self.dirty = true;
                return;
            }
            "battery_saver" => settings.battery_saver = value.as_bool().unwrap_or(settings.battery_saver),
            "accent" => match value.as_str() {
                Some(color) if color.is_empty() || is_hex_color(color) => settings.accent = color.to_lowercase(),
                _ => return,
            },
            "tint_frame" => settings.tint_frame = value.as_bool().unwrap_or(settings.tint_frame),
            "wallpaper_always" => {
                settings.wallpaper_always = value.as_bool().unwrap_or(settings.wallpaper_always)
            }
            "motion" | "bar_style" | "newtab_layout" | "splash" | "tile_size" => {
                let allowed: &[&str] = match key {
                    "motion" => &["smooth", "fast", "bouncy", "off"],
                    "tile_size" => &["s", "m", "l"],
                    "splash" => &["compass", "aurora", "warp", "minimal", "off"],
                    "bar_style" => &["classic", "compact", "floating", "vertical", "island"],
                    _ => &["clock", "minimal", "tiles", "greeting"],
                };
                let Some(choice) = value.as_str().filter(|v| allowed.contains(v)) else { return };
                match key {
                    "motion" => settings.motion = choice.to_string(),
                    "bar_style" => settings.bar_style = choice.to_string(),
                    "splash" => settings.splash = choice.to_string(),
                    "tile_size" => settings.tile_size = choice.to_string(),
                    _ => settings.newtab_layout = choice.to_string(),
                }
            }
            "sleep_minutes" => {
                if let Some(minutes) = value.as_u64() {
                    settings.sleep_minutes = minutes.min(24 * 60) as u32;
                }
            }
            "wallpaper" => match value.as_str() {
                // своё фото можно выбрать, только если оно уже загружено
                Some("image") if wallpaper_file().is_none() => return,
                Some("video") if video_file().is_none() => return,
                Some(live) if live.strip_prefix("live:").is_some_and(|id| live_wall_file(id).is_some()) => {
                    settings.wallpaper = live.to_string()
                }
                Some(name) if WALLPAPERS.contains(&name) => settings.wallpaper = name.to_string(),
                _ => return,
            },
            "island_width" => {
                if let Some(width) = value.as_u64() {
                    settings.island_width = width.clamp(40, 100) as u32;
                }
            }
            "island_radius" => {
                if let Some(radius) = value.as_u64() {
                    settings.island_radius = radius.min(100) as u32;
                }
            }
            "island_tabs" => settings.island_tabs = value.as_bool().unwrap_or(settings.island_tabs),
            "auto_update" => settings.auto_update = value.as_bool().unwrap_or(settings.auto_update),
            "compact" => settings.compact = value.as_bool().unwrap_or(settings.compact),
            "url_in_tab" => settings.url_in_tab = value.as_bool().unwrap_or(settings.url_in_tab),
            "shortcuts" => {
                let Ok(list) = serde_json::from_value::<Vec<store::Shortcut>>(value.clone()) else { return };
                settings.shortcuts = list
                    .into_iter()
                    .filter(|s| platform::hotkey_by_id(&s.action).is_some() && s.key > 0 && s.key < 256 && s.mods < 8)
                    .map(|s| store::Shortcut { label: s.label.chars().take(40).collect(), ..s })
                    .collect();
                apply_shortcuts(&self.keys, &settings.shortcuts);
            }
            "process_limit" => {
                // применяется после перезапуска: аргументы движка задаются при его старте
                if let Some(limit @ (0 | 2 | 4 | 6 | 8 | 12)) = value.as_u64() {
                    settings.process_limit = limit as u32;
                }
            }
            "left_click_background" => {
                settings.left_click_background = value.as_bool().unwrap_or(settings.left_click_background);
                LEFT_CLICK_BACKGROUND.store(settings.left_click_background, Ordering::Relaxed);
            }
            "hidden_tools" | "hidden_menu" => {
                let list: Vec<String> = serde_json::from_value::<Vec<String>>(value.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|k| !k.is_empty() && k.len() < 40)
                    .take(64)
                    .collect();
                if key == "hidden_tools" {
                    settings.hidden_tools = list;
                } else {
                    settings.hidden_menu = list;
                }
            }
            "dark_all" => {
                settings.dark_all = value.as_bool().unwrap_or(settings.dark_all);
                self.store.save_settings();
                self.apply_dark_sites();
                self.dirty = true;
                return;
            }
            "buttons" => {
                // только http(s), не больше 12 кнопок, короткие названия
                let Ok(list) = serde_json::from_value::<Vec<store::CustomButton>>(value.clone()) else { return };
                settings.buttons = list
                    .into_iter()
                    .filter(|b| (b.url.starts_with("https://") || b.url.starts_with("http://")) && b.url.len() < 2000)
                    .map(|b| store::CustomButton { name: b.name.chars().take(40).collect(), url: b.url })
                    .take(12)
                    .collect();
            }
            "glass" => {
                if let Some(glass) = value.as_u64() {
                    settings.glass = glass.min(70) as u32;
                }
            }
            "wallpaper_dim" => {
                if let Some(dim) = value.as_u64() {
                    settings.wallpaper_dim = dim.min(85) as u32;
                }
            }
            _ => return,
        }
        self.store.save_settings();
        self.dirty = true;
    }

    /// Копирует выбранное фото в папку данных: обои не пропадут, если исходник удалят.
    fn set_wallpaper_image(&mut self, source: &Path) {
        let ext = source
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !IMAGE_TYPES.iter().any(|(known, _)| *known == ext) {
            return;
        }
        if let Some((old, _)) = wallpaper_file() {
            let _ = std::fs::remove_file(old);
        }
        if std::fs::copy(source, data_dir().join(format!("wallpaper.{ext}"))).is_err() {
            return;
        }
        let settings = &mut self.store.settings;
        settings.wallpaper = "image".into();
        settings.wallpaper_version += 1;
        self.store.save_settings();
        self.dirty = true;
    }

    /// Своё видео для обоев: копия в папке данных, как и у фото.
    fn set_wallpaper_video(&mut self, source: &Path) {
        let ext = source.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !VIDEO_TYPES.iter().any(|(known, _)| *known == ext) {
            return;
        }
        if let Some((old, _)) = video_file() {
            let _ = std::fs::remove_file(old);
        }
        if std::fs::copy(source, data_dir().join(format!("wallpaper-video.{ext}"))).is_err() {
            return;
        }
        let settings = &mut self.store.settings;
        settings.wallpaper = "video".into();
        settings.wallpaper_version += 1;
        self.store.save_settings();
        self.dirty = true;
    }

    fn set_download_dir(&mut self, dir: String) {
        if let Ok(mut shared) = self.download_dir.lock() {
            shared.clone_from(&dir);
        }
        self.store.settings.download_dir = dir;
        self.store.save_settings();
        self.dirty = true;
    }

    // ---------- команды интерфейса ----------

    fn handle_ui(&mut self, raw: &str) {
        let Ok(message) = serde_json::from_str::<Value>(raw) else { return };
        let text = |key: &str| message[key].as_str().unwrap_or("").to_string();
        let number = |key: &str| message[key].as_u64().unwrap_or(0);
        let id = number("id") as u32;

        match message["cmd"].as_str().unwrap_or("") {
            "ready" => {
                self.sent.clear(); // интерфейс перезагрузился — нужно всё состояние
                self.dirty = true;
            }
            "peek" => {
                let rect = ["x", "y", "w", "h"].map(|k| message[k].as_f64());
                let peek = rect.iter().all(Option::is_some).then(|| rect.map(|v| v.unwrap_or(0.0)));
                if peek != self.peek {
                    self.peek = peek;
                    self.layout();
                }
            }
            "toast" => {
                let size = message["w"].as_f64().zip(message["h"].as_f64());
                if size != self.toast {
                    self.toast = size;
                    self.layout();
                }
            }
            "chrome" => {
                let height = message["height"].as_f64().unwrap_or(DEFAULT_CHROME);
                let side = message["side"].as_f64().unwrap_or(0.0);
                let float = message["float"]["top"].as_f64().map(|top| {
                    let parts = message["float"]["parts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|p| {
                            let v: Vec<f64> = p.as_array()?.iter().filter_map(Value::as_f64).collect();
                            (v.len() == 5).then(|| [v[0], v[1], v[2], v[3], v[4]])
                        })
                        .take(8)
                        .collect();
                    (top, parts)
                });
                let changed = (height - self.chrome_height).abs() > 0.5
                    || (side - self.chrome_side).abs() > 0.5
                    || float != self.chrome_float;
                if changed {
                    self.chrome_height = height;
                    self.chrome_side = side;
                    self.chrome_float = float;
                    self.layout();
                }
            }

            // окно
            "minimize" => self.window.set_minimized(true),
            "maximize" => self.window.set_maximized(!self.window.is_maximized()),
            "close" => self.quit = true,
            "drag" => {
                let _ = self.window.drag_window();
            }
            "resize" => {
                let direction = match message["dir"].as_str().unwrap_or("") {
                    "n" => ResizeDirection::North,
                    "ne" => ResizeDirection::NorthEast,
                    "nw" => ResizeDirection::NorthWest,
                    _ => return,
                };
                let _ = self.window.drag_resize_window(direction);
            }
            "fullscreen" => self.handle_hotkey(Hotkey::Fullscreen),
            "inprivate" => platform::open_inprivate(),
            "new_window" => system::open_new_window(&[]),
            "detach" => {
                // вкладка уезжает в новое окно: там она откроется заново
                let Some(url) = self.tab(id).map(|t| t.url.clone()).filter(|u| !u.is_empty()) else {
                    return;
                };
                system::open_new_window(&[url]);
                self.close_tab(id);
            }
            "import_bookmarks" => {
                let list = pages::read_bookmarks(&text("source"));
                self.store.reload_bookmarks();
                let added = self.store.add_bookmarks(list);
                self.store.save_bookmarks();
                self.dirty = true;
                let _ = self.ui.evaluate_script(&format!("ui.imported({added})"));
            }
            "set_proxy" => {
                let mode = text("mode");
                let host = text("host").trim().to_string();
                let port = number("port");
                let valid = match mode.as_str() {
                    "off" => true,
                    "http" | "socks5" => valid_proxy_host(&host) && (1..=65535).contains(&port),
                    _ => false,
                };
                if !valid {
                    let _ = self.ui.evaluate_script("ui.proxyInvalid()");
                    return;
                }
                let settings = &mut self.store.settings;
                settings.proxy_mode = mode;
                settings.proxy_host = host;
                settings.proxy_port = port as u16;
                self.store.save_settings();
                // прокси меняется только при запуске движка — перезапускаемся, вкладки вернутся
                self.save_session();
                system::restart(platform::browser_pid(&self.ui));
                self.quit = true;
            }
            "toggle_panel" => self.toggle_panel(),
            "make_default" => {
                if system::register() {
                    system::open_default_apps_settings();
                }
            }

            // вкладки
            "new_tab" => self.new_tab(None),
            "close_tab" => self.close_tab(id),
            "activate" => self.activate(id),
            "move_tab" => self.move_tab(id, number("index") as usize),
            "pin" => self.toggle_pin(id),
            "duplicate" => {
                if let Some(url) = self.tab(id).map(|t| t.url.clone()).filter(|u| !u.is_empty()) {
                    self.new_tab(Some(url));
                }
            }
            "close_others" => {
                let others = self.tabs.iter().filter(|t| t.id != id && !t.pinned).map(|t| t.id);
                let others: Vec<u32> = others.collect();
                self.activate(id);
                self.close_many(others);
            }
            "close_right" => {
                let Some(index) = self.index_of(id) else { return };
                let right: Vec<u32> = self.tabs[index + 1..].iter().map(|t| t.id).collect();
                self.close_many(right);
            }
            "reload_tab" => {
                if let Some(webview) = self.tab(id).and_then(|t| t.webview.as_ref()) {
                    let _ = webview.reload();
                }
            }
            "mute" => {
                if let Some(tab) = self.tab_mut(id) {
                    tab.muted = !tab.muted;
                    if let Some(webview) = &tab.webview {
                        platform::set_muted(webview, tab.muted);
                    }
                    self.dirty = true;
                }
            }

            // навигация
            "navigate" => {
                let input = text("text");
                if !input.trim().is_empty() {
                    let url = to_url(&input, search_url(&self.store.settings.search));
                    self.navigate(self.active, &url);
                }
            }
            "open" => {
                let url = text("url");
                if message["background"].as_bool().unwrap_or(false) {
                    self.new_background_tab(&url);
                } else if message["newTab"].as_bool().unwrap_or(false) {
                    self.new_tab(Some(url));
                } else {
                    self.navigate(self.active, &url);
                }
            }
            "back" => self.handle_hotkey(Hotkey::Back),
            "forward" => self.handle_hotkey(Hotkey::Forward),
            "reload" => self.reload(),
            "stop" => {
                if let Some(webview) = self.active_webview() {
                    platform::stop(webview);
                }
            }
            "focus_content" => self.focus_content(),
            "clipboard" => {
                let text = platform::clipboard_text().unwrap_or_default();
                let text = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
                let _ = self.ui.evaluate_script(&format!("ui.pasteText({text})"));
            }
            "paste_go" => self.paste_and_go(),
            "system_keys" => {
                let _ = self.ui.focus();
                platform::press_system_keys(&text("keys"));
            }

            // страница
            "zoom" => match text("dir").as_str() {
                "in" => self.zoom_step(1),
                "out" => self.zoom_step(-1),
                _ => self.zoom_step(0),
            },
            "print" => self.handle_hotkey(Hotkey::Print),
            "devtools" => self.handle_hotkey(Hotkey::DevTools),
            "find" => {
                self.keys.find_open.store(true, Ordering::Relaxed);
                if let Some(webview) = self.active_webview() {
                    platform::find_start(webview, &text("term"));
                }
            }
            "find_next" | "find_prev" => {
                if let Some(webview) = self.active_webview() {
                    platform::find_step(webview, message["cmd"] == "find_next");
                }
            }
            "find_close" => {
                self.close_find();
                self.focus_content();
            }

            // слои поверх страницы
            "pip" => {
                if let Some(webview) = self.active_webview() {
                    platform::run_with_gesture(webview, PIP_SCRIPT);
                }
            }
            "translate" => {
                let Some(tab) = self.active_tab() else { return };
                let target = if message["original"].as_bool().unwrap_or(false) {
                    untranslate_url(&tab.url)
                } else {
                    translate_url(&tab.url)
                };
                if let Some(target) = target {
                    self.navigate(self.active, &target);
                }
            }
            "dark_site" => {
                // тёмная тема для сайта во вкладке: включить или выключить
                let Some(host) = self.active_tab().and_then(|t| adblock::host_of(&t.url)).map(|h| h.trim_start_matches("www.").to_string()) else { return };
                let list = &mut self.store.settings.dark_sites;
                if let Some(at) = list.iter().position(|h| *h == host) {
                    list.remove(at);
                } else {
                    list.push(host);
                }
                self.store.save_settings();
                self.apply_dark_sites();
            }
            "boost" => {
                // усиление звука для сайта во вкладке: 0…600 %, запоминается для сайта
                let Some(host) = self.active_tab().and_then(|t| adblock::host_of(&t.url)).map(|h| h.trim_start_matches("www.").to_string()) else { return };
                let level = (number("level") as u32).min(600);
                if level == 100 {
                    self.store.settings.site_boost.remove(&host);
                } else {
                    self.store.settings.site_boost.insert(host, level);
                }
                self.store.save_settings();
                let script = boost_sites_script(&self.store.settings);
                for tab in &self.tabs {
                    if let Some(webview) = &tab.webview {
                        platform::set_document_script(webview, &tab.boost_script, Some(&script));
                    }
                }
                for id in self.visible_with(self.active) {
                    if let Some(webview) = self.tab(id).and_then(|t| t.webview.as_ref()) {
                        let _ = webview.evaluate_script(&format!("window.__rbBoost && __rbBoost({})", level as f64 / 100.0));
                    }
                }
                self.dirty = true;
            }
            "dark_site_remove" => {
                let host = text("host");
                self.store.settings.dark_sites.retain(|h| *h != host);
                self.store.save_settings();
                self.apply_dark_sites();
            }
            "prewarm" => {
                // наведение на плитку новой вкладки: начинаем грузить сайт заранее
                let url = text("url");
                if self.active_tab().is_none_or(|t| !t.is_start()) {
                    return;
                }
                if !url.starts_with("http") || self.prewarm.as_ref().is_some_and(|(_, u, _)| *u == url) {
                    return;
                }
                self.prewarm = None;
                let id = self.next_id;
                self.next_id += 1;
                let bounds = self.page_rect();
                if let Ok(webview) = build_tab(self, id, bounds) {
                    let _ = webview.set_visible(false);
                    let _ = webview.load_url(&url);
                    platform::raise(&self.ui);
                    self.prewarm = Some((id, url, webview));
                }
            }
            "prewarm_cancel" => self.prewarm = None,
            "split" => {
                // разделить экран: текущая вкладка слева, выбранная (или предыдущая) справа
                let other = message["id"].as_u64().map(|v| v as u32).filter(|o| *o != self.active).or_else(|| {
                    self.tabs.iter().filter(|t| t.id != self.active && !t.url.is_empty()).map(|t| t.id).last()
                });
                let Some(other) = other else {
                    let _ = self.ui.evaluate_script("ui.toast('Откройте ещё одну вкладку с сайтом, чтобы разделить экран')");
                    return;
                };
                self.start_split(self.active, other);
            }
            "unsplit" => {
                let partner = self.visible_with(self.active).into_iter().find(|t| *t != self.active);
                self.split = None;
                if let Some(partner) = partner {
                    self.hide_tab(partner);
                }
                self.layout();
                self.dirty = true;
            }
            "split_swap" => {
                if let Some((l, r, ratio)) = self.split {
                    self.split = Some((r, l, 1.0 - ratio));
                    self.layout();
                    self.dirty = true;
                }
            }
            "group_new" => {
                let id = number("id") as u32;
                if self.tab(id).is_none() {
                    return; // вкладку уже закрыли — пустая группа не нужна
                }
                // номера закрытых групп не занимаем: их ещё могут вернуть
                let gid = self.groups.iter().chain(&self.closed_groups).map(|g| g.id).max().unwrap_or(0) + 1;
                const COLORS: [&str; 8] = ["blue", "purple", "pink", "red", "orange", "yellow", "green", "cyan"];
                let color = COLORS[(gid as usize - 1) % COLORS.len()].to_string();
                self.groups.push(store::TabGroup { id: gid, name: String::new(), color, collapsed: false });
                self.set_group(id, gid);
                let _ = self.ui.evaluate_script(&format!("ui.groupCreated({gid})"));
            }
            "group_add" => self.set_group(number("id") as u32, number("group") as u32),
            "group_new_tab" => {
                let gid = number("group") as u32;
                self.new_tab(None);
                let id = self.active;
                self.set_group(id, gid);
            }
            "group_remove" => self.set_group(number("id") as u32, 0),
            "group_edit" => {
                let gid = number("group") as u32;
                if let Some(group) = self.groups.iter_mut().find(|g| g.id == gid) {
                    if let Some(name) = message["name"].as_str() {
                        group.name = name.chars().take(40).collect();
                    }
                    if let Some(color) = message["color"].as_str().filter(|c| c.len() < 16) {
                        group.color = color.to_string();
                    }
                    if let Some(collapsed) = message["collapsed"].as_bool() {
                        group.collapsed = collapsed;
                    }
                }
                // свернули группу с активной вкладкой — переходим на вкладку вне группы
                let collapsed = self.groups.iter().any(|g| g.id == gid && g.collapsed);
                if collapsed && self.active_tab().is_some_and(|t| t.group == gid) {
                    if let Some(next) = self.tab_outside_collapsed(gid) {
                        self.activate(next);
                    }
                }
                self.dirty = true;
                self.session_dirty = true;
            }
            "group_ungroup" => {
                let gid = number("group") as u32;
                for tab in &mut self.tabs {
                    if tab.group == gid {
                        tab.group = 0;
                    }
                }
                self.prune_groups();
            }
            "group_close" => {
                let gid = number("group") as u32;
                let ids: Vec<u32> = self.tabs.iter().filter(|t| t.group == gid).map(|t| t.id).collect();
                self.close_many(ids);
            }
            "install" => {
                let walls = live_walls().0.clone();
                match installer::install(Some(&walls)) {
                    Ok(path) => {
                        if self.store.settings.update_source.is_empty() {
                            self.store.settings.update_source = installer::current_exe().to_string_lossy().to_string();
                            self.store.save_settings();
                        }
                        let _ = self.ui.evaluate_script("ui.toast('Установлено: Rust Browser есть в «Пуске» и на рабочем столе. Запускаю установленную копию')");
                        // переезжаем в установленную копию
                        self.save_session();
                        self.store.save_all();
                        let _ = std::process::Command::new(&path).arg(format!("--wait-pid={}", std::process::id())).spawn();
                        self.quit = true;
                    }
                    Err(error) => {
                        let _ = self.ui.evaluate_script(&format!("ui.toast({:?})", format!("Установка не удалась: {error}")));
                    }
                }
            }
            "check_update" => self.check_update(true),
            "keys_record" => self.keys.recording.store(message["on"].as_bool().unwrap_or(false), Ordering::Relaxed),
            "__toast_latest" => {
                let _ = self.ui.evaluate_script("ui.toast('У вас последняя версия')");
            }
            "restart_update" => {
                self.save_session();
                self.store.save_all();
                system::restart(None);
                self.quit = true;
            }
            "pick_update_source" => {
                if let Some(path) = platform::pick_file(self.window.hwnd(), "Сборка Rust Browser", "*.exe") {
                    self.store.settings.update_source = path;
                    self.store.save_settings();
                    self.dirty = true;
                }
            }
            "my_button" => {
                // уже открыт этот сайт — переключаемся на его вкладку, иначе открываем новую
                let url = text("url");
                let host = adblock::host_of(&url).map(str::to_string);
                let open = host.as_deref().and_then(|host| {
                    self.tabs.iter().find(|t| adblock::host_of(&t.url) == Some(host)).map(|t| t.id)
                });
                match open {
                    Some(id) if !message["newTab"].as_bool().unwrap_or(false) => self.activate(id),
                    _ if url.starts_with("http") => self.new_tab(Some(url)),
                    _ => {}
                }
            }
            "ext_scan" => {
                let installed: Vec<String> = self.ext_list.iter().map(|(id, _, _)| id.clone()).collect();
                self.ext_browsers = webstore::browser_profiles()
                    .into_iter()
                    .map(|(name, profiles)| {
                        let count = webstore::scan(&profiles).iter().filter(|f| !installed.contains(&f.id)).count();
                        (name.to_string(), count)
                    })
                    .collect();
                self.dirty = true;
            }
            "ext_import" => self.import_extensions(&text("browser")),
            "ext_enable" => {
                let id = text("id");
                if self.ubol_id.as_deref() == Some(id.as_str()) {
                    return; // у блокировщика свой переключатель
                }
                platform::enable_extension(&self.ui, &id, message["on"].as_bool().unwrap_or(true));
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    let _ = proxy.send_event(UserEvent::ExtChanged);
                });
            }
            "ext_remove" => self.remove_extension(&text("id")),
            "ext_options" => {
                let id = text("id");
                if let Some(info) = self.extension_info(&id).filter(|i| !i.options.is_empty()) {
                    self.new_tab(Some(format!("chrome-extension://{id}/{}", info.options)));
                }
            }
            "ext_popup" => {
                let id = text("id");
                let right = message["right"].as_f64().unwrap_or(0.0);
                let top = message["top"].as_f64().unwrap_or(0.0);
                self.open_ext_popup(&id, right, top);
            }
            "ext_popup_close" => self.close_ext_popup(),
            "sync_enable" => self.sync_enable(None),
            "sync_join" => match sync::parse_pair(&text("code")) {
                Some(pair) => self.sync_enable(Some(pair)),
                None => {
                    let _ = self.ui.evaluate_script("ui.accountError('Код не подходит: скопируйте его целиком, он начинается с RBSYNC1.')");
                }
            },
            "sync_pair" => self.send_pair(),
            "sync_webdav" => self.dav_connect(text("url"), text("user"), text("pass")),
            "sync_webdav_off" => {
                if let Some(account) = &mut self.account {
                    account.webdav = None;
                    account.bases.remove("webdav");
                }
                self.save_account();
                self.sync_error = None;
                self.send_pair();
                self.dirty = true;
            }
            "account_sync" => self.sync_now(true),
            "account_logout" => {
                self.account = None;
                self.sync_error = None;
                if let Ok(mut group) = self.lan_group.lock() {
                    group.clear();
                }
                self.save_account();
                self.dirty = true;
            }
            "pw_check" => {
                if self.pw_checking {
                    return;
                }
                self.pw_checking = true;
                self.dirty = true;
                let list: Vec<String> = self.vault.logins.iter().map(|l| passwords::unprotect(&l.secret).unwrap_or_default()).collect();
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let _ = proxy.send_event(UserEvent::PwHealth(autofill::check(&list)));
                });
            }
            "af_save" => {
                // новый или изменённый адрес / карта; index — что заменить
                let index = message["index"].as_u64().map(|i| i as usize);
                match message["kind"].as_str() {
                    Some("address") => {
                        let Ok(address) = serde_json::from_value::<autofill::Address>(message["value"].clone()) else { return };
                        let list = &mut self.autofill.addresses;
                        match index.filter(|i| *i < list.len()) {
                            Some(i) => list[i] = address,
                            None => list.push(address),
                        }
                    }
                    Some("card") => {
                        let Ok(card) = serde_json::from_value::<autofill::Card>(message["value"].clone()) else { return };
                        if !autofill::luhn_ok(&card.number) {
                            let _ = self.ui.evaluate_script("ui.toast('Проверьте номер карты — в нём ошибка')");
                            return;
                        }
                        self.autofill.cards.push(autofill::Card { number: autofill::digits(&card.number), ..card });
                    }
                    _ => return,
                }
                self.save_autofill();
            }
            "af_delete" => {
                let index = number("index") as usize;
                match message["kind"].as_str() {
                    Some("address") if index < self.autofill.addresses.len() => {
                        self.autofill.addresses.remove(index);
                    }
                    Some("card") if index < self.autofill.cards.len() => {
                        self.autofill.cards.remove(index);
                    }
                    _ => return,
                }
                self.save_autofill();
            }
            "af_fill" => {
                let index = number("index") as usize;
                // сайт, который сейчас на экране: подставляем только в него
                let Some(origin) = self.page_origin() else { return };
                match message["kind"].as_str() {
                    Some("address") => self.fill_autofill("address", index, &origin),
                    Some("card") if index < self.autofill.cards.len() => {
                        // номер карты — только после Windows Hello
                        if self.pw_verified.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
                            self.fill_autofill("card", index, &origin);
                            return;
                        }
                        let proxy = self.proxy.clone();
                        passwords::verify(self.window.hwnd(), "Rust Browser: подтвердите, что это вы, чтобы подставить карту", move |verdict| {
                            let _ = proxy.send_event(UserEvent::CardVerified(index, origin, verdict));
                        });
                    }
                    _ => {}
                }
            }
            "pw_save" => self.save_offered_password(),
            "pw_never" => {
                if let Some(offer) = self.pw_offer.take() {
                    if !self.vault.never.contains(&offer.origin) {
                        self.vault.never.push(offer.origin);
                    }
                    self.save_vault();
                }
            }
            "pw_dismiss" => self.pw_offer = None,
            "pw_fill" => self.fill_password(number("index") as usize),
            "pw_reveal" => self.reveal_password(number("index") as usize, message["copy"].as_bool().unwrap_or(false)),
            "pw_delete" => {
                let index = number("index") as usize;
                if index < self.vault.logins.len() {
                    self.vault.logins.remove(index);
                    if index < self.pw_health.len() {
                        self.pw_health.remove(index);
                    }
                    self.save_vault();
                }
            }
            "pw_unblock" => {
                let origin = text("origin");
                self.vault.never.retain(|o| *o != origin);
                self.save_vault();
            }
            "pw_import" => self.import_passwords(),
            "splash_done" => {
                self.splash = false;
                self.layout();
            }
            "send_to_phone" => {
                let id = message["id"].as_u64().map_or(self.active, |v| v as u32);
                self.send_tab_to_phone(id);
            }
            "overlay_shown" => {
                if self.overlay == Overlay::Full {
                    self.layout();
                }
                // окно раскрыто — теперь можно плавно затемнять фон: анимацию видно с начала
                let _ = self.ui.evaluate_script("ui.overlayVisible()");
            }
            "overlay" => match message["mode"].as_str() {
                Some("full") => self.request_full_overlay(),
                Some("sheet") => {
                    self.overlay = Overlay::Sheet(message["height"].as_f64().unwrap_or(0.0));
                    self.layout();
                }
                _ => {
                    self.overlay = Overlay::None;
                    self.overlay_pending = false;
                    self.layout();
                }
            },

            // адресная строка
            "suggest" => {
                let query = text("text");
                let list = self.store.suggest(&query, 6);
                let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
                // опечатка или не та раскладка — исправление досчитает фоновый поток
                self.spell.ask(number("seq") as u64, &query);
                let script = format!("ui.setSuggestions({}, {json})", number("seq"));
                let _ = self.ui.evaluate_script(&script);
            }

            // библиотека
            "history" => self.send_history(&text("q")),
            "history_remove" => {
                self.store.remove_visit(&text("url"), number("time"));
                self.store.save_history();
            }
            "history_clear" => {
                self.store.clear_history();
                self.store.save_history();
                self.send_history("");
            }
            "bookmark_toggle" => self.toggle_bookmark(),
            "bookmark_remove" => {
                self.store.reload_bookmarks();
                self.store.remove_bookmark(&text("url"));
                self.store.save_bookmarks();
                self.dirty = true;
            }
            // плитки стартовой: правка, добавление, порядок
            "bookmark_edit" => {
                let (old, title) = (text("old"), text("title").trim().chars().take(120).collect::<String>());
                let url = to_url(&text("url"), search_url(&self.store.settings.search));
                if !url.starts_with("http") {
                    return;
                }
                self.store.reload_bookmarks();
                if self.store.bookmarks.iter().any(|b| b.url == url && b.url != old) {
                    let _ = self.ui.evaluate_script("ui.toast('Такой сайт уже есть в избранном')");
                    return;
                }
                if let Some(b) = self.store.bookmarks.iter_mut().find(|b| b.url == old) {
                    b.url = url;
                    b.title = title;
                } else if old.is_empty() {
                    let title = if title.is_empty() { adblock::host_of(&url).unwrap_or("").trim_start_matches("www.").to_string() } else { title };
                    // новая плитка — в конец видимых плиток (после первых, что на стартовой)
                    let at = (number("index") as usize).min(self.store.bookmarks.len());
                    self.store.bookmarks.insert(at, store::Bookmark { url, title });
                }
                self.store.save_bookmarks();
                self.dirty = true;
            }
            "bookmark_move" => {
                let (from, to) = (number("from") as usize, number("to") as usize);
                self.store.reload_bookmarks();
                let len = self.store.bookmarks.len();
                if from < len && to < len && from != to {
                    let b = self.store.bookmarks.remove(from);
                    self.store.bookmarks.insert(to, b);
                    self.store.save_bookmarks();
                    self.dirty = true;
                }
            }
            "download_open" => {
                if let Some(download) = self.downloads.iter().find(|d| d.id == number("id")) {
                    platform::open_path(&download.path);
                }
            }
            "download_reveal" => {
                if let Some(download) = self.downloads.iter().find(|d| d.id == number("id")) {
                    platform::reveal_path(&download.path);
                }
            }
            "downloads_clear" => {
                self.downloads.retain(|d| matches!(d.state.as_str(), "progress" | "paused"));
                self.save_downloads();
                self.dirty = true;
            }
            "download_control" => {
                let (id, action) = (number("id"), text("action"));
                if platform::control_download(id, &action) {
                    if let Some(download) = self.downloads.iter_mut().find(|d| d.id == id) {
                        match action.as_str() {
                            "pause" => download.state = "paused".into(),
                            "resume" => download.state = "progress".into(),
                            _ => {}
                        }
                    }
                    self.dirty = true;
                }
            }
            "download_forget" => {
                self.downloads.retain(|d| d.id != number("id"));
                self.save_downloads();
                self.dirty = true;
            }
            "site_info" => self.send_site_info(),
            "tasks" => {
                self.tasks_sent.borrow_mut().clear(); // открыли заново — список нужен в любом случае
                self.refresh_processes(true);
            }
            "site_only" => self.toggle_site_only(),
            "tasks_watch" => {
                self.tasks_watch = message["on"].as_bool().unwrap_or(false);
                self.last_tasks = Instant::now();
            }
            "unload_tab" => {
                self.unload_tab(id);
                self.refresh_processes(true);
            }
            "unload_background" => {
                let ids: Vec<u32> = self.tabs.iter().map(|t| t.id).collect();
                for id in ids {
                    self.unload_tab(id);
                }
                self.refresh_processes(true);
            }
            "site_clear_cookies" => {
                let Some(webview) = self.active_webview() else { return };
                let url = self.active_tab().map(|t| t.url.clone()).unwrap_or_default();
                let cookies = webview.cookies_for_url(&url).unwrap_or_default();
                for cookie in &cookies {
                    let _ = webview.delete_cookie(cookie);
                }
                let _ = self.ui.evaluate_script(&format!("ui.cookiesCleared({})", cookies.len()));
            }

            // настройки
            "set" => self.set_setting(&text("key"), &message["value"]),
            "adblock_toggle" => {
                let enabled = !self.store.settings.adblock;
                self.set_setting("adblock", &json!(enabled));
            }
            "pick_download_dir" => {
                if let Some(dir) = platform::pick_folder(self.window.hwnd()) {
                    self.set_download_dir(dir);
                }
            }
            "reset_download_dir" => self.set_download_dir(String::new()),
            "pick_wallpaper" => {
                if let Some(path) = platform::pick_image(self.window.hwnd()) {
                    self.set_wallpaper_image(Path::new(&path));
                }
            }
            "pick_wallpaper_video" => {
                if let Some(path) = platform::pick_file(self.window.hwnd(), "Видео", "*.mp4;*.webm") {
                    self.set_wallpaper_video(Path::new(&path));
                }
            }
            "remove_wallpaper_video" => {
                if let Some((file, _)) = video_file() {
                    let _ = std::fs::remove_file(file);
                }
                if self.store.settings.wallpaper == "video" {
                    self.store.settings.wallpaper = "aurora".into();
                }
                self.store.save_settings();
                self.dirty = true;
            }
            "remove_wallpaper" => {
                if let Some((file, _)) = wallpaper_file() {
                    let _ = std::fs::remove_file(file);
                }
                if self.store.settings.wallpaper == "image" {
                    self.store.settings.wallpaper = "aurora".into();
                }
                self.store.save_settings();
                self.dirty = true;
            }
            "open_download_dir" => {
                let mut dir = self.store.settings.download_dir.clone();
                if dir.is_empty() {
                    dir = default_download_dir();
                }
                platform::open_path(&dir);
            }
            "clear_data" => {
                // Профиль общий для всех вкладок, поэтому хватает одного вызова.
                let _ = self.ui.clear_all_browsing_data();
                let _ = self.ui.evaluate_script("ui.dataCleared()");
            }
            _ => {}
        }
    }

    // ---------- процессы, режим эффективности, диспетчер вкладок ----------

    /// Просит у WebView2 список процессов; ответ придёт событием Processes.
    fn refresh_processes(&self, report: bool) {
        let proxy = self.proxy.clone();
        efficiency::processes(&self.ui, move |list| {
            let _ = proxy.send_event(UserEvent::Processes(list, report));
        });
    }

    /// Процессы отрисовки каждой вкладки (общие процессы у вкладок одного сайта).
    fn tab_processes(&self) -> Vec<(u32, Vec<u32>)> {
        self.tabs
            .iter()
            .filter_map(|tab| {
                let frame = efficiency::main_frame_id(tab.webview.as_ref()?)?;
                let pids = self
                    .processes
                    .iter()
                    .filter(|p| p.renderer && p.frames.contains(&frame))
                    .map(|p| p.pid)
                    .collect();
                Some((tab.id, pids))
            })
            .collect()
    }

    /// Фоновым вкладкам — режим эффективности, видимой и играющей звук — обычный.
    fn apply_efficiency(&mut self) {
        let tab_pids: Vec<(bool, Vec<u32>)> = self
            .tab_processes()
            .into_iter()
            .map(|(id, pids)| {
                let keep = id == self.active || self.tab(id).is_some_and(|t| t.audio);
                (keep, pids)
            })
            .collect();
        let renderers: Vec<u32> = self.processes.iter().filter(|p| p.renderer).map(|p| p.pid).collect();
        let plan = efficiency::efficiency_plan(&renderers, &tab_pids, self.store.settings.efficiency_mode);
        for (pid, on) in &plan {
            if self.eco.get(pid) != Some(on) && efficiency::set_efficiency(*pid, *on) {
                self.eco.insert(*pid, *on);
            }
        }
        self.eco.retain(|pid, _| plan.contains_key(pid));
    }

    fn send_tasks(&self) {
        let tab_pids: HashMap<u32, Vec<u32>> = self.tab_processes().into_iter().collect();
        let mut memory: HashMap<u32, u64> = HashMap::new();
        for process in &self.processes {
            memory.insert(process.pid, efficiency::private_memory(process.pid).unwrap_or(0));
        }
        let own = efficiency::private_memory(std::process::id()).unwrap_or(0);
        let total: u64 = memory.values().sum::<u64>() + own;
        let mut in_tabs: Vec<u32> = Vec::new();
        let rows: Vec<Value> = self
            .tabs
            .iter()
            .map(|tab| {
                let pids = tab_pids.get(&tab.id).cloned().unwrap_or_default();
                let shared = pids.iter().any(|pid| {
                    tab_pids.iter().any(|(other, list)| *other != tab.id && list.contains(pid))
                });
                in_tabs.extend(&pids);
                let bytes: u64 = pids.iter().map(|pid| memory.get(pid).copied().unwrap_or(0)).sum();
                let state = if tab.id == self.active {
                    "active"
                } else if tab.webview.is_none() {
                    if tab.url.is_empty() { "start" } else { "unloaded" }
                } else if tab.sleeping {
                    "sleeping"
                } else {
                    "loaded"
                };
                json!({
                    "id": tab.id,
                    "title": tab.title,
                    "url": tab.url,
                    "favicon": self.tab_favicon(tab),
                    "state": state,
                    "memory": bytes,
                    "shared": shared,
                    "eco": pids.iter().any(|pid| self.eco.get(pid) == Some(&true)),
                })
            })
            .collect();
        in_tabs.sort_unstable();
        in_tabs.dedup();
        let tabs_total: u64 = in_tabs.iter().map(|pid| memory.get(pid).copied().unwrap_or(0)).sum();
        let info = json!({
            "rows": rows,
            "total": total,
            "other": total.saturating_sub(tabs_total),
            "onBattery": self.on_battery,
            "sleepLimit": self.sleep_limit(),
        });
        // память в списке — с точностью до мегабайта: мелкие колебания не перерисовывают его
        let mut rounded = info.clone();
        for row in rounded["rows"].as_array_mut().into_iter().flatten() {
            row["memory"] = json!(row["memory"].as_u64().unwrap_or(0) >> 20);
        }
        rounded["total"] = json!(info["total"].as_u64().unwrap_or(0) >> 20);
        rounded["other"] = json!(info["other"].as_u64().unwrap_or(0) >> 20);
        let key = rounded.to_string();
        if *self.tasks_sent.borrow() == key {
            return;
        }
        *self.tasks_sent.borrow_mut() = key;
        let _ = self.ui.evaluate_script(&format!("ui.showTasks({info})"));
    }

    /// Выгрузить вкладку вручную (из меню вкладки или диспетчера).
    fn unload_tab(&mut self, id: u32) {
        let downloading = self.downloads.iter().any(|d| d.state == "progress" || d.state == "paused");
        if id == self.active || downloading {
            return;
        }
        if let Some(tab) = self.tab_mut(id) {
            if tab.webview.is_some() {
                unload(tab);
                self.dirty = true;
            }
        }
    }

    /// Сведения для окошка по щелчку на замок: соединение, трекеры, cookie, масштаб.
    fn send_site_info(&self) {
        let Some(tab) = self.active_tab() else { return };
        let Some(webview) = &tab.webview else { return };
        let cookies = webview.cookies_for_url(&tab.url).map(|c| c.len()).unwrap_or(0);
        let info = json!({
            "host": adblock::host_of(&tab.url).unwrap_or(""),
            "secure": tab.url.starts_with("https://"),
            "blocked": tab.blocked,
            "cookies": cookies,
            "zoom": tab.zoom,
            "ubol": self.ubol_id.is_some(),
        });
        let _ = self.ui.evaluate_script(&format!("ui.showSiteInfo({info})"));
    }

    fn send_history(&self, query: &str) {
        let visits = self.store.search_history(query, 400);
        let json = serde_json::to_string(&visits).unwrap_or_else(|_| "[]".into());
        let _ = self.ui.evaluate_script(&format!("ui.setHistory({json})"));
    }

    // ---------- обновление интерфейса и фоновые задачи ----------

    /// Вызывается после каждой пачки событий: передаёт накопленные изменения,
    /// сохраняет данные и усыпляет давно скрытые вкладки.
    fn flush(&mut self) {
        if self.tasks_watch && self.last_tasks.elapsed() >= TASKS_EVERY && !self.window.is_minimized() {
            self.last_tasks = Instant::now();
            self.refresh_processes(true);
        }
        if self.dirty && self.last_push.elapsed() >= PUSH_INTERVAL {
            self.push_state();
        }
        if self.overlay_pending && self.overlay_requested.elapsed() > Duration::from_millis(250) {
            self.finish_overlay(None);
        }
        let since_save = self.last_save.elapsed();
        if self.session_dirty && since_save > Duration::from_secs(3) {
            self.save_session();
            self.last_save = Instant::now();
        }
        if self.history_dirty && since_save > Duration::from_secs(20) {
            self.store.save_history();
            self.store.save_settings(); // счётчик заблокированного
            self.history_dirty = false;
            self.last_save = Instant::now();
        }
        if self.last_sleep_check.elapsed() >= SLEEP_CHECK {
            self.last_sleep_check = Instant::now();
            self.sync_tick();
            // обновления — раз в час, первая проверка через 20 секунд после запуска
            if self.store.settings.auto_update && self.primary && self.last_update_check.elapsed() > Duration::from_secs(3600) {
                self.check_update(false);
            }
            let on_battery = efficiency::on_battery();
            if on_battery != self.on_battery {
                self.on_battery = on_battery;
                self.dirty = true;
            }
            self.sleep_background_tabs();
            self.refresh_processes(false);
        }
    }

    /// Фоновые вкладки сначала засыпают (WebView2 замораживает страницу), а через
    /// втрое больший срок выгружаются целиком: процесс страницы закрывается и
    /// память возвращается системе. Выгруженная вкладка загрузится заново при открытии.
    /// Через сколько минут усыплять: от батареи с экономией — не дольше 5 минут.
    fn sleep_limit(&self) -> u32 {
        let minutes = self.store.settings.sleep_minutes;
        if self.on_battery && self.store.settings.battery_saver {
            if minutes == 0 { 5 } else { minutes.min(5) }
        } else {
            minutes
        }
    }

    /// Окно свернули — то, что было на экране (вкладки, интерфейс, мессенджер в панели),
    /// переходит в режим низкой памяти, как советует Microsoft для неактивного приложения.
    /// Развернули — обратно в обычный. Скрипты при этом работают: музыка не прерывается.
    fn apply_minimized(&mut self) {
        let minimized = self.window.is_minimized();
        if minimized == self.minimized {
            return;
        }
        self.minimized = minimized;
        for id in self.visible_with(self.active) {
            if let Some(webview) = self.tab(id).filter(|t| !t.sleeping).and_then(|t| t.webview.as_ref()) {
                platform::set_background(webview, minimized);
            }
        }
        platform::set_background(&self.ui, minimized);
        if let Some(panel) = &self.panel {
            if !self.panel_sleeping {
                platform::set_background(panel, minimized);
            }
        }
        let app = self.store.settings.sidebar_app.as_str();
        for (name, webview) in &self.panel_apps {
            // мессенджер на экране панели — по окну; остальные и так в фоне
            if *name == app && self.panel_width() > 0.0 {
                platform::set_background(webview, minimized);
            }
        }
    }

    /// Как «Экономия памяти» в Chrome: когда оперативки в системе почти не осталось,
    /// не ждём таймера сна, а выгружаем фоновые вкладки — сначала чужих сайтов и давно скрытые.
    fn relieve_memory_pressure(&mut self) {
        let load = efficiency::memory_load();
        let active_host = self.tab(self.active).and_then(|t| adblock::host_of(&t.url)).map(str::to_owned);
        let candidates: Vec<PressureTab> = self
            .tabs
            .iter()
            .map(|t| PressureTab {
                id: t.id,
                hidden_for: t.hidden_since.map(|since| since.elapsed()),
                keep: t.audio || t.pinned || t.webview.is_none(),
                same_site: active_host.is_some() && adblock::host_of(&t.url) == active_host.as_deref(),
            })
            .collect();
        let victims = pressure_victims(&candidates, load);
        for id in &victims {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == *id) {
                unload(tab);
            }
        }
        if !victims.is_empty() {
            self.dirty = true;
        }
    }

    /// Закрытая боковая панель (заметки, калькулятор) засыпает сразу — WebView2 сам
    /// разбудит её, когда панель снова покажут. Мессенджеры не трогаем: в них уведомления.
    fn sleep_panel(&mut self) {
        let hidden = self.panel_width() == 0.0;
        match &self.panel {
            Some(panel) if hidden && !self.panel_sleeping => {
                platform::suspend(panel);
                self.panel_sleeping = true;
            }
            Some(_) if !hidden => self.panel_sleeping = false,
            _ => {}
        }
    }

    fn sleep_background_tabs(&mut self) {
        self.sleep_panel();
        let minutes = self.sleep_limit();
        // Выгрузка отменила бы загрузку, поэтому пока что-то качается — только сон.
        let downloading = self.downloads.iter().any(|d| d.state == "progress" || d.state == "paused");
        if minutes > 0 && !downloading {
            self.relieve_memory_pressure();
        }
        for tab in &mut self.tabs {
            let hidden_for = tab.hidden_since.map(|since| since.elapsed());
            let action = rest_action(hidden_for, minutes, tab.sleeping, tab.audio, tab.webview.is_some());
            match action {
                Rest::Suspend => {
                    if let Some(webview) = &tab.webview {
                        platform::suspend(webview);
                    }
                    tab.sleeping = true;
                }
                Rest::Discard if !downloading => unload(tab),
                _ => continue,
            }
            self.dirty = true;
        }
    }

    fn push_state(&mut self) {
        self.dirty = false;
        self.last_push = Instant::now();
        let tabs: Vec<Value> = self
            .tabs
            .iter()
            .map(|tab| {
                json!({
                    "id": tab.id,
                    "title": tab.title,
                    "url": tab.url,
                    "favicon": self.tab_favicon(tab),
                    "loading": tab.loading,
                    "blocked": tab.blocked,
                    "canBack": tab.can_back,
                    "canForward": tab.can_forward,
                    "start": tab.is_start(),
                    "pinned": tab.pinned,
                    "zoom": tab.zoom,
                    "audio": tab.audio,
                    "video": tab.video,
                    "shot": self.shot_versions.get(&tab.id).copied().unwrap_or(0),
                    "muted": tab.muted,
                    "sleeping": tab.sleeping || (tab.webview.is_none() && !tab.url.is_empty()),
                    "lang": tab.lang,
                    "group": tab.group,
                    "dark": adblock::host_of(&tab.url).is_some_and(|h| self.store.settings.dark_sites.iter().any(|d| d == h.trim_start_matches("www."))),
                })
            })
            .collect();
        let bookmarked = self.active_tab().is_some_and(|t| self.store.is_bookmarked(&t.url));
        let engines: Vec<Value> =
            ENGINES.iter().map(|(id, name, _)| json!({ "id": id, "name": name })).collect();
        let settings = &self.store.settings;
        let search = if ENGINES.iter().any(|(id, ..)| *id == settings.search) {
            settings.search.as_str()
        } else {
            ENGINES[0].0
        };
        let state = json!({
            "tabs": tabs,
            "active": self.active,
            "bookmarked": bookmarked,
            "bookmarks": self.store.bookmarks,
            "downloads": self.downloads,
            "adblock": settings.adblock,
            "blockedTotal": settings.blocked_total,
            "engines": engines,
            "search": search,
            "settings": {
                "theme": settings.theme,
                "restoreSession": settings.restore_session,
                "favbar": settings.favbar,
                "sleepMinutes": settings.sleep_minutes,
                "passwords": settings.passwords,
                "efficiencyMode": settings.efficiency_mode,
                "batterySaver": settings.battery_saver,
                "accent": settings.accent,
                "tintFrame": settings.tint_frame,
                "glass": settings.glass,
                "islandWidth": settings.island_width,
                "islandRadius": settings.island_radius,
                "islandTabs": settings.island_tabs,
                "darkAll": settings.dark_all,
                "leftClickBackground": settings.left_click_background,
                "autoUpdate": settings.auto_update,
                "processLimit": settings.process_limit,
                "compact": settings.compact,
                "urlInTab": settings.url_in_tab,
                "shortcuts": settings.shortcuts,
                "processLimitActive": PROCESS_LIMIT.load(Ordering::Relaxed),
                "updateSource": settings.update_source,
                "darkSites": settings.dark_sites,
                "siteBoost": settings.site_boost,
                "buttons": settings.buttons,
                "hiddenTools": settings.hidden_tools,
                "hiddenMenu": settings.hidden_menu,
                "splash": settings.splash,
                "motion": settings.motion,
                "barStyle": settings.bar_style,
                "newtabLayout": settings.newtab_layout,
                "tileSize": settings.tile_size,
                "wallpaperAlways": settings.wallpaper_always,
                "proxyMode": settings.proxy_mode,
                "proxyHost": settings.proxy_host,
                "proxyPort": settings.proxy_port,
                "downloadDir": settings.download_dir,
                "defaultDownloadDir": default_download_dir(),
                "wallpaper": settings.wallpaper,
                "wallpaperDim": settings.wallpaper_dim,
                "wallpaperUrl": format!("http://rb.localhost/wallpaper?v={}", settings.wallpaper_version),
                "hasImage": wallpaper_file().is_some(),
                "hasVideo": video_file().is_some(),
                "liveWalls": live_walls().1,
                "videoUrl": format!("http://rb.localhost/wallpaper-video?v={}", settings.wallpaper_version),
            },
            "ubolId": self.ubol_id,
            "isDefault": self.is_default,
            "panelOpen": self.panel_width() > 0.0,
            "siteOnly": self.site_only,
            "onBattery": self.on_battery,
            "importSources": self.import_sources,
            "logins": self.vault.logins.iter().map(|l| json!({
                "site": passwords::display_host(&l.origin),
                "username": l.username,
            })).collect::<Vec<_>>(),
            "pwNever": self.vault.never.iter().map(|o| json!([o, passwords::display_host(o)])).collect::<Vec<_>>(),
            "pwHere": self.passwords_here(),
            "pwHealth": self.pw_health,
            "pwChecking": self.pw_checking,
            "autofill": self.autofill.summary(),
            "afHere": self.active_tab().filter(|t| passwords::origin_of(&t.url).is_some()).map(|t| json!({ "card": t.af_card, "address": t.af_address })),
            "extensions": self.extensions_state(),
            "groups": self.groups,
            "split": self.split.map(|(l, r, ratio)| json!({ "left": l, "right": r, "ratio": ratio })),
            "account": self.account_state(),
            "install": json!({
                "installed": installer::is_installed(),
                "isInstalledCopy": installer::is_installed_copy(),
                "dir": installer::install_dir().display().to_string(),
                "exe": installer::current_exe().display().to_string(),
                "updateReady": self.update_ready,
                "version": env!("CARGO_PKG_VERSION"),
            }),
            "extBrowsers": self.ext_browsers,
            "dark": self.dark(),
            "maximized": self.maximized,
            "fullscreen": self.is_fullscreen(),
        });
        let mut state = state;
        if let Value::Object(fields) = &mut state {
            for key in ["bookmarks", "downloads", "engines", "settings", "logins", "pwNever", "extensions", "extBrowsers", "autofill"] {
                let Some(value) = fields.get(key) else { continue };
                let text = value.to_string();
                if self.sent.get(key) == Some(&text) {
                    fields.remove(key); // не менялось — интерфейс оставит прежнее
                } else {
                    self.sent.insert(key, text);
                }
            }
        }
        let _ = self.ui.evaluate_script(&format!("ui.setState({state})"));
    }
}

impl App {
    // ---------- обновления ----------

    /// Источник обновлений: выбранный exe или тот, из которого установили браузер.
    fn check_update(&mut self, manual: bool) {
        self.last_update_check = Instant::now();
        let source = PathBuf::from(&self.store.settings.update_source);
        if self.store.settings.update_source.is_empty() || !source.is_file() {
            if manual {
                let _ = self.ui.evaluate_script("ui.toast('Источник обновлений не выбран — укажите файл новой сборки')");
            }
            return;
        }
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let result = if installer::update_available(&source) {
                installer::apply_update(&source).map(|()| true)
            } else {
                Ok(false)
            };
            if manual && matches!(result, Ok(false)) {
                let _ = proxy.send_event(UserEvent::Ui(r#"{"cmd":"__toast_latest"}"#.into()));
            }
            let _ = proxy.send_event(UserEvent::Updated(result));
        });
    }

    // ---------- группы вкладок и разделённый экран ----------

    /// Какие вкладки видны, если активна эта: она сама или обе половины разделённого экрана.
    fn visible_with(&self, id: u32) -> Vec<u32> {
        match self.split {
            Some((l, r, _)) if id == l || id == r => vec![l, r],
            _ => vec![id],
        }
    }

    /// Прямоугольники половин: (левый, правый, левая вкладка, правая вкладка).
    fn split_rects(&self) -> Option<(Rect, Rect, u32, u32)> {
        let (left, right, ratio) = self.split?;
        if self.is_fullscreen() {
            return None;
        }
        let area = self.page_rect();
        let (pos, size) = (area.position.to_logical::<f64>(1.0), area.size.to_logical::<f64>(1.0));
        let gap = SPLIT_GAP;
        let left_w = ((size.width - gap) * ratio).round();
        let right_w = size.width - gap - left_w;
        Some((
            rect(pos.x, pos.y, left_w, size.height),
            rect(pos.x + left_w + gap, pos.y, right_w, size.height),
            left,
            right,
        ))
    }

    fn start_split(&mut self, left: u32, right: u32) {
        if self.tab(right).is_none() || left == right {
            return;
        }
        // стартовая страница занимает всё окно и закрыла бы вторую половину
        if [left, right].iter().any(|id| self.tab(*id).is_some_and(Tab::is_start)) {
            let _ = self.ui.evaluate_script("ui.toast('Разделить экран можно только для вкладок с сайтами')");
            return;
        }
        // правая вкладка могла быть выгружена — загрузим её, когда разделение уже на месте:
        // если сделать её активной, фокус и адресная строка уедут в правую половину
        let lazy = self.tab(right).filter(|t| t.webview.is_none() && !t.url.is_empty()).map(|t| t.url.clone());
        // прежняя пара разделённого экрана: кто в новую не попал, уходит в фон
        if let Some((l, r, _)) = self.split {
            for old in [l, r] {
                if old != left && old != right {
                    self.hide_tab(old);
                }
            }
        }
        self.split = Some((left, right, 0.5));
        self.activate(left);
        if let Some(url) = lazy {
            self.navigate(right, &url); // фокус остаётся у активной левой половины
        }
        if let Some(tab) = self.tab_mut(right) {
            tab.hidden_since = None;
            tab.sleeping = false;
            if let Some(webview) = &tab.webview {
                platform::set_background(webview, false);
            }
        }
        self.layout();
        self.dirty = true;
    }

    /// Вкладка ушла с экрана (но не через переключение): фоновый режим и отсчёт до сна.
    fn hide_tab(&mut self, id: u32) {
        if let Some(tab) = self.tab_mut(id) {
            tab.hidden_since = Some(Instant::now());
            if let Some(webview) = &tab.webview {
                platform::set_background(webview, true);
            }
        }
    }

    /// Разделитель между половинами: курсор-стрелки над ним, перетаскивание меняет доли.
    fn split_cursor(&mut self, x: f64, y: f64) {
        self.cursor = (x, y);
        let Some((left_rect, _, _, _)) = self.split_rects().filter(|_| self.visible_with(self.active).len() == 2) else {
            return;
        };
        let scale = self.window.scale_factor();
        let (lx, ly) = (x / scale, y / scale);
        let left = left_rect.position.to_logical::<f64>(1.0);
        let size = left_rect.size.to_logical::<f64>(1.0);
        if self.split_drag {
            let area = self.page_rect();
            let (ax, aw) = (area.position.to_logical::<f64>(1.0).x, area.size.to_logical::<f64>(1.0).width);
            let ratio = ((lx - ax - SPLIT_GAP / 2.0) / (aw - SPLIT_GAP)).clamp(0.2, 0.8);
            if let Some(split) = &mut self.split {
                split.2 = ratio;
            }
            self.layout();
            return;
        }
        let over = lx >= left.x + size.width - 2.0 && lx <= left.x + size.width + SPLIT_GAP + 2.0 && ly >= left.y;
        self.window.set_cursor_icon(if over { tao::window::CursorIcon::ColResize } else { tao::window::CursorIcon::Default });
    }

    fn split_press(&mut self, pressed: bool) {
        if !pressed {
            if self.split_drag {
                self.split_drag = false;
                self.dirty = true;
            }
            return;
        }
        let Some((left_rect, _, _, _)) = self.split_rects() else { return };
        let scale = self.window.scale_factor();
        let lx = self.cursor.0 / scale;
        let left = left_rect.position.to_logical::<f64>(1.0).x + left_rect.size.to_logical::<f64>(1.0).width;
        self.split_drag = lx >= left - 2.0 && lx <= left + SPLIT_GAP + 2.0;
    }

    /// Вкладка вступает в группу (или выходит из неё при 0) и встаёт рядом с остальными.
    fn set_group(&mut self, id: u32, group: u32) {
        let Some(index) = self.index_of(id) else { return };
        if group != 0 && !self.groups.iter().any(|g| g.id == group) {
            return;
        }
        self.tabs[index].group = group;
        if group != 0 && !self.tabs[index].pinned {
            // вкладки группы держатся вместе: ставим за последней вкладкой группы
            if let Some(last) = self.tabs.iter().rposition(|t| t.group == group && t.id != id) {
                let tab = self.tabs.remove(index);
                let at = if last >= index { last } else { last + 1 };
                self.tabs.insert(at.min(self.tabs.len()), tab);
            }
        }
        self.prune_groups();
        self.dirty = true;
        self.session_dirty = true;
    }

    /// Куда перейти, когда сворачивают группу с активной вкладкой.
    fn tab_outside_collapsed(&self, gid: u32) -> Option<u32> {
        let from = self.index_of(self.active)?;
        let collapsed: Vec<u32> = self.groups.iter().filter(|g| g.collapsed).map(|g| g.id).collect();
        let groups: Vec<u32> = self.tabs.iter().map(|t| t.group).collect();
        nearest_outside(&groups, &collapsed, from, gid).map(|i| self.tabs[i].id)
    }

    /// Группы без вкладок исчезают сами.
    fn prune_groups(&mut self) {
        let before = self.groups.len();
        let used: Vec<u32> = self.tabs.iter().map(|t| t.group).collect();
        // опустевшая группа запоминается: Ctrl+Shift+T вернёт вкладку вместе с ней
        let (kept, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.groups).into_iter().partition(|g| used.contains(&g.id));
        self.groups = kept;
        self.closed_groups.extend(gone);
        let excess = self.closed_groups.len().saturating_sub(20);
        self.closed_groups.drain(..excess);
        if self.groups.len() != before {
            self.dirty = true;
            self.session_dirty = true;
        }
    }

    // ---------- тёмные сайты, перевод ----------


    /// Перерегистрирует скрипт тёмной темы во всех вкладках и сразу применяет его.
    fn apply_dark_sites(&mut self) {
        let script = dark_sites_script(&self.store.settings);
        for tab in &self.tabs {
            if let Some(webview) = &tab.webview {
                platform::set_document_script(webview, &tab.dark_script, Some(&script));
                let _ = webview.evaluate_script(&script);
            }
        }
        self.dirty = true;
    }

    /// Язык страницы: если он не русский, на панели появится кнопка перевода.
    fn detect_language(&self, id: u32) {
        let Some(webview) = self.tab(id).and_then(|t| t.webview.as_ref()) else { return };
        let proxy = self.proxy.clone();
        let _ = webview.evaluate_script_with_callback(
            "(document.documentElement.lang || document.querySelector('meta[http-equiv=content-language]')?.content || '').slice(0, 12)",
            move |json| {
                let lang: String = serde_json::from_str(&json).unwrap_or_default();
                let _ = proxy.send_event(UserEvent::PageLang(id, lang.to_ascii_lowercase()));
            },
        );
    }

    // ---------- значки сайтов ----------

    /// Открытая страница сама говорит, где её значки: берём их, если для сайта ещё нет картинки.
    fn collect_site_icon(&self, id: u32) {
        if self.inprivate {
            return;
        }
        let Some(tab) = self.tab(id).filter(|t| t.url.starts_with("http") && !t.failed) else { return };
        let Some(host) = adblock::host_of(&tab.url).map(str::to_string) else { return };
        let dir = icons::cache_dir(&data_dir());
        if !icons::valid_host(&host) || icons::has(&dir, &host) {
            return;
        }
        let Some(webview) = tab.webview.as_ref() else { return };
        let page_url = tab.url.clone();
        let proxy = self.proxy.clone();
        let _ = webview.evaluate_script_with_callback(icons::PAGE_ICONS, move |json| {
            // ответ — JSON-строка с JSON внутри
            let inner: String = serde_json::from_str(&json).unwrap_or_default();
            let list: Vec<(String, String, String, String)> = serde_json::from_str(&inner).unwrap_or_default();
            if list.is_empty() {
                return;
            }
            let (dir, host, page_url, proxy) = (dir.clone(), host.clone(), page_url.clone(), proxy.clone());
            std::thread::spawn(move || {
                if icons::save_from_page(&dir, &host, &page_url, &list) {
                    let _ = proxy.send_event(UserEvent::IconSaved);
                }
            });
        });
    }

    // ---------- пароли ----------

    fn save_autofill(&mut self) {
        if self.autofill_locked {
            let _ = self.ui.evaluate_script("ui.toast('Файл адресов и карт не расшифровался — чтобы его не стереть, изменения не сохраняются')");
            return;
        }
        if let Some(stored) = self.autofill.store() {
            self.store.write_named("autofill.json", &stored);
        }
        self.dirty = true;
    }

    /// Origin страницы, которая сейчас в активной вкладке (HTTPS или localhost). Адрес —
    /// у самого WebView2: tab.url меняется уже в начале перехода, когда на экране ещё
    /// прежняя страница.
    fn page_origin(&self) -> Option<String> {
        let tab = self.active_tab()?;
        let current = tab.webview.as_ref()?.url().ok()?;
        let origin = passwords::origin_of(&current)?;
        (passwords::origin_of(&tab.url).as_deref() == Some(origin.as_str())).then_some(origin)
    }

    /// Подставляет адрес или карту в страницу на экране — только на HTTPS-сайт `origin`.
    fn fill_autofill(&mut self, kind: &str, index: usize, origin: &str) {
        let fields = match kind {
            "address" => self.autofill.addresses.get(index).map(autofill::address_fields),
            _ => self.autofill.cards.get(index).map(autofill::card_fields),
        };
        let Some(fields) = fields else { return };
        if kind == "card" {
            self.pw_verified = Some(Instant::now());
        }
        // пока шла проверка Windows Hello, вкладка могла смениться или уйти на другой сайт
        if self.page_origin().as_deref() != Some(origin) {
            return;
        }
        let Some(webview) = self.active_tab().and_then(|t| t.webview.as_ref()) else { return };
        let _ = webview.evaluate_script(&autofill::fill_script(origin, &fields));
        let _ = webview.focus();
    }

    fn save_vault(&mut self) {
        self.store.write_named("passwords.json", &self.vault);
        self.dirty = true;
    }

    /// Логины для страницы на экране: (номер в хранилище, логин). Только если на ней форма входа.
    fn passwords_here(&self) -> Vec<Value> {
        let Some(tab) = self.active_tab().filter(|t| t.pw_form) else { return Vec::new() };
        let Some(origin) = passwords::origin_of(&tab.url) else { return Vec::new() };
        self.vault
            .logins
            .iter()
            .enumerate()
            .filter(|(_, l)| l.origin == origin)
            .map(|(i, l)| json!([i, l.username]))
            .collect()
    }

    fn handle_page(&mut self, id: u32, url: &str, raw: &str) {
        let Ok(message) = serde_json::from_str::<Value>(raw) else { return };
        match message["cmd"].as_str().unwrap_or("") {
            "pw_form" => {
                // сообщение могло прийти от прошлой страницы этой вкладки
                let Some(tab) = self.tab_mut(id) else { return };
                if passwords::origin_of(&tab.url) == passwords::origin_of(url) {
                    tab.pw_form = true;
                    self.dirty = true;
                }
            }
            "open_bg" => {
                let link = message["url"].as_str().unwrap_or("");
                if (link.starts_with("https://") || link.starts_with("http://")) && link.len() < 8000 {
                    self.new_background_tab(link);
                }
            }
            "boost_drm" => {
                let _ = self.ui.evaluate_script("ui.toast('Это видео защищено от копирования — его звук браузер усилить не может')");
            }
            "translate_sel" => {
                // выделенный текст — переводим в фоне, ответ вернётся в эту вкладку
                let text: String = message["text"].as_str().unwrap_or("").chars().take(2000).collect();
                let seq = message["seq"].as_u64().unwrap_or(0);
                if text.trim().is_empty() {
                    return;
                }
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let _ = proxy.send_event(UserEvent::SelTranslated(id, seq, translate_text(&text)));
                });
            }
            "has_video" => {
                if let Some(tab) = self.tab_mut(id).filter(|t| !t.video) {
                    tab.video = true;
                    self.dirty = true;
                }
            }
            "scroll_dir" => {
                if id == self.active && self.chrome_float.is_some() {
                    let down = message["dir"] == "down";
                    let y = message["y"].as_f64().unwrap_or(0.0);
                    let _ = self.ui.evaluate_script(&format!("ui.pageScroll({down}, {y})"));
                }
            }
            "af_form" => {
                // поля карты и адреса — только на HTTPS (и localhost), как и пароли
                let Some(tab) = self.tab_mut(id) else { return };
                if passwords::origin_of(url).is_some() && passwords::origin_of(&tab.url) == passwords::origin_of(url) {
                    tab.af_card |= message["card"].as_bool().unwrap_or(false);
                    tab.af_address |= message["address"].as_bool().unwrap_or(false);
                    self.dirty = true;
                }
            }
            "ext_store_check" => {
                let installed = webstore::store_id(url)
                    .is_some_and(|ext| self.ext_list.iter().any(|(id, _, _)| *id == ext));
                if installed {
                    self.store_page_status(Some(id), "Установлено ✓");
                }
            }
            "ext_store_install" => {
                // кнопка есть только на страницах магазина: адрес проверяем по самому WebView2
                if url.starts_with("https://chromewebstore.google.com/") {
                    self.install_from_store(id, url);
                }
            }
            "pw_capture" => {
                // предложение показывается только для активной вкладки: фоновая не должна
                // молча подменить его своим логином, пока человек смотрит на чужой сайт
                if self.inprivate || !self.store.settings.passwords || id != self.active {
                    return;
                }
                let Some(origin) = passwords::origin_of(url) else { return };
                let username: String = message["username"].as_str().unwrap_or("").chars().take(200).collect();
                let password = message["password"].as_str().unwrap_or("");
                if password.is_empty() || password.len() > 1000 || self.vault.never.contains(&origin) {
                    return;
                }
                if self.vault.has_same(&origin, &username, password) {
                    return;
                }
                let update = self.vault.for_origin(&origin).any(|l| l.username == username);
                let site = passwords::display_host(&origin).to_string();
                self.pw_offer = Some(PasswordOffer {
                    tab: id,
                    origin,
                    username: username.clone(),
                    password: password.to_string(),
                });
                if id == self.active {
                    let offer = json!({ "site": site, "username": username, "update": update });
                    let _ = self.ui.evaluate_script(&format!("ui.passwordOffer({offer})"));
                }
            }
            _ => {}
        }
    }

    fn save_offered_password(&mut self) {
        let Some(offer) = self.pw_offer.take() else { return };
        if self.vault.upsert(&offer.origin, &offer.username, &offer.password, unix_now()) {
            self.save_vault();
            let _ = self.ui.evaluate_script("ui.toast('Пароль сохранён и зашифрован')");
        }
        // вкладка могла уже уйти на другой сайт — тогда форма входа там не наша
        if let Some(tab) = self.tab_mut(offer.tab) {
            if passwords::origin_of(&tab.url).as_deref() == Some(offer.origin.as_str()) {
                tab.pw_form = true;
            }
        }
    }

    /// Подставляет пароль только по щелчку в интерфейсе и только на тот же сайт.
    fn fill_password(&mut self, index: usize) {
        let Some(login) = self.vault.logins.get(index) else { return };
        // сверяем и с адресом вкладки, и с тем, что на самом деле открыто в WebView2
        if self.page_origin().as_deref() != Some(login.origin.as_str()) {
            return;
        }
        let Some(webview) = self.active_tab().and_then(|t| t.webview.as_ref()) else { return };
        let Some(password) = passwords::unprotect(&login.secret) else {
            let _ = self.ui.evaluate_script("ui.toast('Не удалось расшифровать пароль')");
            return;
        };
        let _ = webview.evaluate_script(&passwords::fill_script(&login.origin, &login.username, &password));
        let _ = webview.focus();
        self.vault.logins[index].used = unix_now();
        self.store.write_named("passwords.json", &self.vault);
    }

    /// Показать или скопировать пароль можно только после Windows Hello.
    fn reveal_password(&mut self, index: usize, copy: bool) {
        if index >= self.vault.logins.len() {
            return;
        }
        if self.pw_verified.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
            self.password_verified(index, copy, passwords::Verdict::Verified);
            return;
        }
        let proxy = self.proxy.clone();
        passwords::verify(
            self.window.hwnd(),
            "Rust Browser: подтвердите, что это вы, чтобы увидеть пароль",
            move |verdict| {
                let _ = proxy.send_event(UserEvent::Verified(index, copy, verdict));
            },
        );
    }

    fn password_verified(&mut self, index: usize, copy: bool, verdict: passwords::Verdict) {
        match verdict {
            passwords::Verdict::Verified => {
                self.pw_verified = Some(Instant::now());
                let Some(password) = self.vault.logins.get(index).and_then(|l| passwords::unprotect(&l.secret))
                else {
                    return;
                };
                let reply = json!({ "index": index, "password": password, "copy": copy });
                let _ = self.ui.evaluate_script(&format!("ui.passwordRevealed({reply})"));
            }
            passwords::Verdict::Canceled => {}
            passwords::Verdict::Unavailable => {
                let _ = self.ui.evaluate_script(
                    "ui.toast('Включите ПИН-код Windows Hello в Параметрах Windows — без него пароли не показываются')",
                );
            }
        }
    }

    fn import_passwords(&mut self) {
        let Some(path) = platform::pick_file(self.window.hwnd(), "Пароли (CSV)", "*.csv") else { return };
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let now = unix_now();
        let mut added = 0;
        for (url, username, password) in passwords::parse_csv(&text) {
            let Some(origin) = passwords::origin_of(&url) else { continue };
            if !self.vault.has_same(&origin, &username, &password) && self.vault.upsert(&origin, &username, &password, now) {
                added += 1;
            }
        }
        self.save_vault();
        let _ = self.ui.evaluate_script(&format!("ui.passwordsImported({added})"));
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl App {
    // ---------- расширения ----------

    fn extensions_dir() -> PathBuf {
        data_dir().join("extensions").join("installed")
    }

    fn save_extensions(&mut self) {
        self.store.write_named("extensions.json", &self.extensions);
        let ubol = self.ubol_id.as_deref().zip(self.ubol_folder.as_deref());
        register_icons(&self.extensions, ubol);
        self.dirty = true;
    }

    /// Спрашивает у профиля, какие расширения стоят и включены.
    fn refresh_extensions(&self) {
        let proxy = self.proxy.clone();
        platform::list_extensions(&self.ui, move |list| {
            let _ = proxy.send_event(UserEvent::ExtList(list));
        });
    }

    /// Расширения для интерфейса: что стоит в профиле, плюс сведения из manifest.json.
    fn extensions_state(&self) -> Vec<Value> {
        self.ext_list
            .iter()
            // служебные компоненты WebView2 (просмотр PDF, буфер обмена) не показываем
            .filter(|(id, _, _)| self.ubol_id.as_deref() == Some(id.as_str()) || self.extensions.iter().any(|e| &e.id == id))
            .map(|(id, name, enabled)| {
                let ubol = self.ubol_id.as_deref() == Some(id.as_str());
                let info = self.extensions.iter().find(|e| &e.id == id).cloned().or_else(|| {
                    ubol.then(|| self.ubol_folder.as_deref().and_then(webstore::describe)).flatten()
                });
                let info = info.unwrap_or_default();
                json!({
                    "id": id,
                    "name": if info.name.is_empty() { name.clone() } else { info.name.clone() },
                    "enabled": enabled,
                    "version": info.version,
                    "source": if ubol { "встроено".to_string() } else { info.source.clone() },
                    "popup": !info.popup.is_empty(),
                    "options": !info.options.is_empty(),
                    "icon": !info.icon.is_empty(),
                    "builtin": ubol,
                })
            })
            .collect()
    }

    fn extension_info(&self, id: &str) -> Option<webstore::Installed> {
        if self.ubol_id.as_deref() == Some(id) {
            return self.ubol_folder.as_deref().and_then(webstore::describe);
        }
        self.extensions.iter().find(|e| e.id == id).cloned()
    }

    /// Кнопка на странице магазина: id берём из адреса вкладки, который знает браузер, а не страница.
    fn install_from_store(&mut self, tab: u32, page_url: &str) {
        let tab_url = self.tab(tab).map(|t| t.url.clone()).unwrap_or_default();
        let Some(id) = webstore::store_id(&tab_url).or_else(|| webstore::store_id(page_url)) else { return };
        if self.ext_busy.contains(&id) {
            return;
        }
        self.ext_busy.insert(id.clone());
        let _ = self.ui.evaluate_script("ui.toast('Скачиваю расширение из Chrome Web Store…')");
        let proxy = self.proxy.clone();
        let engine = wry::webview_version().unwrap_or_default();
        std::thread::spawn(move || {
            let result = webstore::download(&id, &Self::extensions_dir(), &engine).map_err(|e| (id, e));
            let _ = proxy.send_event(UserEvent::ExtReady(result, Some(tab)));
        });
    }

    /// Перенос из другого браузера: копируем в фоне, ставим по одному.
    fn import_extensions(&mut self, browser: &str) {
        let Some((name, profiles)) = webstore::browser_profiles().into_iter().find(|(n, _)| *n == browser) else { return };
        let installed: Vec<String> = self.ext_list.iter().map(|(id, _, _)| id.clone()).collect();
        let found: Vec<_> = webstore::scan(&profiles).into_iter().filter(|f| !installed.contains(&f.id)).collect();
        if found.is_empty() {
            let _ = self.ui.evaluate_script(&format!("ui.toast({:?})", format!("Из {name} переносить нечего — всё уже есть")));
            return;
        }
        let _ = self.ui.evaluate_script(&format!("ui.toast({:?})", format!("Переношу расширения из {name}: {}", found.len())));
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            for item in found {
                let result = webstore::copy_from(&item, &Self::extensions_dir(), name).map_err(|e| (item.id.clone(), e));
                let _ = proxy.send_event(UserEvent::ExtReady(result, None));
            }
        });
    }

    /// Папка с расширением готова — ставим в профиль (по очереди, чтобы не мешать друг другу).
    fn extension_ready(&mut self, result: Result<webstore::Installed, (String, String)>, tab: Option<u32>) {
        match result {
            Ok(info) => {
                self.ext_queue.push_back((info, tab));
                self.install_next_extension();
            }
            Err((id, error)) => {
                self.ext_busy.remove(&id);
                if self.ext_quiet.remove(&id) {
                    return;
                }
                self.store_page_status(tab, "Не получилось");
                let _ = self.ui.evaluate_script(&format!("ui.toast({:?})", format!("Расширение не установилось: {error}")));
            }
        }
    }

    fn install_next_extension(&mut self) {
        if self.ext_installing {
            return;
        }
        let Some((info, tab)) = self.ext_queue.pop_front() else { return };
        self.ext_installing = true;
        let proxy = self.proxy.clone();
        let pending = info.clone();
        platform::add_extension(&self.ui, &info.folder, &info.id, move |result| {
            let _ = proxy.send_event(UserEvent::ExtAdded(pending, result, tab));
        });
    }

    fn extension_added(&mut self, mut info: webstore::Installed, result: Result<String, String>, tab: Option<u32>) {
        self.ext_installing = false;
        self.ext_busy.remove(&info.id);
        match result {
            Ok(id) => {
                info.id = id;
                self.extensions.retain(|e| e.id != info.id);
                let name = info.name.clone();
                self.extensions.push(info);
                self.save_extensions();
                self.refresh_extensions();
                self.store_page_status(tab, "Установлено ✓");
                let _ = self.ui.evaluate_script(&format!("ui.extensionInstalled({:?})", name));
            }
            Err(error) => {
                let _ = std::fs::remove_dir_all(&info.folder);
                if self.ext_quiet.remove(&info.id) {
                    self.install_next_extension();
                    return;
                }
                self.store_page_status(tab, "Не поддерживается");
                let reason = if info.mv < 3 {
                    "старый формат (Manifest V2) больше не работает в движке Chromium".to_string()
                } else {
                    error
                };
                let _ = self.ui.evaluate_script(&format!(
                    "ui.toast({:?})",
                    format!("«{}» не установилось: {reason}", info.name)
                ));
            }
        }
        self.install_next_extension();
    }

    fn store_page_status(&self, tab: Option<u32>, text: &str) {
        let webview = tab.and_then(|id| self.tab(id)).and_then(|t| t.webview.as_ref());
        if let Some(webview) = webview {
            let _ = webview.evaluate_script(&format!("window.__rbStoreDone && __rbStoreDone({text:?})"));
        }
    }

    fn remove_extension(&mut self, id: &str) {
        if self.ubol_id.as_deref() == Some(id) {
            return; // встроенный блокировщик выключается переключателем рекламы
        }
        self.close_ext_popup();
        let folder = self.extensions.iter().find(|e| e.id == id).map(|e| e.folder.clone());
        let proxy = self.proxy.clone();
        platform::remove_extension(&self.ui, id, move || {
            if let Some(folder) = folder.filter(|f| f.starts_with(Self::extensions_dir())) {
                let _ = std::fs::remove_dir_all(folder);
            }
            let _ = proxy.send_event(UserEvent::ExtChanged);
        });
        self.extensions.retain(|e| e.id != id);
        self.save_extensions();
    }

    // ----- окошко расширения (его кнопка на панели) -----

    fn open_ext_popup(&mut self, id: &str, right: f64, top: f64) {
        self.close_ext_popup();
        let Some(info) = self.extension_info(id) else { return };
        if info.popup.is_empty() {
            if !info.options.is_empty() {
                self.new_tab(Some(format!("chrome-extension://{id}/{}", info.options)));
            }
            let _ = self.ui.evaluate_script("ui.closeOverlays()");
            return;
        }
        let url = format!("chrome-extension://{id}/{}", info.popup);
        let (width, height) = (360.0, 120.0);
        let bounds = rect((right - width).max(8.0), top, width, height);
        let webview = match self.ext_popup_spare.take() {
            Some(webview) => {
                let _ = webview.set_bounds(bounds);
                let _ = webview.load_url(&url);
                platform::set_background(&webview, false);
                webview
            }
            None => {
                let network = proxy_config(&self.store.settings);
                let popup = self.proxy.clone();
                let (dark, inprivate, theme) = (self.dark(), self.inprivate, self.page_theme());
                let built = WebViewBuilder::new_with_web_context(&mut self.web_context)
                    .with_bounds(bounds)
                    .with_url(url)
                    .with_visible(false)
                    .with_background_color(if dark { (32, 33, 40, 255) } else { (255, 255, 255, 255) })
                    .with_incognito(inprivate)
                    .with_browser_extensions_enabled(true)
                    .with_network(network)
                    .with_theme(theme)
                    .with_new_window_req_handler(move |url, _| {
                        let _ = popup.send_event(UserEvent::NewTabRequested(url, false));
                        NewWindowResponse::Deny
                    })
                    .build_as_child(&self.window);
                let Ok(webview) = built else { return };
                platform::hook_hotkeys(&webview, self.keys.clone(), self.proxy.clone());
                webview
            }
        };
        // показывается после первого замера — уже нужного размера, без прыжков
        let _ = webview.set_visible(false);
        self.ext_popup = Some((webview, right, top));
        // окошко само подстраивается под содержимое, как в Chrome: меряем несколько раз, пока оно рисуется
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            let _ = proxy.send_event(UserEvent::ExtPopupShow);
        });
        for delay in [40u64, 120, 300, 700, 1500] {
            let proxy = self.proxy.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(delay));
                let _ = proxy.send_event(UserEvent::ExtPopupMeasure);
            });
        }
    }

    fn measure_ext_popup(&self) {
        let Some((webview, _, _)) = &self.ext_popup else { return };
        let proxy = self.proxy.clone();
        platform::content_size(webview, move |width, height| {
            let _ = proxy.send_event(UserEvent::ExtPopupSize(width, height));
        });
    }

    fn resize_ext_popup(&mut self, width: f64, height: f64) {
        let Some((webview, right, top)) = &self.ext_popup else { return };
        let (window_width, window_height) = window_size(&self.window);
        let width = width.clamp(240.0, 800.0).min(window_width - 16.0);
        let height = height.clamp(80.0, 600.0).min(window_height - top - 8.0);
        let left = (right - width).max(8.0);
        let _ = webview.set_bounds(rect(left, *top, width, height));
        let _ = webview.set_visible(true);
        let _ = webview.focus();
        let box_rect = json!({ "left": left, "top": top, "width": width, "height": height });
        let _ = self.ui.evaluate_script(&format!("ui.extPopupBox({box_rect})"));
    }

    fn close_ext_popup(&mut self) {
        if let Some((webview, _, _)) = self.ext_popup.take() {
            let _ = webview.set_visible(false);
            let _ = webview.load_url("about:blank"); // скрипты расширения останавливаются
            platform::set_background(&webview, true);
            self.ext_popup_spare = Some(webview);
        }
    }
}

/// Значки расширений для интерфейса: http://rb.localhost/exticon/<id>.
static EXT_ICONS: std::sync::Mutex<Vec<(String, PathBuf)>> = std::sync::Mutex::new(Vec::new());

fn register_icons(list: &[webstore::Installed], ubol: Option<(&str, &Path)>) {
    let mut icons: Vec<(String, PathBuf)> = list
        .iter()
        .filter(|e| safe_relative(&e.icon))
        .map(|e| (e.id.clone(), e.folder.join(&e.icon)))
        .collect();
    if let Some((id, info)) = ubol.and_then(|(id, f)| webstore::describe(f).map(|i| (id, i))).filter(|(_, i)| safe_relative(&i.icon)) {
        icons.push((id.to_string(), info.folder.join(&info.icon)));
    }
    if let Ok(mut guard) = EXT_ICONS.lock() {
        *guard = icons;
    }
}

/// Путь из manifest.json — только внутри папки расширения: без «..», без диска и корня.
fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path).components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir))
}

fn extension_icon(id: &str) -> Option<PathBuf> {
    let guard = EXT_ICONS.lock().ok()?;
    guard.iter().find(|(k, _)| k == id).map(|(_, p)| p.clone())
}

/// Усилитель звука: громкость видео и музыки на странице до 600 %, через Web Audio
/// (усиление + ограничитель, чтобы не хрипело). Работает во всех рамках страницы:
/// верхняя пересылает уровень встроенным плеерам. Звук элемента не трогаем, пока
/// громкость 100 %, и не трогаем вовсе, если Web Audio его не получит (чужой сайт
/// без CORS — иначе была бы тишина).
const BOOST_SCRIPT: &str = r#"(() => {
  if (window.__rbBoostInit) return;
  window.__rbBoostInit = true;
  const top = window.top === window;
  let level = 1, ctx = null, explicit = false;
  // уровень сайта: список приходит отдельным скриптом, он выполняется позже этого
  const saved = () => {
    const v = (window.__rbBoostSites || {})[location.hostname.replace(/^www\./, '')];
    return v ? v / 100 : 1;
  };
  const settle = () => { if (top && !explicit) level = saved(); };
  const hooked = new WeakMap();
  const usable = (m) => {
    const src = m.currentSrc || m.src || '';
    if (!src || src.startsWith('blob:') || src.startsWith('data:') || m.crossOrigin) return true;
    try { return new URL(src, location.href).origin === location.origin; } catch (e) { return false; }
  };
  // защищённое видео (DRM, фильмы по подписке): движок может отдавать его звук мимо Web Audio —
  // перехват дал бы тишину до перезагрузки страницы, поэтому такие видео не трогаем
  let drmTold = false;
  const drm = (m) => {
    if (!m.mediaKeys) return false;
    if (level !== 1 && !drmTold) {
      drmTold = true;
      try { top ? window.chrome.webview.postMessage(JSON.stringify({ cmd: 'boost_drm' })) : window.top.postMessage({ __rbBoostDrm: 1 }, '*'); } catch (e) {}
    }
    return true;
  };
  const hook = (m) => {
    if (hooked.has(m)) return hooked.get(m);
    if (level === 1 || !usable(m) || drm(m)) return null;
    try {
      ctx = ctx || new AudioContext({ latencyHint: 'playback' });
      const source = ctx.createMediaElementSource(m);
      const gain = ctx.createGain();
      const limiter = ctx.createDynamicsCompressor();
      limiter.threshold.value = -1.5; limiter.knee.value = 2; limiter.ratio.value = 20;
      limiter.attack.value = 0.003; limiter.release.value = 0.2;
      const meter = ctx.createAnalyser();
      meter.fftSize = 2048;
      source.connect(gain).connect(limiter).connect(meter).connect(ctx.destination);
      const node = { gain, meter };
      hooked.set(m, node);
      return node;
    } catch (e) { return null; }
  };
  const media = () => [...document.querySelectorAll('video, audio')];
  const apply = () => {
    for (const m of media()) {
      const node = hook(m);
      if (node) node.gain.gain.setTargetAtTime(level, ctx.currentTime, 0.04);
    }
    if (ctx && ctx.state === 'suspended') ctx.resume().catch(() => {});
  };
  const forward = () => {
    for (const f of document.querySelectorAll('iframe')) {
      try { f.contentWindow.postMessage({ __rbBoost: level }, '*'); } catch (e) {}
    }
  };
  window.__rbBoost = (v) => {
    level = Math.max(0, Math.min(6, Number.isFinite(Number(v)) ? Number(v) : 1));
    explicit = true;
    apply();
    forward();
  };
  // для проверки: насколько громко сейчас звучит (среднеквадратичное, 0…1)
  window.__rbBoostMeter = () => {
    let best = 0;
    for (const m of media()) {
      const node = hooked.get(m);
      if (!node) continue;
      const data = new Float32Array(node.meter.fftSize);
      node.meter.getFloatTimeDomainData(data);
      best = Math.max(best, Math.sqrt(data.reduce((s, x) => s + x * x, 0) / data.length));
    }
    return { level, hooked: media().filter((m) => hooked.has(m)).length, media: media().length, rms: best, state: ctx && ctx.state };
  };
  addEventListener('message', (e) => {
    const d = e.data;
    if (!d || typeof d !== 'object') return;
    if (typeof d.__rbBoost === 'number' && e.source === parent && !top) window.__rbBoost(d.__rbBoost);
    // встроенный плеер родился позже — спрашивает уровень у родителя
    if (d.__rbBoostDrm && top) { try { window.chrome.webview.postMessage(JSON.stringify({ cmd: 'boost_drm' })); } catch (err) {} }
    if (d.__rbBoostAsk && e.source) { settle(); try { e.source.postMessage({ __rbBoost: level }, '*'); } catch (err) {} }
  });
  const onMedia = (e) => { if (!(e.target instanceof HTMLMediaElement)) return; settle(); if (level !== 1) apply(); };
  addEventListener('play', onMedia, true);
  addEventListener('loadedmetadata', onMedia, true);
  if (top) {
    addEventListener('DOMContentLoaded', () => { settle(); forward(); });
    addEventListener('load', forward);
  } else {
    try { parent.postMessage({ __rbBoostAsk: 1 }, '*'); } catch (e) {}
  }
})();"#;

/// Уровни усиления по сайтам — для скрипта выше (выполняется раньше него в каждом документе).
fn boost_sites_script(settings: &store::Settings) -> String {
    let sites = serde_json::to_string(&settings.site_boost).unwrap_or_else(|_| "{}".into());
    format!("window.__rbBoostSites = {sites};")
}

/// Предзагрузка ссылок: при наведении на ссылку своего сайта Chromium начинает грузить
/// её заранее (правила speculation rules, «moderate» — после короткой задержки наведения).
const PREFETCH_SCRIPT: &str = r#"(() => {
  if (window.top !== window || !HTMLScriptElement.supports || !HTMLScriptElement.supports('speculationrules')) return;
  const add = () => {
    if (!document.head || document.getElementById('__rb_prefetch')) return;
    const s = document.createElement('script');
    s.type = 'speculationrules';
    s.id = '__rb_prefetch';
    s.textContent = JSON.stringify({ prefetch: [{ source: 'document', eagerness: 'moderate',
      where: { and: [{ href_matches: '/*' }, { not: { selector_matches: '[rel~=nofollow], [href*=logout], [href*=signout], [href*=exit]' } }] } }] });
    try { document.head.append(s); } catch (e) {}
  };
  // сайты с Trusted Types (YouTube) не дают вписать текст в script — там просто без предзагрузки
  const safe = () => { try { add(); } catch (e) {} };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', safe); else safe();
})();"#;

/// «Картинка в картинке»: самое крупное видео на странице; повторно — вернуть обратно.
const PIP_SCRIPT: &str = r#"(async () => {
  if (document.pictureInPictureElement) return document.exitPictureInPicture();
  const all = [...document.querySelectorAll('video')].filter((v) => v.readyState > 0 && !v.disablePictureInPicture);
  all.sort((a, b) => (!b.paused - !a.paused) || (b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight));
  if (all[0]) await all[0].requestPictureInPicture();
})()"#;

/// Тёмная тема для сайтов: инверсия цветов страницы с возвратом картинок и видео.
/// Срабатывает для сайтов из списка или для всех; уже тёмные сайты при «для всех» не трогаем.
fn dark_sites_script(settings: &store::Settings) -> String {
    let sites = serde_json::to_string(&settings.dark_sites).unwrap_or_else(|_| "[]".into());
    format!(
        r#"(() => {{
  const SITES = {sites}, ALL = {all};
  const host = location.hostname.replace(/^www\./, '');
  const on = SITES.includes(host) || (ALL && /^https?:/.test(location.protocol));
  const apply = () => {{
    let style = document.getElementById('__rb_dark');
    if (!on) {{ style?.remove(); return true; }}
    if (!document.documentElement) return false;
    if (!style) {{
      style = document.createElement('style');
      style.id = '__rb_dark';
      style.textContent = 'html{{filter:invert(.92) hue-rotate(180deg)!important;background:#fff!important}}' +
        'img,video,picture,canvas,iframe,embed,object,svg image,[style*="background-image"]{{filter:invert(1) hue-rotate(180deg)!important}}';
      document.documentElement.append(style);
    }}
    return true;
  }};
  if (!apply()) new MutationObserver((_, o) => {{ if (apply()) o.disconnect(); }}).observe(document, {{ childList: true }});
  if (!on) return;
  // У сайта уже тёмный фон (YouTube, GitHub в тёмной теме…) — инверсия сделала бы его
  // светлым, поэтому свою тему снимаем. Фон бывает у body или только у html; сайт может
  // включить свою тёмную тему уже после загрузки — тогда проверяем заново.
  const dark = () => {{
    for (const node of [document.body, document.documentElement]) {{
      if (!node) continue;
      const m = getComputedStyle(node).backgroundColor.match(/[\d.]+/g);
      if (!m) continue;
      const [r, g, b, a = 1] = m.map(Number);
      if (a < .1) continue; // прозрачный — смотрим дальше
      return r * .3 + g * .59 + b * .11 < 110;
    }}
    return false;
  }};
  let queued = false;
  const check = () => {{
    queued = false;
    const style = document.getElementById('__rb_dark');
    if (!style) return;
    style.disabled = true; // смотрим на родные цвета сайта
    style.disabled = dark();
  }};
  const later = () => {{ if (!queued) {{ queued = true; requestAnimationFrame(check); }} }};
  addEventListener('DOMContentLoaded', later);
  addEventListener('load', later);
  setTimeout(later, 1500);
  const watch = new MutationObserver(later);
  const observe = () => {{
    watch.observe(document.documentElement, {{ attributes: true, attributeFilter: ['class', 'style', 'dark', 'data-theme', 'data-color-mode'] }});
    if (document.body) watch.observe(document.body, {{ attributes: true, attributeFilter: ['class', 'style', 'data-theme'] }});
  }};
  if (document.body) observe(); else addEventListener('DOMContentLoaded', observe);
}})();"#,
        all = settings.dark_all
    )
}

/// Адрес страницы через переводчик Google (https://сайт-ru.translate.goog/…): страница
/// остаётся живой — ссылки, кнопки и прокрутка работают, а текст уже по-русски.
fn translate_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap_or((rest, "/".into()));
    if host.ends_with(".translate.goog") || host.contains(':') {
        return None;
    }
    let encoded = host.replace('-', "--").replace('.', "-");
    let (path, fragment) = path.split_once('#').map(|(p, f)| (p.to_string(), format!("#{f}"))).unwrap_or((path, String::new()));
    let joiner = if path.contains('?') { '&' } else { '?' };
    Some(format!("https://{encoded}.translate.goog{path}{joiner}_x_tr_sl=auto&_x_tr_tl=ru&_x_tr_hl=ru{fragment}"))
}

/// Обратно к оригиналу с переведённой страницы.
fn untranslate_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let encoded = host.strip_suffix(".translate.goog")?;
    // «--» — дефис, «-» — точка
    let original = encoded.replace("--", "\u{0}").replace('-', ".").replace('\u{0}', "-");
    let (path, fragment) = path.split_once('#').map(|(p, f)| (p, format!("#{f}"))).unwrap_or((path, String::new()));
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let query: Vec<&str> = query.split('&').filter(|p| !p.is_empty() && !p.starts_with("_x_tr_")).collect();
    let query = if query.is_empty() { String::new() } else { format!("?{}", query.join("&")) };
    Some(format!("https://{original}/{path}{query}{fragment}"))
}

/// Сообщает браузеру, куда листают страницу: «Остров» прячется при прокрутке вниз.
/// Ловим прокрутку и самой страницы, и вложенных блоков (у многих сайтов листается div).
/// Перевод выделенного текста: рядом с выделением — кнопка «Перевести», по нажатию —
/// карточка с переводом. Всё в теневом DOM: стили сайта её не ломают.
const SELECTION_SCRIPT: &str = r#"(() => {
  if (window.top !== window || window.__rbSel) return;
  window.__rbSel = true;
  let host, root, button, card, seq = 0, lastText = '', anchor = null;
  const LANGS = { en: 'Английский', ru: 'Русский', de: 'Немецкий', fr: 'Французский', es: 'Испанский', it: 'Итальянский',
    uk: 'Украинский', pl: 'Польский', tr: 'Турецкий', pt: 'Португальский', zh: 'Китайский', 'zh-CN': 'Китайский',
    ja: 'Японский', ko: 'Корейский', ar: 'Арабский', nl: 'Нидерландский', kk: 'Казахский', be: 'Белорусский' };
  const lang = (c) => LANGS[c] || c;
  const ensure = () => {
    if (host) return;
    host = document.createElement('rb-translate');
    host.style.cssText = 'all:initial;position:fixed;inset:0;pointer-events:none;z-index:2147483647';
    root = host.attachShadow({ mode: 'closed' });
    root.innerHTML = `<style>
      .b, .c { position: fixed; pointer-events: auto; font: 13px/1.45 "Segoe UI Variable Text", "Segoe UI", sans-serif;
        --bg: #fff; --fg: #1b1c22; --mu: #697084; --bd: rgba(20,24,48,.12); --ac: #4f5bf5; }
      @media (prefers-color-scheme: dark) { .b, .c { --bg: #23242c; --fg: #eceef5; --mu: #9a9fb1; --bd: rgba(255,255,255,.12); --ac: #8288ff; } }
      .b { display: none; align-items: center; gap: 6px; height: 30px; padding: 0 12px 0 10px; border-radius: 15px; border: 1px solid var(--bd);
        background: var(--bg); color: var(--fg); box-shadow: 0 6px 20px rgba(0,0,0,.18); cursor: pointer; animation: in .14s ease-out; }
      .b:hover { color: var(--ac); }
      .b svg { width: 15px; height: 15px; }
      .c { display: none; width: min(380px, calc(100vw - 24px)); max-height: min(360px, 60vh); overflow: auto; padding: 12px 14px 10px;
        border-radius: 14px; border: 1px solid var(--bd); background: var(--bg); color: var(--fg);
        box-shadow: 0 14px 40px rgba(0,0,0,.25); animation: in .16s ease-out; }
      .h { display: flex; align-items: center; justify-content: space-between; color: var(--mu); font-size: 12px; margin-bottom: 6px; }
      .t { font-size: 14px; white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
      .f { display: flex; gap: 6px; justify-content: flex-end; margin-top: 10px; }
      button { font: inherit; border: 0; background: none; color: var(--ac); cursor: pointer; padding: 4px 8px; border-radius: 8px; }
      button:hover { background: rgba(127,127,127,.12); }
      .x { color: var(--mu); font-size: 16px; line-height: 1; padding: 2px 6px; }
      @keyframes in { from { opacity: 0; transform: translateY(4px); } }
    </style>
    <div class="b"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 5h9M8.5 3v2M11 5c-.8 4-3.5 7-7 9"/><path d="M6.5 9c1 2 3 4 5.5 5"/><path d="m13 21 4-9 4 9M14.4 18h5.2"/></svg>Перевести</div>
    <div class="c"><div class="h"><span class="l">Перевожу…</span><button class="x" title="Закрыть">×</button></div><div class="t"></div>
      <div class="f"><button class="cp">Копировать</button></div></div>`;
    button = root.querySelector('.b');
    card = root.querySelector('.c');
    (document.body || document.documentElement).append(host);
    button.addEventListener('mousedown', (e) => { e.preventDefault(); e.stopPropagation(); });
    button.addEventListener('click', (e) => { e.stopPropagation(); translate(); });
    card.addEventListener('mousedown', (e) => e.stopPropagation());
    root.querySelector('.x').addEventListener('click', hideAll);
    root.querySelector('.cp').addEventListener('click', () => {
      const text = root.querySelector('.t').textContent;
      if (navigator.clipboard) navigator.clipboard.writeText(text).then(() => { root.querySelector('.cp').textContent = 'Скопировано'; });
    });
  };
  const hideAll = () => { if (!host) return; button.style.display = 'none'; card.style.display = 'none'; };
  const place = (el, rect, below) => {
    const w = el.offsetWidth || 120, h = el.offsetHeight || 30;
    const x = Math.min(Math.max(8, rect.left + rect.width / 2 - w / 2), innerWidth - w - 8);
    let y = below ? rect.bottom + 8 : rect.top - h - 8;
    if (y < 8) y = rect.bottom + 8;
    if (y + h > innerHeight - 8) y = Math.max(8, rect.top - h - 8);
    el.style.left = x + 'px';
    el.style.top = y + 'px';
  };
  const translate = () => {
    if (!lastText) return;
    seq++;
    button.style.display = 'none';
    root.querySelector('.l').textContent = 'Перевожу…';
    root.querySelector('.t').textContent = '';
    root.querySelector('.cp').textContent = 'Копировать';
    card.style.display = 'block';
    place(card, anchor, true);
    try { window.chrome.webview.postMessage(JSON.stringify({ cmd: 'translate_sel', text: lastText, seq })); } catch (e) {}
  };
  // перевод из контекстного меню: текст и точку щелчка даёт движок
  window.__rbTranslateText = (text, x, y) => {
    ensure();
    lastText = text.trim();
    anchor = { left: x, right: x, top: y, bottom: y, width: 0, height: 0 };
    translate();
  };
  window.__rbTranslated = (r) => {
    if (!host || r.seq !== seq) return;
    root.querySelector('.l').textContent = r.error ? 'Не удалось перевести' : lang(r.from) + ' → ' + lang(r.to);
    root.querySelector('.t').textContent = r.error || r.text;
    place(card, anchor, true);
  };
  // Что выделено: обычный текст страницы или кусок в поле ввода (там getSelection пуст).
  const selected = () => {
    const active = document.activeElement;
    if (active && /^(INPUT|TEXTAREA)$/.test(active.tagName) && active.type !== 'password' && typeof active.selectionStart === 'number') {
      const text = active.value.slice(active.selectionStart, active.selectionEnd).trim();
      return text ? { text, rect: active.getBoundingClientRect() } : null;
    }
    const sel = getSelection();
    const text = sel && !sel.isCollapsed ? sel.toString().trim() : '';
    return text ? { text, rect: sel.getRangeAt(0).getBoundingClientRect() } : null;
  };
  const check = () => setTimeout(() => {
    const found = selected();
    if (!found || found.text.length > 2000 || !/\p{L}/u.test(found.text)) {
      if (!card || card.style.display !== 'block') hideAll();
      return;
    }
    if (button && button.style.display === 'flex' && found.text === lastText) return;
    ensure();
    lastText = found.text;
    anchor = found.rect;
    card.style.display = 'none';
    button.style.display = 'flex';
    place(button, anchor, false);
  }, 10);
  addEventListener('mouseup', (e) => { if (!(host && e.composedPath().includes(host))) check(); }, true);
  // выделение с клавиатуры: Shift+стрелки, Ctrl+A
  addEventListener('keyup', (e) => { if (e.shiftKey || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') || e.key === 'Shift') check(); }, true);
  addEventListener('mousedown', (e) => { if (host && !e.composedPath().includes(host)) hideAll(); }, true);
  addEventListener('scroll', () => { if (button && button.style.display === 'flex') hideAll(); }, { capture: true, passive: true });
  addEventListener('keydown', (e) => { if (e.key === 'Escape') hideAll(); }, true);
})();"#;

/// Перевод фрагмента через Google Переводчик (тот же, что для страниц): на русский,
/// а русский текст — на английский. Возвращает (перевод, язык оригинала, язык перевода).
/// Ответ translate_a/single (client=gtx): [[["перевод","оригинал",…],…],null,"en",…].
fn parse_gtx(body: &[u8]) -> Option<(String, String)> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let translated: String = value[0].as_array()?.iter().filter_map(|part| part[0].as_str()).collect();
    let from = value[2].as_str().unwrap_or("auto").to_string();
    (!translated.trim().is_empty()).then_some((translated, from))
}

/// Ответ translate_a/t (client=dict-chrome-ex): [["перевод","en"]] или ["перевод"].
fn parse_dict(body: &[u8]) -> Option<(String, String)> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let first = &value[0];
    let (translated, from) = match first.as_array() {
        Some(pair) => (pair.first()?.as_str()?, pair.get(1).and_then(Value::as_str).unwrap_or("auto")),
        None => (first.as_str()?, "auto"),
    };
    (!translated.trim().is_empty()).then(|| (translated.to_string(), from.to_string()))
}

fn translate_text(text: &str) -> Result<(String, String, String), String> {
    let request = |to: &str| -> Result<(String, String), String> {
        let url = format!(
            "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={to}&dt=t&q={}",
            percent_encode(text)
        );
        // основной адрес Google иногда отвечает «слишком много запросов» (429) —
        // тогда спрашиваем адрес, которым пользуется расширение Google Переводчика
        // после отказа основной адрес 10 минут не спрашиваем — сразу запасной, без задержки
        static GTX_BLOCKED_UNTIL: AtomicU64 = AtomicU64::new(0);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        if now >= GTX_BLOCKED_UNTIL.load(Ordering::Relaxed) {
            if let Some(found) = icons::curl(&url, 2_000_000, 6).and_then(|(body, _)| parse_gtx(&body)) {
                return Ok(found);
            }
            GTX_BLOCKED_UNTIL.store(now + 600, Ordering::Relaxed);
        }
        let backup = format!(
            "https://clients5.google.com/translate_a/t?client=dict-chrome-ex&sl=auto&tl={to}&q={}",
            percent_encode(text)
        );
        let (body, _) = icons::curl(&backup, 2_000_000, 10).ok_or("нет связи с переводчиком")?;
        parse_dict(&body).ok_or_else(|| "переводчик ничего не вернул".into())
    };
    let (translated, from) = request("ru")?;
    if from == "ru" {
        let (english, _) = request("en")?;
        return Ok((english, from, "en".into()));
    }
    Ok((translated, from, "ru".into()))
}

const SCROLL_SCRIPT: &str = r#"(() => {
  if (window.top !== window || window.__rbScroll) return;
  window.__rbScroll = true;
  let last = 0, sent = '';
  addEventListener('scroll', (e) => {
    const t = e.target;
    const el = t === document || t === document.documentElement ? document.scrollingElement || document.documentElement : t;
    if (!el || el.scrollHeight - el.clientHeight < 200) return;
    const y = el.scrollTop;
    if (Math.abs(y - last) < 30) return;
    const dir = y > last ? 'down' : 'up';
    last = y;
    if (dir === sent && y > 0) return;
    sent = dir;
    try { window.chrome.webview.postMessage(JSON.stringify({ cmd: 'scroll_dir', dir, y })); } catch (e) {}
  }, { capture: true, passive: true });
  // есть ли на странице видео — иначе «картинка в картинке» в меню неактивна
  let videoAt = '';
  const video = (e) => {
    if (!(e.target instanceof HTMLVideoElement) || videoAt === location.href) return;
    videoAt = location.href;
    try { window.chrome.webview.postMessage(JSON.stringify({ cmd: 'has_video' })); } catch (e) {}
  };
  addEventListener('loadedmetadata', video, true);
  addEventListener('play', video, true);
  // ссылка колёсиком или с Ctrl — в новой вкладке в фоне (сами, а не по состоянию кнопок мыши:
  // к моменту запроса нового окна колёсико уже отпущено)
  const background = (e) => {
    const middle = e.type === 'auxclick' && e.button === 1;
    const ctrl = e.type === 'click' && e.button === 0 && (e.ctrlKey || e.metaKey) && !e.shiftKey;
    if (e.defaultPrevented || (!middle && !ctrl)) return;
    const link = e.composedPath().find((n) => n.tagName === 'A' && n.href);
    if (!link || !/^https?:/i.test(link.href)) return;
    e.preventDefault();
    try { window.chrome.webview.postMessage(JSON.stringify({ cmd: 'open_bg', url: link.href })); } catch (err) {}
  };
  addEventListener('auxclick', background, true);
  addEventListener('click', background, true);
})();"#;

/// Поля параметров, которые относятся к этому компьютеру и не синхронизируются.
const LOCAL_SETTINGS: [&str; 12] = [
    "download_dir", "proxy_mode", "proxy_host", "proxy_port", "sidebar_open",
    "wallpaper_version", "ubol_version", "blocked_total", "battery_saver",
    "history_cleared", // едет вместе с журналом
    "auto_update", "update_source",
];
const SYNC_EVERY: Duration = Duration::from_secs(5 * 60);

impl App {
    // ---------- аккаунт и синхронизация ----------

    fn account_key(&self) -> Option<[u8; 32]> {
        let account = self.account.as_ref()?;
        let hex = passwords::unprotect(&account.key)?;
        let bytes: Vec<u8> = (0..hex.len()).step_by(2).filter_map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok()).collect();
        bytes.try_into().ok()
    }

    /// Всё, что синхронизируется, — по разделам.
    fn collect_sections(&self) -> std::collections::BTreeMap<String, Value> {
        let mut sections = std::collections::BTreeMap::new();
        let mut settings = serde_json::to_value(&self.store.settings).unwrap_or_default();
        if let Value::Object(fields) = &mut settings {
            for key in LOCAL_SETTINGS {
                fields.remove(key);
            }
            // своё фото и видео лежат на этом компьютере
            if matches!(self.store.settings.wallpaper.as_str(), "image" | "video") {
                fields.remove("wallpaper");
            }
        }
        sections.insert("settings".into(), settings);
        sections.insert("bookmarks".into(), json!(self.store.bookmarks));
        sections.insert(
            "history".into(),
            json!({ "cleared": self.store.settings.history_cleared, "visits": self.store.history }),
        );
        let mut logins: Vec<Value> = self
            .vault
            .logins
            .iter()
            .filter_map(|l| {
                let password = passwords::unprotect(&l.secret)?;
                Some(json!({ "origin": l.origin, "username": l.username, "password": password, "created": l.created }))
            })
            .collect();
        logins.sort_by_key(|l| l.to_string());
        sections.insert("passwords".into(), json!({ "logins": logins, "never": self.vault.never }));
        let extensions: Vec<Value> = self
            .extensions
            .iter()
            .map(|e| json!({ "id": e.id, "name": e.name, "source": e.source }))
            .collect();
        sections.insert("extensions".into(), json!(extensions));
        sections.insert("zoom".into(), json!(self.store.site_zoom));
        let notes: String = self.store.read_named("notes.json");
        sections.insert("notes".into(), json!(notes));
        sections.insert("autofill".into(), json!(self.autofill));
        sections.insert("sent_tabs".into(), json!(fresh_sent_tabs(&self.sent_tabs)));
        sections
    }

    /// Применяет раздел из облака (или объединённый) к браузеру.
    fn apply_section(&mut self, name: &str, value: Value) {
        match name {
            "sent_tabs" => {
                self.sent_tabs = value.as_array().cloned().unwrap_or_default();
                self.store.write_named("sent_tabs.json", &self.sent_tabs);
            }
            "settings" => {
                let mut merged = serde_json::to_value(&self.store.settings).unwrap_or_default();
                if let (Value::Object(local), Value::Object(remote)) = (&mut merged, value) {
                    for (key, v) in remote {
                        if !LOCAL_SETTINGS.contains(&key.as_str()) {
                            local.insert(key, v);
                        }
                    }
                }
                if let Ok(settings) = serde_json::from_value::<store::Settings>(merged) {
                    self.store.settings = settings;
                    self.store.save_settings();
                    AMOLED.store(self.store.settings.theme == "amoled", Ordering::Relaxed);
                    THEME_BACKDROP.store(theme_backdrop(&self.store.settings.theme), Ordering::Relaxed);
                    LEFT_CLICK_BACKGROUND.store(self.store.settings.left_click_background, Ordering::Relaxed);
                    self.apply_theme();
                    self.sent.clear();
                }
            }
            "bookmarks" => {
                if let Ok(list) = serde_json::from_value::<Vec<store::Bookmark>>(value) {
                    self.store.bookmarks = list;
                    self.store.save_bookmarks();
                }
            }
            "history" => {
                // готовый итог слияния: заменяем, чтобы доходили и удаления
                let cleared = value["cleared"].as_u64().unwrap_or(0);
                if let Ok(list) = serde_json::from_value::<Vec<store::Visit>>(value["visits"].clone()) {
                    self.store.replace_history(list.into_iter().filter(|v| v.time > cleared).collect());
                }
                if cleared > self.store.settings.history_cleared {
                    self.store.settings.history_cleared = cleared;
                    self.store.save_settings();
                }
            }
            "passwords" => {
                let logins = value["logins"].as_array().cloned().unwrap_or_default();
                let mut vault = passwords::Vault {
                    logins: Vec::new(),
                    never: serde_json::from_value(value["never"].clone()).unwrap_or_default(),
                };
                for login in logins {
                    let (Some(origin), Some(username), Some(password)) =
                        (login["origin"].as_str(), login["username"].as_str(), login["password"].as_str())
                    else {
                        continue;
                    };
                    vault.upsert(origin, username, password, login["created"].as_u64().unwrap_or_else(unix_now));
                }
                self.vault = vault;
                self.store.write_named("passwords.json", &self.vault);
            }
            "extensions" => {
                // недостающие ставим из Chrome Web Store; расширения только из магазина Edge там не найдутся
                let wanted: Vec<(String, String)> = value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| Some((e["id"].as_str()?.to_string(), e["name"].as_str().unwrap_or("").to_string())))
                    .collect();
                let missing: Vec<String> = wanted
                    .into_iter()
                    .map(|(id, _)| id)
                    .filter(|id| webstore::valid_id(id) && !self.extensions.iter().any(|e| &e.id == id) && !self.ext_busy.contains(id))
                    .collect();
                for id in missing {
                    self.ext_busy.insert(id.clone());
                    self.ext_quiet.insert(id.clone());
                    let proxy = self.proxy.clone();
                    let engine = wry::webview_version().unwrap_or_default();
                    std::thread::spawn(move || {
                        let result = webstore::download(&id, &Self::extensions_dir(), &engine).map_err(|e| (id, e));
                        let _ = proxy.send_event(UserEvent::ExtReady(result, None));
                    });
                }
            }
            "autofill" => {
                if let Ok(profile) = serde_json::from_value::<autofill::Profile>(value) {
                    self.autofill = profile;
                    self.save_autofill();
                }
            }
            "zoom" => {
                if let Ok(map) = serde_json::from_value(value) {
                    self.store.site_zoom = map;
                    self.store.write_named("zoom.json", &self.store.site_zoom);
                }
            }
            "notes" => {
                if let Some(text) = value.as_str() {
                    self.store.write_named("notes.json", &text.to_string());
                    self.push_panel_state();
                }
            }
            _ => {}
        }
    }

    /// Объединение, когда раздел поменяли и здесь, и на другом компьютере.
    fn merge_section(name: &str, local: &Value, remote: &Value) -> Value {
        match name {
            "bookmarks" => sync::union_by(remote, local, |b| b["url"].to_string()),
            "history" => {
                // очистка на любом компьютере стирает всё, что было до неё
                let cleared = remote["cleared"].as_u64().unwrap_or(0).max(local["cleared"].as_u64().unwrap_or(0));
                let visits = sync::union_by(&remote["visits"], &local["visits"], |v| format!("{}|{}", v["url"], v["time"]));
                let visits: Vec<Value> = visits
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|v| v["time"].as_u64().unwrap_or(0) > cleared)
                    .cloned()
                    .collect();
                json!({ "cleared": cleared, "visits": visits })
            }
            "passwords" => json!({
                "logins": sync::union_by(&remote["logins"], &local["logins"], |l| format!("{}|{}", l["origin"], l["username"])),
                "never": sync::union_by(&remote["never"], &local["never"], |o| o.to_string()),
            }),
            "extensions" => sync::union_by(remote, local, |e| e["id"].to_string()),
            "sent_tabs" => json!(fresh_sent_tabs(sync::union_by(remote, local, |t| t["id"].to_string()).as_array().unwrap_or(&Vec::new()))),
            "autofill" => json!({
                "addresses": sync::union_by(&remote["addresses"], &local["addresses"], |a| a.to_string()),
                "cards": sync::union_by(&remote["cards"], &local["cards"], |c| c["number"].to_string()),
            }),
            "notes" => {
                let (r, l) = (remote.as_str().unwrap_or(""), local.as_str().unwrap_or(""));
                if r.is_empty() || l.contains(r) {
                    local.clone()
                } else if l.is_empty() || r.contains(l) {
                    remote.clone()
                } else {
                    json!(format!("{r}\n\n——— с этого компьютера ———\n\n{l}"))
                }
            }
            // параметры и масштаб: облачные значения поверх своих
            _ => sync::merge_objects(remote, local),
        }
    }

    /// Слить пакет собеседника `peer` в браузер; `only` — какие разделы (у телефона их меньше).
    /// Возвращает, сколько разделов поменялось здесь.
    fn merge_from(&mut self, remote: &sync::Bundle, peer: &str, only: Option<&[String]>) -> usize {
        let base = self.account.as_ref().and_then(|a| a.bases.get(peer)).cloned().unwrap_or_default();
        let local = self.collect_sections();
        let mut changed = 0;
        for (name, local_value) in &local {
            if only.is_some_and(|names| !names.contains(name)) {
                continue;
            }
            let value = match sync::choose(local_value, remote.sections.get(name), base.get(name)) {
                sync::Choice::Local => continue,
                sync::Choice::Remote => remote.sections[name].clone(),
                sync::Choice::Merge => Self::merge_section(name, local_value, &remote.sections[name]),
            };
            if value != *local_value {
                self.apply_section(name, value);
                changed += 1;
            }
        }
        changed
    }

    fn fingerprints(sections: &std::collections::BTreeMap<String, Value>) -> std::collections::BTreeMap<String, String> {
        sections.iter().map(|(k, v)| (k.clone(), sync::fingerprint(v))).collect()
    }

    /// Обмен с телефоном по Wi‑Fi: его пакет внутрь, в ответ — слитые разделы.
    fn lan_exchange(&mut self, body: &[u8]) -> Option<Vec<u8>> {
        let account = self.account.clone()?;
        let key = self.account_key()?;
        let remote = sync::decrypt(body, &key).ok()?;
        if remote.id.is_empty() {
            return None;
        }
        let peer = format!("peer:{}", remote.id);
        let names: Vec<String> = remote.sections.keys().cloned().collect();
        let changed = self.merge_from(&remote, &peer, Some(&names));
        let sections: std::collections::BTreeMap<String, Value> =
            self.collect_sections().into_iter().filter(|(k, _)| names.contains(k)).collect();
        let hashes = Self::fingerprints(&sections);
        if let Some(acc) = &mut self.account {
            acc.bases.insert(peer, hashes);
            let name = if remote.device.is_empty() { "Телефон".to_string() } else { remote.device.clone() };
            acc.peers.insert(remote.id.clone(), sync::Peer { name, seen: unix_now() });
            acc.synced = unix_now();
        }
        self.save_account();
        self.open_received_tabs();
        self.dirty = true;
        if changed > 0 {
            // с телефона пришло новое — скоро и на диск, для других компьютеров
            self.last_sync = Instant::now() - SYNC_EVERY + Duration::from_secs(3);
        }
        let bundle = sync::Bundle {
            sections,
            device: std::env::var("COMPUTERNAME").unwrap_or_default(),
            id: account.device.clone(),
            time: unix_now(),
        };
        sync::encrypt(&bundle, &key, &account.group).ok()
    }

    /// Обмен с диском WebDAV. Сеть — в фоне: скачать, слить здесь, записать в фоне.
    fn sync_now(&mut self, manual: bool) {
        let Some(account) = self.account.clone() else { return };
        self.last_sync = Instant::now();
        let Some(dav) = account.webdav.clone() else {
            if manual {
                let text = if account.peers.is_empty() {
                    "Подключите телефон по QR-коду или диск WebDAV"
                } else {
                    "Телефон синхронизируется сам, когда он в той же сети Wi‑Fi"
                };
                let _ = self.ui.evaluate_script(&format!("ui.toast({text:?})"));
            }
            return;
        };
        if self.dav_busy {
            return;
        }
        let (Some(key), Some(pass)) = (self.account_key(), passwords::unprotect(&dav.pass)) else {
            self.sync_error = Some("ключи не расшифровались — подключите синхронизацию заново".into());
            self.dirty = true;
            return;
        };
        self.dav_busy = true;
        let proxy = self.proxy.clone();
        let file = sync::dav_file(&dav.url, &account.group);
        std::thread::spawn(move || {
            let result = sync::dav_get(&file, &dav.user, &pass).and_then(|found| match found {
                Some((bytes, etag)) => sync::decrypt(&bytes, &key).map(|b| (Some(b), etag)),
                None => Ok((None, String::new())),
            });
            let _ = proxy.send_event(UserEvent::DavFetched(result, manual));
        });
    }

    fn dav_fetched(&mut self, result: Result<(Option<sync::Bundle>, String), String>, manual: bool) {
        let (remote, etag) = match result {
            Ok(found) => found,
            Err(error) => {
                self.dav_busy = false;
                self.sync_error = Some(error);
                self.dirty = true;
                return;
            }
        };
        let Some(account) = self.account.clone() else {
            self.dav_busy = false;
            return;
        };
        let changed = remote.as_ref().map_or(0, |r| self.merge_from(r, "webdav", None));
        // на диск — всё своё; разделы, которых здесь нет (с других устройств), сохраняем
        let mut sections = remote.as_ref().map(|r| r.sections.clone()).unwrap_or_default();
        sections.extend(self.collect_sections());
        let hashes = Self::fingerprints(&sections);
        if remote.as_ref().is_some_and(|r| Self::fingerprints(&r.sections) == hashes) {
            return self.dav_stored(Ok(etag), hashes, manual, changed);
        }
        let (Some(key), Some(dav)) = (self.account_key(), account.webdav.clone()) else {
            self.dav_busy = false;
            return;
        };
        let pass = passwords::unprotect(&dav.pass).unwrap_or_default();
        let bundle = sync::Bundle {
            sections,
            device: std::env::var("COMPUTERNAME").unwrap_or_default(),
            id: account.device.clone(),
            time: unix_now(),
        };
        let bytes = match sync::encrypt(&bundle, &key, &account.group) {
            Ok(bytes) => bytes,
            Err(error) => return self.dav_stored(Err(error), hashes, manual, changed),
        };
        let proxy = self.proxy.clone();
        let file = sync::dav_file(&dav.url, &account.group);
        std::thread::spawn(move || {
            let result = sync::dav_put(&file, &dav.user, &pass, &bytes)
                .map(|()| sync::dav_etag(&file, &dav.user, &pass).ok().flatten().unwrap_or_default());
            let _ = proxy.send_event(UserEvent::DavStored(result, hashes, manual, changed));
        });
    }

    fn dav_stored(&mut self, result: Result<String, String>, hashes: std::collections::BTreeMap<String, String>, manual: bool, changed: usize) {
        self.dav_busy = false;
        self.dirty = true;
        let etag = match result {
            Ok(etag) => etag,
            Err(error) => {
                self.sync_error = Some(format!("на диск не записалось: {error}"));
                return;
            }
        };
        if let Some(account) = &mut self.account {
            account.bases.insert("webdav".into(), hashes);
            if let Some(dav) = &mut account.webdav {
                dav.etag = etag;
                dav.synced = unix_now();
            }
            account.synced = unix_now();
        }
        self.sync_error = None;
        self.open_received_tabs();
        self.save_account();
        if manual {
            let text = if changed > 0 { format!("Синхронизировано: обновлено разделов — {changed}") } else { "Всё синхронизировано".to_string() };
            let _ = self.ui.evaluate_script(&format!("ui.toast({text:?})"));
        }
    }

    /// Раз в 30 секунд: телефону — знак, что есть новое; диску — раз в 5 минут,
    /// а если файл на диске поменялся (проверка метки раз в минуту) — сразу.
    fn sync_tick(&mut self) {
        let Some(account) = &self.account else { return };
        let phone_recent = account.peers.values().any(|p| unix_now().saturating_sub(p.seen) < 15 * 60);
        let dav = account.webdav.clone();
        let group = account.group.clone();
        if phone_recent {
            self.bump_lan_rev();
        }
        let Some(dav) = dav else { return };
        if self.last_sync.elapsed() >= SYNC_EVERY {
            return self.sync_now(false);
        }
        if self.dav_busy || self.dav_checked.elapsed() < Duration::from_secs(55) {
            return;
        }
        let Some(pass) = passwords::unprotect(&dav.pass) else { return };
        self.dav_checked = Instant::now();
        self.dav_busy = true;
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let etag = sync::dav_etag(&sync::dav_file(&dav.url, &group), &dav.user, &pass).ok().flatten();
            let _ = proxy.send_event(UserEvent::DavEtag(etag));
        });
    }

    /// Поменялось то, что нужно телефону, — номер версии растёт, телефон придёт за данными.
    fn bump_lan_rev(&mut self) {
        let sections = self.collect_sections();
        let phone: Vec<String> = ["bookmarks", "history", "passwords", "sent_tabs"]
            .iter()
            .map(|k| sections.get(*k).map(sync::fingerprint).unwrap_or_default())
            .collect();
        let fp = phone.join("|");
        if fp != self.lan_fp {
            self.lan_fp = fp;
            self.lan_rev.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Сервер в домашней сети — один на всё время работы; номер связки можно менять на лету.
    fn start_lan(&mut self) {
        let Some(account) = &self.account else { return };
        if let Ok(mut group) = self.lan_group.lock() {
            *group = account.group.clone();
        }
        if self.lan_port.is_some() || self.inprivate {
            return;
        }
        let proxy = Mutex::new(self.proxy.clone());
        let lan = sync::Lan { group: self.lan_group.clone(), rev: self.lan_rev.clone() };
        self.lan_port = sync::serve(sync::bind_address(), lan, move |body| {
            let (tx, rx) = std::sync::mpsc::channel();
            proxy.lock().ok()?.send_event(UserEvent::LanSync(body, tx)).ok()?;
            rx.recv_timeout(Duration::from_secs(20)).ok().flatten()
        });
    }

    /// Включить синхронизацию: новая связка или вход в чужую по коду.
    fn sync_enable(&mut self, pair: Option<sync::Pair>) {
        let (group, key) = match &pair {
            Some(pair) => (pair.g.clone(), base64_decode(&pair.k).unwrap_or_default()),
            None => (sync::hex(&sync::random_bytes::<8>()), sync::random_bytes::<32>().to_vec()),
        };
        let Some(protected) = passwords::protect(&sync::hex(&key)) else {
            let _ = self.ui.evaluate_script("ui.accountError('Windows не дала защитить ключ')");
            return;
        };
        let webdav = pair.as_ref().and_then(|p| p.w.as_ref()).and_then(|w| {
            Some(sync::WebDav { url: w.u.clone(), user: w.l.clone(), pass: passwords::protect(&w.p)?, ..Default::default() })
        });
        self.account = Some(sync::Account {
            group,
            key: protected,
            device: sync::hex(&sync::random_bytes::<6>()),
            webdav,
            ..Default::default()
        });
        self.sync_error = None;
        self.save_account();
        self.start_lan();
        self.send_pair();
        self.dirty = true;
        if pair.is_some() {
            self.sync_now(true);
        }
    }

    /// Код связки и его QR — для экрана «Подключить телефон».
    fn send_pair(&mut self) {
        let Some(account) = &self.account else { return };
        let Some(key) = self.account_key() else { return };
        let webdav = account.webdav.as_ref().and_then(|d| {
            Some(sync::PairDav { u: d.url.clone(), l: d.user.clone(), p: passwords::unprotect(&d.pass)? })
        });
        let pair = sync::Pair {
            g: account.group.clone(),
            k: base64(&key),
            n: std::env::var("COMPUTERNAME").unwrap_or_default(),
            a: sync::local_ips(),
            p: self.lan_port.unwrap_or(sync::LAN_PORT),
            w: webdav,
        };
        let code = sync::pair_code(&pair);
        let svg = sync::qr_svg(&code);
        let _ = self.ui.evaluate_script(&format!("ui.setPair({})", json!({ "code": code, "svg": svg })));
    }

    /// Подключить диск: сначала проверить, что пускает и запись проходит.
    fn dav_connect(&mut self, url: String, user: String, pass: String) {
        let Some(account) = &self.account else { return };
        let url = url.trim().to_string();
        let fail = |app: &mut App, text: &str| {
            let _ = app.ui.evaluate_script(&format!("ui.accountError({text:?})"));
        };
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return fail(self, "Адрес диска начинается с https://");
        }
        if user.trim().is_empty() || pass.is_empty() {
            return fail(self, "Укажите логин и пароль приложения");
        }
        let Some(protected) = passwords::protect(&pass) else { return fail(self, "Windows не дала защитить пароль") };
        let dav = sync::WebDav { url: url.clone(), user: user.trim().to_string(), pass: protected, ..Default::default() };
        let file = sync::dav_file(&url, &account.group);
        self.dav_busy = true;
        self.dirty = true;
        let proxy = self.proxy.clone();
        let user = user.trim().to_string();
        std::thread::spawn(move || {
            let result = sync::dav_etag(&file, &user, &pass).map(|_| ());
            let _ = proxy.send_event(UserEvent::DavChecked(result, dav));
        });
    }

    /// Вкладки, которые отправили на этот компьютер: открываются в фоне, каждая один раз.
    fn open_received_tabs(&mut self) {
        let incoming: Vec<(String, String, String, String)> = self
            .sent_tabs
            .iter()
            .filter(|t| t["to"] == "pc")
            .filter_map(|t| {
                let id = t["id"].as_str()?.to_string();
                let url = t["url"].as_str().filter(|u| u.starts_with("http"))?.to_string();
                Some((id, url, t["title"].as_str().unwrap_or("").to_string(), t["from"].as_str().unwrap_or("").to_string()))
            })
            .filter(|(id, ..)| !self.sent_handled.contains(id))
            .collect();
        if incoming.is_empty() {
            return;
        }
        for (id, url, ..) in &incoming {
            self.new_background_tab(url);
            self.sent_handled.insert(id.clone());
        }
        self.store.write_named("sent_handled.json", &self.sent_handled);
        let (title, from) = (&incoming[0].2, &incoming[0].3);
        let text = if incoming.len() == 1 {
            format!("Вкладка с телефона{}: {}", if from.is_empty() { String::new() } else { format!(" ({from})") }, if title.is_empty() { &incoming[0].1 } else { title })
        } else {
            format!("С телефона пришло вкладок: {}", incoming.len())
        };
        let _ = self.ui.evaluate_script(&format!("ui.toast({text:?})"));
    }

    /// «Отправить на телефон»: телефон в той же сети заберёт её за несколько секунд,
    /// не в сети — через диск.
    fn send_tab_to_phone(&mut self, id: u32) {
        let Some(tab) = self.tab(id).filter(|t| t.url.starts_with("http")) else { return };
        if self.account.is_none() {
            let _ = self.ui.evaluate_script("ui.toast('Чтобы отправлять вкладки, включите синхронизацию: Параметры → Синхронизация')");
            return;
        }
        let key = sync::hex(&sync::random_bytes::<8>());
        let entry = json!({
            "id": key,
            "url": tab.url,
            "title": tab.title,
            "from": std::env::var("COMPUTERNAME").unwrap_or_default(),
            "to": "phone",
            "time": unix_now(),
        });
        self.sent_tabs.push(entry);
        self.store.write_named("sent_tabs.json", &self.sent_tabs);
        self.bump_lan_rev();
        self.sync_now(false);
        let _ = self.ui.evaluate_script("ui.toast('Вкладка отправлена — откроется на телефоне')");
    }

    fn save_account(&self) {
        let path = data_dir().join("account.json");
        match &self.account {
            Some(account) => self.store.write_named("account.json", account),
            None => {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    fn account_state(&self) -> Value {
        match &self.account {
            Some(a) => {
                let peers: Vec<Value> = a.peers.values().map(|p| json!({ "name": p.name, "seen": p.seen })).collect();
                json!({
                    "on": true,
                    "device": std::env::var("COMPUTERNAME").unwrap_or_default(),
                    "synced": a.synced,
                    "error": self.sync_error,
                    "busy": self.dav_busy,
                    "lan": self.lan_port,
                    "peers": peers,
                    "webdav": a.webdav.as_ref().map(|d| json!({ "url": d.url, "user": d.user, "synced": d.synced })),
                })
            }
            None => json!({ "on": false, "busy": self.dav_busy }),
        }
    }
}

/// Выгружает вкладку: webview и процесс страницы уничтожаются, адрес остаётся;
/// при открытии страница загрузится заново и вернётся к прежней прокрутке.
fn unload(tab: &mut Tab) {
    tab.webview = None;
    tab.restore_scroll = tab.scroll;
    tab.sleeping = true;
    tab.loading = false;
}

/// Ближайшая к `from` вкладка (сначала справа, потом слева) не из группы `leaving`
/// и не из свёрнутых групп; если таких нет — любая не из `leaving`. `groups` — группа каждой вкладки.
fn nearest_outside(groups: &[u32], collapsed: &[u32], from: usize, leaving: u32) -> Option<usize> {
    let order = (from + 1..groups.len()).chain((0..from.min(groups.len())).rev());
    let fits = |i: &usize| groups[*i] != leaving;
    order.clone().filter(fits).find(|i| !collapsed.contains(&groups[*i])).or_else(|| order.filter(fits).next())
}

/// Группа вкладки после перетаскивания между соседями с группами `before` и `after`
/// (None — края полосы). `others` — остались ли в её группе другие вкладки.
fn group_after_move(before: Option<u32>, after: Option<u32>, own: u32, others: bool) -> u32 {
    match (before, after) {
        // между двумя вкладками одной группы — внутри неё
        (Some(a), Some(b)) if a == b && a != 0 => a,
        _ if own == 0 || !others => own,
        // рядом с вкладкой своей группы — остаётся в ней
        _ if before == Some(own) || after == Some(own) => own,
        _ => 0,
    }
}

#[derive(Debug, PartialEq)]
enum Rest {
    Keep,
    Suspend,
    Discard,
}

/// Вкладка глазами «экономии памяти».
struct PressureTab {
    id: u32,
    hidden_for: Option<Duration>,
    /// Играет звук, закреплена или уже выгружена — не трогаем.
    keep: bool,
    /// Тот же сайт, что и открытая вкладка: к нему, скорее всего, вернутся.
    same_site: bool,
}

/// Какие вкладки выгрузить при загрузке памяти системы `load` %. До 80% — никакие;
/// 80–89% — одну за проверку (раз в 30 с), от 90% — до трёх. Только скрытые дольше минуты,
/// сначала чужих сайтов, среди них — давно скрытые.
fn pressure_victims(tabs: &[PressureTab], load: u32) -> Vec<u32> {
    let count = match load {
        0..=79 => return Vec::new(),
        80..=89 => 1,
        _ => 3,
    };
    let mut list: Vec<&PressureTab> = tabs
        .iter()
        .filter(|t| !t.keep && t.hidden_for.is_some_and(|h| h >= Duration::from_secs(60)))
        .collect();
    list.sort_by(|a, b| a.same_site.cmp(&b.same_site).then(b.hidden_for.cmp(&a.hidden_for)));
    list.into_iter().take(count).map(|t| t.id).collect()
}

/// Что сделать с фоновой вкладкой, скрытой `hidden_for`, при пороге сна `limit_minutes`.
fn rest_action(
    hidden_for: Option<Duration>,
    limit_minutes: u32,
    sleeping: bool,
    audio: bool,
    loaded: bool,
) -> Rest {
    // Вкладку со звуком не трогаем: на ней может играть музыка.
    let Some(hidden_for) = hidden_for else { return Rest::Keep };
    if limit_minutes == 0 || audio || !loaded {
        return Rest::Keep;
    }
    let limit = Duration::from_secs(limit_minutes as u64 * 60);
    if hidden_for >= limit * 3 {
        Rest::Discard
    } else if hidden_for >= limit && !sleeping {
        Rest::Suspend
    } else {
        Rest::Keep
    }
}

/// Прокси в цепочке настройки webview: без прокси — ничего не меняем.
trait WithNetwork {
    fn with_network(self, config: Option<wry::ProxyConfig>) -> Self;
}

impl WithNetwork for WebViewBuilder<'_> {
    /// Аргументы движка собираем сами: те же, что ставит wry, плюс прокси и лимит процессов.
    /// У всех webview они одинаковые — иначе WebView2 не дал бы им общий профиль.
    fn with_network(self, config: Option<wry::ProxyConfig>) -> Self {
        use wry::WebViewBuilderExtWindows;
        self.with_additional_browser_args(engine_args(config.as_ref(), PROCESS_LIMIT.load(Ordering::Relaxed)))
    }
}

/// Свои сочетания клавиш из параметров — в общее состояние клавиатуры.
fn apply_shortcuts(keys: &KeyState, list: &[store::Shortcut]) {
    if let Ok(mut custom) = keys.custom.write() {
        *custom = list
            .iter()
            .filter_map(|s| Some((s.key, s.mods, platform::hotkey_by_id(&s.action)?)))
            .collect();
    }
}

/// Сколько процессов страниц может запустить движок (0 — сколько решит сам).
static PROCESS_LIMIT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn engine_args(proxy: Option<&wry::ProxyConfig>, process_limit: u32) -> String {
    let mut args = String::from(
        // первые три — флаги wry (наши аргументы заменяют его набор), остальные — фоновые службы
        // Chromium, которые браузеру не нужны: перевод страниц, подсказки оптимизации, трансляция
        // на ТВ и запасной процесс страницы, который держится в памяти «на всякий случай»
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate,OptimizationHints,MediaRouter,SpareRendererForSitePerProcess --autoplay-policy=no-user-gesture-required",
    );
    match proxy {
        Some(wry::ProxyConfig::Http(e)) => args.push_str(&format!(" --proxy-server=http://{}:{}", e.host, e.port)),
        Some(wry::ProxyConfig::Socks5(e)) => args.push_str(&format!(" --proxy-server=socks5://{}:{}", e.host, e.port)),
        None => {}
    }
    if process_limit > 0 {
        // сайты делят процессы: меньше памяти, но одна тяжёлая вкладка может тормозить соседей
        args.push_str(&format!(" --renderer-process-limit={process_limit}"));
    }
    args
}

/// Прокси из настроек для движка (None — без прокси).
fn proxy_config(settings: &store::Settings) -> Option<wry::ProxyConfig> {
    if !valid_proxy_host(&settings.proxy_host) || settings.proxy_port == 0 {
        return None;
    }
    let endpoint = wry::ProxyEndpoint {
        host: settings.proxy_host.clone(),
        port: settings.proxy_port.to_string(),
    };
    match settings.proxy_mode.as_str() {
        "http" => Some(wry::ProxyConfig::Http(endpoint)),
        "socks5" => Some(wry::ProxyConfig::Socks5(endpoint)),
        _ => None,
    }
}

/// Адрес прокси: имя или IP без пробелов, схем и путей — он попадает в строку запуска движка.
fn valid_proxy_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']'))
}

/// "#rrggbb" — шесть шестнадцатеричных цифр после решётки.
fn is_hex_color(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Готовые обои; "image" — своё фото.
const WALLPAPERS: [&str; 12] = [
    "aurora", "sunset", "ocean", "forest", "night", "mono", // спокойные
    "waves", "stars", "particles", "flow", // живые
    "image", "video", // свои
];
const VIDEO_TYPES: [(&str, &str); 2] = [("mp4", "video/mp4"), ("webm", "video/webm")];
/// Больше за один запрос не отдаём: видео читается кусками, а не целиком в память.
const VIDEO_CHUNK: u64 = 4 * 1024 * 1024;

/// Готовые живые обои: видео со свободной лицензией из папки assets/walls
/// (рядом с exe или выше по дереву — так находит и собранный, и скопированный exe).
#[derive(serde::Deserialize, Serialize, Clone)]
struct LiveWall {
    id: String,
    name: String,
    file: String,
    license: String,
    author: String,
    source: String,
}

fn live_walls() -> &'static (PathBuf, Vec<LiveWall>) {
    static WALLS: std::sync::OnceLock<(PathBuf, Vec<LiveWall>)> = std::sync::OnceLock::new();
    WALLS.get_or_init(|| {
        let exe = std::env::current_exe().unwrap_or_default();
        let dir = exe
            .ancestors()
            .skip(1)
            .take(5)
            .flat_map(|d| [d.join("wallpapers"), d.join("assets").join("walls")])
            .find(|d| d.join("credits.json").exists());
        let Some(dir) = dir else { return (PathBuf::new(), Vec::new()) };
        let text = std::fs::read_to_string(dir.join("credits.json")).unwrap_or_default();
        let list: Vec<LiveWall> = serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default();
        let list = list
            .into_iter()
            .filter(|w| !w.file.contains(['/', '\\']) && dir.join(&w.file).is_file())
            .collect();
        (dir, list)
    })
}

fn live_wall_file(id: &str) -> Option<PathBuf> {
    let (dir, list) = live_walls();
    list.iter().find(|w| w.id == id).map(|w| dir.join(&w.file))
}

/// Своё видео для обоев в папке данных, если оно есть.
fn video_file() -> Option<(PathBuf, &'static str)> {
    VIDEO_TYPES.iter().find_map(|(ext, mime)| {
        let path = data_dir().join(format!("wallpaper-video.{ext}"));
        path.exists().then_some((path, *mime))
    })
}

/// Разбирает заголовок Range ("bytes=100-" или "bytes=100-199") для файла длины `len`.
/// Возвращает включительные границы куска, урезанного до VIDEO_CHUNK.
fn parse_range(header: Option<&str>, len: u64) -> Option<(u64, u64)> {
    if len == 0 {
        return None;
    }
    let spec = header?.trim().strip_prefix("bytes=")?;
    let (start, end) = spec.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    if start >= len {
        return None;
    }
    let wanted_end = end.trim().parse::<u64>().unwrap_or(len - 1).min(len - 1);
    if wanted_end < start {
        return None;
    }
    Some((start, wanted_end.min(start + VIDEO_CHUNK - 1)))
}
const IMAGE_TYPES: [(&str, &str); 6] = [
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("png", "image/png"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
    ("bmp", "image/bmp"),
];

/// Файл своих обоев в папке данных, если он есть.
fn wallpaper_file() -> Option<(PathBuf, &'static str)> {
    IMAGE_TYPES.iter().find_map(|(ext, mime)| {
        let path = data_dir().join(format!("wallpaper.{ext}"));
        path.exists().then_some((path, *mime))
    })
}

/// Видео отдаётся кусками по заголовку Range — так его и запрашивает тег <video>.
fn serve_video(found: Option<(PathBuf, &str)>, range: Option<&str>) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    use std::io::{Read, Seek, SeekFrom};
    let not_found = || {
        wry::http::Response::builder().status(404).body(Vec::new().into()).unwrap_or_default()
    };
    let Some((path, mime)) = found else { return not_found() };
    let Ok(mut file) = std::fs::File::open(path) else { return not_found() };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let (start, end) = parse_range(range, len).unwrap_or((0, len.saturating_sub(1).min(VIDEO_CHUNK - 1)));
    let mut bytes = vec![0u8; (end + 1 - start) as usize];
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut bytes).is_err() {
        return not_found();
    }
    wry::http::Response::builder()
        .status(206)
        .header("Content-Type", mime)
        .header("Accept-Ranges", "bytes")
        .header("Content-Range", format!("bytes {start}-{end}/{len}"))
        .header("Content-Length", bytes.len().to_string())
        .body(bytes.into())
        .unwrap_or_default()
}

/// Отвечает на запросы интерфейса к http://rb.localhost/…
fn serve_local(path: &str, range: Option<&str>) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    if path == "/wallpaper-video" {
        return serve_video(video_file(), range);
    }
    if let Some(icon) = path.strip_prefix("/exticon/").and_then(extension_icon) {
        let mime = match icon.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "svg" => "image/svg+xml",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            _ => "image/png",
        };
        return match std::fs::read(icon) {
            Ok(bytes) => wry::http::Response::builder().header("Content-Type", mime).body(bytes.into()).unwrap_or_default(),
            Err(_) => wry::http::Response::builder().status(404).body(Vec::new().into()).unwrap_or_default(),
        };
    }
    if let Some(id) = path.strip_prefix("/live/") {
        return serve_video(live_wall_file(id).map(|p| (p, "video/webm")), range);
    }
    let found = (path == "/wallpaper")
        .then(wallpaper_file)
        .flatten()
        .and_then(|(file, mime)| std::fs::read(file).ok().map(|bytes| (bytes, mime)));
    match found {
        Some((bytes, mime)) => wry::http::Response::builder()
            .header("Content-Type", mime)
            .header("Cache-Control", "max-age=31536000")
            .body(bytes.into())
            .unwrap_or_default(),
        None => wry::http::Response::builder()
            .status(404)
            .body(Vec::new().into())
            .unwrap_or_default(),
    }
}

/// Превращает ввод из адресной строки в URL: адрес открываем, остальное ищем.
fn to_url(input: &str, search: &str) -> String {
    let input = input.trim();
    if input.contains("://") || input.starts_with("about:") {
        input.to_string()
    } else if !input.contains(' ') && (input.contains('.') || input.starts_with("localhost")) {
        format!("https://{input}")
    } else {
        format!("{search}{}", percent_encode(input))
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Обратное к base64; None — если в тексте посторонние символы.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DDG: &str = "https://duckduckgo.com/?q=";

    #[test]
    fn address_gets_https() {
        assert_eq!(to_url("example.com", DDG), "https://example.com");
        assert_eq!(to_url("localhost:3000", DDG), "https://localhost:3000");
    }

    #[test]
    fn full_url_is_kept() {
        assert_eq!(to_url(" http://example.com/a b ", DDG), "http://example.com/a b");
    }

    #[test]
    fn text_becomes_search() {
        assert_eq!(to_url("rust lang", DDG), "https://duckduckgo.com/?q=rust%20lang");
        assert_eq!(to_url("кот", DDG), "https://duckduckgo.com/?q=%D0%BA%D0%BE%D1%82");
    }

    #[test]
    fn search_uses_chosen_engine() {
        assert_eq!(
            to_url("кот", search_url("yandex")),
            "https://yandex.ru/search/?text=%D0%BA%D0%BE%D1%82"
        );
        assert_eq!(to_url("a b", search_url("google")), "https://www.google.com/search?q=a%20b");
        // Адреса от выбора поисковика не зависят.
        assert_eq!(to_url("example.com", search_url("bing")), "https://example.com");
    }

    #[test]
    fn unknown_engine_falls_back_to_default() {
        let google = "https://www.google.com/search?q=";
        assert_eq!(search_url("nope"), google);
        assert_eq!(search_url(""), google);
        assert_eq!(search_url(&store::Settings::default().search), google);
        assert_eq!(search_url("duckduckgo"), DDG);
    }

    #[test]
    fn background_tabs_sleep_then_unload() {
        let min = |m: u64| Some(Duration::from_secs(m * 60));
        assert_eq!(rest_action(min(5), 15, false, false, true), Rest::Keep);
        assert_eq!(rest_action(min(15), 15, false, false, true), Rest::Suspend);
        assert_eq!(rest_action(min(20), 15, true, false, true), Rest::Keep); // уже спит
        assert_eq!(rest_action(min(45), 15, true, false, true), Rest::Discard);
    }

    #[test]
    fn translator_answers_are_parsed() {
        let gtx = r#"[[["Привет, ","Hello, ",null],["мир","world",null]],null,"en"]"#.as_bytes();
        assert_eq!(parse_gtx(gtx), Some(("Привет, мир".into(), "en".into())));
        assert_eq!(parse_gtx(b"<html>Sorry</html>"), None);
        assert_eq!(parse_dict(r#"[["Безумная информатика","en"]]"#.as_bytes()), Some(("Безумная информатика".into(), "en".into())));
        assert_eq!(parse_dict(r#"["Привет"]"#.as_bytes()), Some(("Привет".into(), "auto".into())));
        assert_eq!(parse_dict(b"[]"), None);
    }

    #[test]
    fn memory_pressure_unloads_other_sites_first() {
        let tab = |id, secs: Option<u64>, keep, same_site| PressureTab {
            id,
            hidden_for: secs.map(Duration::from_secs),
            keep,
            same_site,
        };
        let tabs = [
            tab(1, None, false, true),          // активная
            tab(2, Some(600), false, true),     // тот же сайт, давно
            tab(3, Some(120), false, false),    // чужой сайт
            tab(4, Some(900), false, false),    // чужой сайт, дольше всех
            tab(5, Some(3000), true, false),    // играет звук
            tab(6, Some(30), false, false),     // скрыта только что
        ];
        assert!(pressure_victims(&tabs, 70).is_empty());
        assert_eq!(pressure_victims(&tabs, 85), vec![4]);
        assert_eq!(pressure_victims(&tabs, 95), vec![4, 3, 2]);
    }

    #[test]
    fn some_tabs_are_never_unloaded() {
        let long = Some(Duration::from_secs(10 * 3600));
        assert_eq!(rest_action(long, 15, false, true, true), Rest::Keep); // играет звук
        assert_eq!(rest_action(long, 0, false, false, true), Rest::Keep); // сон выключен
        assert_eq!(rest_action(long, 15, true, false, false), Rest::Keep); // уже выгружена
        assert_eq!(rest_action(None, 15, false, false, true), Rest::Keep); // активная
    }

    #[test]
    fn local_server_answers_404_for_unknown_paths() {
        assert_eq!(serve_local("/nothing", None).status(), 404);
        assert_eq!(serve_local("/../settings.json", None).status(), 404);
    }

    #[test]
    fn video_ranges_are_parsed_and_capped() {
        let mb = 1024 * 1024;
        assert_eq!(parse_range(Some("bytes=0-"), 100), Some((0, 99)));
        assert_eq!(parse_range(Some("bytes=10-19"), 100), Some((10, 19)));
        assert_eq!(parse_range(Some("bytes=90-500"), 100), Some((90, 99)));
        assert_eq!(parse_range(Some("bytes=0-"), 100 * mb), Some((0, 4 * mb - 1)));
        assert_eq!(parse_range(Some("bytes=100-"), 100), None);
        assert_eq!(parse_range(Some("items=0-1"), 100), None);
        assert_eq!(parse_range(None, 100), None);
        assert_eq!(parse_range(Some("bytes=5-1"), 100), None);
    }

    #[test]
    fn proxy_settings_are_validated() {
        assert!(valid_proxy_host("127.0.0.1"));
        assert!(valid_proxy_host("proxy.example.com"));
        assert!(!valid_proxy_host(""));
        assert!(!valid_proxy_host("evil --flag"));
        assert!(!valid_proxy_host("http://a"));
        let mut settings = store::Settings::default();
        assert!(proxy_config(&settings).is_none());
        settings.proxy_mode = "socks5".into();
        settings.proxy_host = "127.0.0.1".into();
        settings.proxy_port = 1080;
        assert!(matches!(proxy_config(&settings), Some(wry::ProxyConfig::Socks5(_))));
        settings.proxy_port = 0;
        assert!(proxy_config(&settings).is_none());
    }

    #[test]
    fn accent_colors_are_validated() {
        assert!(is_hex_color("#4f5bf5"));
        assert!(is_hex_color("#ABCDEF"));
        assert!(!is_hex_color("4f5bf5"));
        assert!(!is_hex_color("#4f5bf"));
        assert!(!is_hex_color("#4f5bfz"));
        assert!(!is_hex_color("red;}body{"));
    }

    #[test]
    fn engine_args_include_proxy_and_limit() {
        let proxy = wry::ProxyConfig::Socks5(wry::ProxyEndpoint { host: "127.0.0.1".into(), port: "1080".into() });
        let args = engine_args(Some(&proxy), 4);
        assert!(args.contains("--proxy-server=socks5://127.0.0.1:1080"));
        assert!(args.contains("--renderer-process-limit=4"));
        assert!(args.starts_with("--disable-features=msWebOOUI"));
        assert!(!engine_args(None, 0).contains("renderer-process-limit"));
    }

    #[test]
    fn manifest_paths_stay_inside_extension() {
        assert!(safe_relative("icons/128.png"));
        assert!(safe_relative("./icon.png"));
        assert!(!safe_relative("../../secret.txt"));
        assert!(!safe_relative(r"C:\Windows\win.ini"));
        assert!(!safe_relative("/etc/passwd"));
        assert!(!safe_relative(""));
    }

    #[test]
    fn translate_round_trip() {
        let url = "https://my-site.example.com/path/a?q=1#top";
        let t = translate_url(url).unwrap();
        assert_eq!(t, "https://my--site-example-com.translate.goog/path/a?q=1&_x_tr_sl=auto&_x_tr_tl=ru&_x_tr_hl=ru#top");
        assert_eq!(untranslate_url(&t).as_deref(), Some(url));
        assert_eq!(translate_url("https://a.com").unwrap(), "https://a-com.translate.goog/?_x_tr_sl=auto&_x_tr_tl=ru&_x_tr_hl=ru");
        assert_eq!(untranslate_url("https://a-com.translate.goog/?_x_tr_sl=auto&_x_tr_tl=ru").as_deref(), Some("https://a.com/"));
        assert!(translate_url("file:///C:/a.html").is_none());
    }

    #[test]
    fn base64_matches_reference() {
        for sample in [&b""[..], b"f", b"fo", b"foo", b"foob", &[0xff, 0, 0x10]] {
            assert_eq!(base64_decode(&base64(sample)).as_deref(), Some(sample));
        }
        assert_eq!(base64_decode("a*b"), None);
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xFF, 0xD8, 0xFF]), "/9j/");
    }

    #[test]
    fn zoom_levels_are_sorted() {
        assert!(ZOOM_LEVELS.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(ZOOM_LEVELS.contains(&1.0));
    }

    #[test]
    fn moved_tab_follows_group_bounds() {
        // внутрь чужой группы — вступает в неё
        assert_eq!(group_after_move(Some(2), Some(2), 0, false), 2);
        assert_eq!(group_after_move(Some(2), Some(2), 1, true), 2);
        // по-прежнему рядом со своей группой — остаётся
        assert_eq!(group_after_move(Some(1), Some(0), 1, true), 1);
        assert_eq!(group_after_move(None, Some(1), 1, true), 1);
        // унесли от своей группы — выходит из неё
        assert_eq!(group_after_move(Some(0), Some(0), 1, true), 0);
        assert_eq!(group_after_move(Some(0), None, 1, true), 0);
        // группа из одной вкладки переезжает вместе с ней
        assert_eq!(group_after_move(Some(0), Some(0), 1, false), 1);
        // на границе двух разных групп обычная вкладка остаётся без группы
        assert_eq!(group_after_move(Some(1), Some(2), 0, false), 0);
    }

    #[test]
    fn collapsing_group_moves_to_visible_tab() {
        // вкладки: [гр.1, гр.1, гр.2 (свёрнута), без группы]; активна вторая
        let groups = [1, 1, 2, 0];
        assert_eq!(nearest_outside(&groups, &[1, 2], 1, 1), Some(3));
        // справа видимых нет — берём слева
        assert_eq!(nearest_outside(&[0, 1, 2], &[1, 2], 1, 1), Some(0));
        // кроме свёрнутых групп ничего нет — хоть куда-то, но не в сворачиваемую
        assert_eq!(nearest_outside(&[1, 2], &[1, 2], 0, 1), Some(1));
        assert_eq!(nearest_outside(&[1, 1], &[1], 0, 1), None);
    }
}
