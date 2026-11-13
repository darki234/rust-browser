// В release-сборке не открываем консольное окно.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adblock;
mod efficiency;
mod extension;
mod platform;
mod pages;
mod store;
mod system;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
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
    if dark { (18, 19, 24, 255) } else { (255, 255, 255, 255) }
}

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
    /// uBlock Origin Lite распакован в эту папку.
    ExtensionReady(PathBuf),
    /// uBlock Origin Lite установлен в профиль; id нужен для включения и настроек.
    ExtensionInstalled(Option<String>),
    /// Другой запуск браузера передал адреса (пусто — просьба открыть новое окно).
    Remote(Vec<String>),
    /// Прокрутка вкладки в момент ухода в фон.
    Scrolled(u32, f64),
    /// Процессы WebView2; true — показать их в диспетчере вкладок.
    Processes(Vec<efficiency::ProcessInfo>, bool),
    /// Страница не загрузилась: (вкладка, вид ошибки для своей страницы ошибки).
    LoadFailed(u32, &'static str),
}

pub enum DownloadEvent {
    Started { id: u64, url: String, path: String },
    Progress { id: u64, received: i64, total: i64 },
    /// "progress" | "paused" | "done" | "canceled" | "failed"
    State { id: u64, state: &'static str, received: i64 },
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
        }
    }

    fn is_start(&self) -> bool {
        self.webview.is_none() && self.url.is_empty()
    }

    fn snapshot(&self) -> SessionTab {
        SessionTab { url: self.url.clone(), title: self.title.clone(), pinned: self.pinned }
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
    tabs: Vec<Tab>,
    active: u32,
    next_id: u32,
    /// Закрытые вкладки для Ctrl+Shift+T.
    closed: Vec<SessionTab>,
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
    quit: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let inprivate = args.iter().any(|arg| arg == "--inprivate");
    let new_window = args.iter().any(|arg| arg == "--new-window");
    let urls: Vec<String> = args
        .iter()
        .filter(|arg| !arg.starts_with("--"))
        .map(|arg| system::arg_to_address(arg))
        .collect();

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
    let dark = match store.settings.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => window.theme() == WindowTheme::Dark,
    };
    let keys = Arc::new(KeyState::default());

    // Профиль лежит в папке данных, а не рядом с exe: программу можно
    // переносить и запускать откуда угодно. У InPrivate профиль отдельный.
    let profile = if inprivate { "WebView2-InPrivate" } else { "WebView2" };
    let mut web_context = WebContext::new(Some(data_dir().join(profile)));
    let ui = build_ui(&window, proxy.clone(), &mut web_context, keys.clone(), inprivate, dark)?;

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
        tabs: Vec::new(),
        active: 0,
        next_id: 1,
        closed: Vec::new(),
        downloads: Vec::new(),
        keys,
        overlay: Overlay::None,
        overlay_pending: false,
        overlay_requested: Instant::now(),
        chrome_height: DEFAULT_CHROME,
        chrome_side: 0.0,
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
        quit: false,
    };
    app.apply_theme();
    app.load_downloads();
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
    if app.primary && system::is_registered() {
        // exe могли перенести — обновляем путь в регистрации Windows
        std::thread::spawn(system::register);
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + SLEEP_CHECK);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => app.quit = true,
                WindowEvent::Resized(_) => {
                    app.layout();
                    let maximized = app.window.is_maximized();
                    if maximized != app.maximized {
                        app.maximized = maximized;
                        app.dirty = true;
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
        if app.overlay_pending {
            // проснуться вовремя, чтобы открыть меню и без снимка
            *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
        }

        if app.quit {
            app.save_session();
            app.store.save_all();
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// Тёмная ли тема, ещё до создания окна: по настройке, а «как в системе» — по Windows.
fn window_dark_hint() -> bool {
    let settings: store::Settings = Store::load(data_dir()).settings;
    match settings.theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => system::system_prefers_dark(),
    }
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
) -> wry::Result<WebView> {
    let html = include_str!("ui/index.html")
        .replace("/*__CSS__*/", include_str!("ui/style.css"))
        .replace("/*__JS__*/", include_str!("ui/app.js"));
    let engine = wry::webview_version().unwrap_or_default();
    let boot = json!({
        "inprivate": inprivate,
        "version": env!("CARGO_PKG_VERSION"),
        "engine": engine,
    });
    let (width, height) = window_size(window);

    let ipc = proxy.clone();
    let ui = WebViewBuilder::new_with_web_context(web_context)
        .with_bounds(rect(0.0, 0.0, width, height))
        // Своё фото для обоев: интерфейс берёт его по адресу http://rb.localhost/wallpaper.
        .with_custom_protocol("rb".into(), |_, request| {
            let range = request.headers().get("range").and_then(|v| v.to_str().ok());
            serve_local(request.uri().path(), range)
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
        // Интерфейс не должен уходить на сайты, например от перетаскивания ссылки.
        .with_navigation_handler(|url| !url.starts_with("http"))
        .with_ipc_handler(move |request: Request<String>| {
            let _ = ipc.send_event(UserEvent::Ui(request.into_body()));
        })
        .build_as_child(window)?;
    platform::hook_hotkeys(&ui, keys, proxy);
    Ok(ui)
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
        .with_devtools(true)
        .with_theme(theme)
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
        // Ссылки «в новом окне» открываем новой вкладкой.
        .with_new_window_req_handler(move |url, _| {
            let background = platform::background_click();
            let _ = popup.send_event(UserEvent::NewTabRequested(url, background));
            NewWindowResponse::Deny
        })
        .build_as_child(&app.window)?;

    platform::hook_hotkeys(&webview, app.keys.clone(), proxy.clone());
    platform::hook_adblock(&webview, id, app.adblock.clone(), proxy.clone());
    platform::hook_history(&webview, id, proxy.clone());
    platform::hook_page_state(&webview, id, proxy.clone());
    platform::hook_find(&webview, id, proxy.clone());
    platform::hook_load_errors(&webview, id, proxy.clone());
    platform::hook_downloads(&webview, app.download_dir.clone(), proxy);
    platform::set_passwords(&webview, app.store.settings.passwords);
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
    fn page_rect(&self) -> Rect {
        let (width, height) = window_size(&self.window);
        if self.is_fullscreen() {
            rect(0.0, 0.0, width, height)
        } else if self.chrome_side > 0.0 {
            rect(self.chrome_side, 0.0, width - self.chrome_side, height)
        } else {
            rect(0.0, self.chrome_height, width, height - self.chrome_height)
        }
    }

    fn layout(&self) {
        let (width, height) = window_size(&self.window);
        let chrome = if self.is_fullscreen() { 0.0 } else { self.chrome_height };
        let side = if self.is_fullscreen() { 0.0 } else { self.chrome_side };
        let on_start_page = self.active_webview().is_none();

        let ui_rect = match self.overlay {
            _ if on_start_page => rect(0.0, 0.0, width, height),
            Overlay::Full => rect(0.0, 0.0, width, height),
            // боковая колонка и так на всю высоту — подсказки помещаются в ней
            _ if side > 0.0 => rect(0.0, 0.0, side, height),
            Overlay::Sheet(extra) => rect(0.0, 0.0, width, (chrome + extra).min(height)),
            Overlay::None => rect(0.0, 0.0, width, chrome),
        };
        let _ = self.ui.set_bounds(ui_rect);

        // Сначала показываем новую вкладку и только потом прячем остальные: иначе
        // на миг не видно ни одной, и сквозь щель мелькает фон окна.
        if let Some(webview) = self.active_webview() {
            let _ = webview.set_bounds(self.page_rect());
            let _ = webview.set_visible(true);
        }
        for tab in &self.tabs {
            if tab.id == self.active {
                continue;
            }
            if let Some(webview) = &tab.webview {
                let _ = webview.set_visible(false);
            }
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
        match self.store.settings.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => self.window.theme() == WindowTheme::Dark,
        }
    }

    fn page_theme(&self) -> Theme {
        match self.store.settings.theme.as_str() {
            "dark" => Theme::Dark,
            "light" => Theme::Light,
            _ => Theme::Auto,
        }
    }

    fn apply_theme(&mut self) {
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
            self.tabs.push(Tab::new(id, &saved.url, &saved.title, saved.pinned));
        }
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
        if !tab.url.is_empty() {
            self.closed.push(tab.snapshot());
        }
        if let Some(webview) = &tab.webview {
            platform::find_stop(webview);
        }
        drop(tab); // уничтожает webview вкладки
        self.dirty = true;
        self.session_dirty = true;

        if self.tabs.is_empty() {
            self.quit = true;
            return;
        }
        if self.active == id {
            let neighbour = index.min(self.tabs.len() - 1);
            self.activate(self.tabs[neighbour].id);
        }
    }

    fn close_many(&mut self, ids: Vec<u32>) {
        for id in ids {
            self.close_tab(id);
        }
    }

    /// Делает вкладку активной без загрузки и фокуса.
    fn set_active(&mut self, id: u32) {
        let previous = self.active;
        let scroll_proxy = self.proxy.clone();
        if let Some(old) = self.tab_mut(previous) {
            if previous != id {
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
        let tab = self.tabs.remove(from);
        let pinned = self.pinned_count();
        let to = if tab.pinned { to.min(pinned) } else { to.clamp(pinned, self.tabs.len()) };
        self.tabs.insert(to, tab);
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
                }
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
                let Some(tab) = self.tab_mut(id) else { return };
                let Some(webview) = &tab.webview else { return };
                tab.failed = true;
                let _ = webview.load_html(&pages::error_page(&tab.url, kind, dark));
                self.dirty = true;
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
        let backdrop = match bytes {
            Some(bytes) if has_page => format!("'data:image/jpeg;base64,{}'", base64(&bytes)),
            _ => "null".to_string(),
        };
        let _ = self.ui.evaluate_script(&format!("ui.setBackdrop({backdrop})"));
        self.overlay = Overlay::Full;
        self.layout();
        let _ = self.ui.focus();
        let _ = self.ui.evaluate_script("ui.overlayReady()");
    }

    // ---------- настройки ----------

    fn set_setting(&mut self, key: &str, value: &Value) {
        let settings = &mut self.store.settings;
        match key {
            "theme" => {
                if let Some(theme @ ("system" | "light" | "dark")) = value.as_str() {
                    settings.theme = theme.to_string();
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
                let enabled = settings.passwords;
                for webview in self.tabs.iter().filter_map(|t| t.webview.as_ref()) {
                    platform::set_passwords(webview, enabled);
                }
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
            "motion" | "bar_style" | "newtab_layout" => {
                let allowed: &[&str] = match key {
                    "motion" => &["smooth", "fast", "bouncy", "off"],
                    "bar_style" => &["classic", "compact", "floating", "vertical"],
                    _ => &["clock", "minimal", "tiles", "greeting"],
                };
                let Some(choice) = value.as_str().filter(|v| allowed.contains(v)) else { return };
                match key {
                    "motion" => settings.motion = choice.to_string(),
                    "bar_style" => settings.bar_style = choice.to_string(),
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
                Some(name) if WALLPAPERS.contains(&name) => settings.wallpaper = name.to_string(),
                _ => return,
            },
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
            "chrome" => {
                let height = message["height"].as_f64().unwrap_or(DEFAULT_CHROME);
                let side = message["side"].as_f64().unwrap_or(0.0);
                if (height - self.chrome_height).abs() > 0.5 || (side - self.chrome_side).abs() > 0.5 {
                    self.chrome_height = height;
                    self.chrome_side = side;
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
                let list = self.store.suggest(&text("text"), 6);
                let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
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
            "tasks" => self.refresh_processes(true),
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
                    "favicon": tab.favicon,
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
        if self.dirty && self.last_push.elapsed() >= PUSH_INTERVAL {
            self.push_state();
        }
        if self.overlay_pending && self.overlay_requested.elapsed() > Duration::from_millis(600) {
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

    fn sleep_background_tabs(&mut self) {
        let minutes = self.sleep_limit();
        // Выгрузка отменила бы загрузку, поэтому пока что-то качается — только сон.
        let downloading = self.downloads.iter().any(|d| d.state == "progress" || d.state == "paused");
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
                    "favicon": tab.favicon,
                    "loading": tab.loading,
                    "blocked": tab.blocked,
                    "canBack": tab.can_back,
                    "canForward": tab.can_forward,
                    "start": tab.is_start(),
                    "pinned": tab.pinned,
                    "zoom": tab.zoom,
                    "audio": tab.audio,
                    "muted": tab.muted,
                    "sleeping": tab.sleeping || (tab.webview.is_none() && !tab.url.is_empty()),
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
                "motion": settings.motion,
                "barStyle": settings.bar_style,
                "newtabLayout": settings.newtab_layout,
                "wallpaperAlways": settings.wallpaper_always,
                "downloadDir": settings.download_dir,
                "defaultDownloadDir": default_download_dir(),
                "wallpaper": settings.wallpaper,
                "wallpaperDim": settings.wallpaper_dim,
                "wallpaperUrl": format!("http://rb.localhost/wallpaper?v={}", settings.wallpaper_version),
                "hasImage": wallpaper_file().is_some(),
                "hasVideo": video_file().is_some(),
                "videoUrl": format!("http://rb.localhost/wallpaper-video?v={}", settings.wallpaper_version),
            },
            "ubolId": self.ubol_id,
            "isDefault": self.is_default,
            "onBattery": self.on_battery,
            "importSources": self.import_sources,
            "dark": self.dark(),
            "maximized": self.maximized,
            "fullscreen": self.is_fullscreen(),
        });
        let mut state = state;
        if let Value::Object(fields) = &mut state {
            for key in ["bookmarks", "downloads", "engines", "settings"] {
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

/// Выгружает вкладку: webview и процесс страницы уничтожаются, адрес остаётся;
/// при открытии страница загрузится заново и вернётся к прежней прокрутке.
fn unload(tab: &mut Tab) {
    tab.webview = None;
    tab.restore_scroll = tab.scroll;
    tab.sleeping = true;
    tab.loading = false;
}

#[derive(Debug, PartialEq)]
enum Rest {
    Keep,
    Suspend,
    Discard,
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
fn serve_video(range: Option<&str>) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    use std::io::{Read, Seek, SeekFrom};
    let not_found = || {
        wry::http::Response::builder().status(404).body(Vec::new().into()).unwrap_or_default()
    };
    let Some((path, mime)) = video_file() else { return not_found() };
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
        return serve_video(range);
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
    fn accent_colors_are_validated() {
        assert!(is_hex_color("#4f5bf5"));
        assert!(is_hex_color("#ABCDEF"));
        assert!(!is_hex_color("4f5bf5"));
        assert!(!is_hex_color("#4f5bf"));
        assert!(!is_hex_color("#4f5bfz"));
        assert!(!is_hex_color("red;}body{"));
    }

    #[test]
    fn base64_matches_reference() {
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
}
