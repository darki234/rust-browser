//! Всё, чего нет в wry и что берётся напрямую из WebView2 и Win32:
//! горячие клавиши, перехват запросов, загрузки, поиск по странице, масштаб,
//! усыпление вкладок, снимок страницы, оформление окна и выбор папки.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::c_void,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use tao::event_loop::EventLoopProxy;
use webview2_com::{
    take_pwstr, AcceleratorKeyPressedEventHandler, BytesReceivedChangedEventHandler,
    BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler,
    CapturePreviewCompletedHandler, ContainsFullScreenElementChangedEventHandler,
    DownloadStartingEventHandler, FaviconChangedEventHandler,
    FindActiveMatchIndexChangedEventHandler, FindMatchCountChangedEventHandler,
    FindStartCompletedHandler, HistoryChangedEventHandler,
    IsDocumentPlayingAudioChangedEventHandler, Microsoft::Web::WebView2::Win32::*,
    NavigationCompletedEventHandler,
    ProfileAddBrowserExtensionCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler,
    StateChangedEventHandler, TrySuspendCompletedHandler, WebResourceRequestedEventHandler,
    ZoomFactorChangedEventHandler,
};
use windows::{
    core::{Interface, BOOL, HSTRING, PCWSTR, PWSTR},
    Win32::{
        Foundation::{HANDLE, HGLOBAL, HWND},
        Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
            DWMWCP_ROUND,
        },
        System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
        System::Memory::{GlobalLock, GlobalUnlock},
        System::Ole::CF_UNICODETEXT,
        System::Com::{
            CoCreateInstance, CoTaskMemFree, IStream, StructuredStorage::CreateStreamOnHGlobal,
            CLSCTX_INPROC_SERVER, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET,
        },
        UI::{
            Input::KeyboardAndMouse::{
                keybd_event, GetAsyncKeyState, GetKeyState, VK_MBUTTON, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_CONTROL, VK_LWIN,
                VK_MENU, VK_OEM_PERIOD, VK_SHIFT,
            },
            Shell::{
                Common::COMDLG_FILTERSPEC, FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST,
                FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
            },
            WindowsAndMessaging::{
                SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            },
        },
    },
};
use wry::{MemoryUsageLevel, WebView, WebViewExtWindows};

use crate::{adblock, DownloadEvent, UserEvent};

type Proxy = EventLoopProxy<UserEvent>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hotkey {
    NewTab,
    CloseTab,
    ReopenTab,
    InPrivate,
    NewWindow,
    FocusUrl,
    NextTab,
    PrevTab,
    /// 1–8 — вкладка по номеру, 9 — последняя.
    SelectTab(u8),
    Reload,
    Back,
    Forward,
    Bookmark,
    Bookmarks,
    History,
    Downloads,
    Settings,
    /// Ctrl+Shift+L — вставить адрес или запрос из буфера обмена и сразу перейти.
    PasteGo,
    /// Shift+Esc — диспетчер вкладок, как в Edge.
    TaskManager,
    Find,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Print,
    DevTools,
    FavBar,
    Fullscreen,
    Menu,
    /// Перехватывается только в полноэкранном режиме или при открытом поиске.
    Escape,
}

impl Hotkey {
    /// Сопоставляет код виртуальной клавиши и модификаторы с действием.
    pub fn from_keys(key: u32, ctrl: bool, shift: bool, alt: bool) -> Option<Self> {
        const TAB: u32 = 0x09;
        const ESC: u32 = 0x1B;
        const PAGE_UP: u32 = 0x21;
        const PAGE_DOWN: u32 = 0x22;
        const LEFT: u32 = 0x25;
        const RIGHT: u32 = 0x27;
        const DELETE: u32 = 0x2E;
        const NUM0: u32 = 0x60;
        const NUM_PLUS: u32 = 0x6B;
        const NUM_MINUS: u32 = 0x6D;
        const F5: u32 = 0x74;
        const F6: u32 = 0x75;
        const F11: u32 = 0x7A;
        const F12: u32 = 0x7B;
        const PLUS: u32 = 0xBB; // клавиша «=/+»
        const MINUS: u32 = 0xBD;

        let letter = |c: char| key == c as u32;
        Some(match (ctrl, shift, alt) {
            (false, false, false) if key == ESC => Self::Escape,
            (false, true, false) if key == ESC => Self::TaskManager,
            (false, false, false) if key == F5 => Self::Reload,
            (false, false, false) if key == F6 => Self::FocusUrl,
            (false, false, false) if key == F11 => Self::Fullscreen,
            (false, false, false) if key == F12 => Self::DevTools,
            (false, false, true) if key == LEFT => Self::Back,
            (false, false, true) if key == RIGHT => Self::Forward,
            (false, false, true) if letter('D') => Self::FocusUrl,
            (false, false, true) if letter('F') => Self::Menu,
            (true, false, false) if letter('T') => Self::NewTab,
            (true, false, false) if letter('N') => Self::NewWindow,
            (true, false, false) if letter('W') => Self::CloseTab,
            (true, false, false) if letter('L') => Self::FocusUrl,
            (true, false, false) if letter('R') => Self::Reload,
            (true, false, false) if letter('D') => Self::Bookmark,
            (true, false, false) if letter('H') => Self::History,
            (true, false, false) if letter('J') => Self::Downloads,
            (true, false, false) if letter('F') => Self::Find,
            (true, false, false) if letter('P') => Self::Print,
            (true, false, false) if key == PLUS || key == NUM_PLUS => Self::ZoomIn,
            (true, false, false) if key == MINUS || key == NUM_MINUS => Self::ZoomOut,
            (true, false, false) if key == 0x30 || key == NUM0 => Self::ZoomReset,
            (true, false, false) if key == TAB || key == PAGE_DOWN => Self::NextTab,
            (true, false, false) if key == PAGE_UP => Self::PrevTab,
            (true, false, false) if (0x31..=0x39).contains(&key) => {
                Self::SelectTab((key - 0x30) as u8)
            }
            (true, true, false) if letter('T') => Self::ReopenTab,
            (true, true, false) if letter('N') => Self::InPrivate,
            (true, true, false) if letter('O') => Self::Bookmarks,
            (true, true, false) if letter('B') => Self::FavBar,
            (true, true, false) if letter('L') => Self::PasteGo,
            (true, true, false) if letter('I') => Self::DevTools,
            (true, true, false) if key == DELETE => Self::Settings,
            (true, true, false) if key == TAB => Self::PrevTab,
            _ => return None,
        })
    }
}

/// Состояние, от которого зависит, перехватывать ли Esc.
#[derive(Default)]
pub struct KeyState {
    pub fullscreen: AtomicBool,
    pub find_open: AtomicBool,
}

/// Поднимает webview над соседними: новые дочерние окна wry создаёт поверх всех.
pub fn raise(webview: &WebView) {
    unsafe {
        let mut hwnd = HWND::default();
        if webview.controller().ParentWindow(&mut hwnd).is_ok() {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}

/// Ловит сочетания клавиш, где бы ни был фокус: в панели или на странице.
pub fn hook_hotkeys(webview: &WebView, keys: Arc<KeyState>, proxy: Proxy) {
    let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        unsafe {
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
            args.KeyEventKind(&mut kind)?;
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
            {
                return Ok(());
            }

            let mut key = 0u32;
            args.VirtualKey(&mut key)?;
            let down = |vk: u16| GetKeyState(vk as i32) < 0;
            let Some(hotkey) =
                Hotkey::from_keys(key, down(VK_CONTROL.0), down(VK_SHIFT.0), down(VK_MENU.0))
            else {
                return Ok(());
            };
            // Esc нужен сайтам, забираем его только когда он закрывает наше.
            if hotkey == Hotkey::Escape
                && !keys.fullscreen.load(Ordering::Relaxed)
                && !keys.find_open.load(Ordering::Relaxed)
            {
                return Ok(());
            }

            args.SetHandled(true)?;
            // Удержание клавиши не должно, например, открывать десятки вкладок.
            // Масштаб — исключение: его удобно менять зажатой клавишей.
            let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
            args.PhysicalKeyStatus(&mut status)?;
            let repeatable = matches!(hotkey, Hotkey::ZoomIn | Hotkey::ZoomOut);
            if repeatable || !status.WasKeyDown.as_bool() {
                let _ = proxy.send_event(UserEvent::Hotkey(hotkey));
            }
        }
        Ok(())
    }));
    unsafe {
        let _ = webview.controller().add_AcceleratorKeyPressed(&handler, &mut 0i64);
    }
}

/// Отвечает 403 на запросы к доменам из списка, пока блокировка включена.
pub fn hook_adblock(webview: &WebView, tab: u32, enabled: Arc<AtomicBool>, proxy: Proxy) {
    let core = webview.webview();
    let environment = webview.environment();
    let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        if !enabled.load(Ordering::Relaxed) {
            return Ok(());
        }
        unsafe {
            let mut uri = PWSTR::null();
            args.Request()?.Uri(&mut uri)?;
            let uri = take_pwstr(uri);
            if adblock::is_blocked(&uri) {
                let response = environment.CreateWebResourceResponse(
                    None::<&IStream>,
                    403,
                    &HSTRING::from("Blocked"),
                    &HSTRING::new(),
                )?;
                args.SetResponse(&response)?;
                let _ = proxy.send_event(UserEvent::Blocked(tab));
            }
        }
        Ok(())
    }));
    unsafe {
        let _ = core.AddWebResourceRequestedFilter(
            &HSTRING::from("*"),
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        );
        let _ = core.add_WebResourceRequested(&handler, &mut 0i64);
    }
}

/// Сообщает о любом изменении истории вкладки, включая переходы внутри SPA.
pub fn hook_history(webview: &WebView, tab: u32, proxy: Proxy) {
    let handler = HistoryChangedEventHandler::create(Box::new(move |_, _| {
        let _ = proxy.send_event(UserEvent::HistoryChanged(tab));
        Ok(())
    }));
    unsafe {
        let _ = webview.webview().add_HistoryChanged(&handler, &mut 0i64);
    }
}

/// Значок сайта, звук на странице, полноэкранное видео и масштаб колёсиком.
pub fn hook_page_state(webview: &WebView, tab: u32, proxy: Proxy) {
    let core = webview.webview();
    unsafe {
        if let Ok(core15) = core.cast::<ICoreWebView2_15>() {
            let favicon = proxy.clone();
            let handler = FaviconChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(sender) = sender else { return Ok(()) };
                let mut uri = PWSTR::null();
                sender.cast::<ICoreWebView2_15>()?.FaviconUri(&mut uri)?;
                let _ = favicon.send_event(UserEvent::Favicon(tab, take_pwstr(uri)));
                Ok(())
            }));
            let _ = core15.add_FaviconChanged(&handler, &mut 0i64);
        }

        if let Ok(core8) = core.cast::<ICoreWebView2_8>() {
            let audio = proxy.clone();
            let handler =
                IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |sender, _| {
                    let Some(sender) = sender else { return Ok(()) };
                    let mut playing = BOOL::default();
                    sender.cast::<ICoreWebView2_8>()?.IsDocumentPlayingAudio(&mut playing)?;
                    let _ = audio.send_event(UserEvent::Audio(tab, playing.as_bool()));
                    Ok(())
                }));
            let _ = core8.add_IsDocumentPlayingAudioChanged(&handler, &mut 0i64);
        }

        let fullscreen = proxy.clone();
        let handler =
            ContainsFullScreenElementChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(sender) = sender else { return Ok(()) };
                let mut full = BOOL::default();
                sender.ContainsFullScreenElement(&mut full)?;
                let _ = fullscreen.send_event(UserEvent::PageFullscreen(tab, full.as_bool()));
                Ok(())
            }));
        let _ = core.add_ContainsFullScreenElementChanged(&handler, &mut 0i64);

        let zoom = proxy.clone();
        let handler = ZoomFactorChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(sender) = sender else { return Ok(()) };
            let mut factor = 1.0;
            sender.ZoomFactor(&mut factor)?;
            let _ = zoom.send_event(UserEvent::Zoom(tab, factor));
            Ok(())
        }));
        let _ = webview.controller().add_ZoomFactorChanged(&handler, &mut 0i64);
    }
}

/// Вид ошибки загрузки для своей страницы ошибки; None — показывать её не нужно
/// (человек сам остановил загрузку, сайт просит пароль и т. п.).
pub fn error_kind(status: COREWEBVIEW2_WEB_ERROR_STATUS) -> Option<&'static str> {
    Some(match status {
        COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED => "dns",
        COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT => "timeout",
        COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED => "offline",
        COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT
        | COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET
        | COREWEBVIEW2_WEB_ERROR_STATUS_SERVER_UNREACHABLE
        | COREWEBVIEW2_WEB_ERROR_STATUS_ERROR_HTTP_INVALID_SERVER_RESPONSE
        | COREWEBVIEW2_WEB_ERROR_STATUS_REDIRECT_FAILED => "connection",
        COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_COMMON_NAME_IS_INCORRECT
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_REVOKED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CLIENT_CERTIFICATE_CONTAINS_ERRORS => "certificate",
        COREWEBVIEW2_WEB_ERROR_STATUS_UNEXPECTED_ERROR => "other",
        _ => return None,
    })
}

/// Сообщает о загрузках страницы, которые закончились ошибкой.
pub fn hook_load_errors(webview: &WebView, tab: u32, proxy: Proxy) {
    let handler = NavigationCompletedEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        unsafe {
            let mut success = BOOL::default();
            args.IsSuccess(&mut success)?;
            if success.as_bool() {
                return Ok(());
            }
            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
            args.WebErrorStatus(&mut status)?;
            if let Some(kind) = error_kind(status) {
                let _ = proxy.send_event(UserEvent::LoadFailed(tab, kind));
            }
        }
        Ok(())
    }));
    unsafe {
        let _ = webview.webview().add_NavigationCompleted(&handler, &mut 0i64);
    }
}

pub fn set_muted(webview: &WebView, muted: bool) {
    unsafe {
        if let Ok(core8) = webview.webview().cast::<ICoreWebView2_8>() {
            let _ = core8.SetIsMuted(muted);
        }
    }
}

/// Сохранение паролей и автозаполнение форм — настройка общего профиля.
pub fn set_passwords(webview: &WebView, enabled: bool) {
    unsafe {
        let profile = webview
            .webview()
            .cast::<ICoreWebView2_13>()
            .and_then(|core| core.Profile())
            .and_then(|profile| profile.cast::<ICoreWebView2Profile6>());
        if let Ok(profile) = profile {
            let _ = profile.SetIsPasswordAutosaveEnabled(enabled);
            let _ = profile.SetIsGeneralAutofillEnabled(true);
        }
    }
}

// ---------- расширения ----------

fn profile7(webview: &WebView) -> Option<ICoreWebView2Profile7> {
    unsafe {
        webview
            .webview()
            .cast::<ICoreWebView2_13>()
            .and_then(|core| core.Profile())
            .and_then(|profile| profile.cast::<ICoreWebView2Profile7>())
            .ok()
    }
}

unsafe fn extension_text(
    get: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>,
) -> Option<String> {
    let mut text = PWSTR::null();
    get(&mut text).ok()?;
    Some(take_pwstr(text))
}

/// Ставит распакованное расширение в профиль, если его там ещё нет, и сообщает его id.
/// `reinstall` — сначала убрать прежнюю версию (её узнаём по части имени).
pub fn install_extension(
    webview: &WebView,
    folder: &Path,
    name_part: &'static str,
    reinstall: bool,
    done: impl FnOnce(Option<String>) + 'static,
) {
    let Some(profile) = profile7(webview) else { return done(None) };
    let folder = HSTRING::from(folder.as_os_str());
    let adder = profile.clone();
    let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |result, list| unsafe {
        let mut existing = Vec::new();
        if let (Ok(()), Some(list)) = (result, list) {
            let mut count = 0u32;
            list.Count(&mut count)?;
            for index in 0..count {
                let ext = list.GetValueAtIndex(index)?;
                if extension_text(|p| ext.Name(p)).is_some_and(|name| name.contains(name_part)) {
                    existing.push(ext);
                }
            }
        }
        if !reinstall {
            if let Some(ext) = existing.first() {
                done(extension_text(|p| ext.Id(p)));
                return Ok(());
            }
        }
        for ext in &existing {
            let _ = ext.Remove(&BrowserExtensionRemoveCompletedHandler::create(Box::new(|_| Ok(()))));
        }
        let added = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |result, ext| {
            let id = result.ok().and(ext).and_then(|ext| extension_text(|p| ext.Id(p)));
            done(id);
            Ok(())
        }));
        adder.AddBrowserExtension(&folder, &added)?;
        Ok(())
    }));
    unsafe {
        let _ = profile.GetBrowserExtensions(&handler);
    }
}

/// Включает или выключает расширение с этим id.
pub fn enable_extension(webview: &WebView, id: &str, enabled: bool) {
    let Some(profile) = profile7(webview) else { return };
    let id = id.to_string();
    let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |result, list| unsafe {
        let (Ok(()), Some(list)) = (result, list) else { return Ok(()) };
        let mut count = 0u32;
        list.Count(&mut count)?;
        for index in 0..count {
            let ext = list.GetValueAtIndex(index)?;
            if extension_text(|p| ext.Id(p)).as_deref() == Some(id.as_str()) {
                let done = BrowserExtensionEnableCompletedHandler::create(Box::new(|_| Ok(())));
                ext.Enable(enabled, &done)?;
            }
        }
        Ok(())
    }));
    unsafe {
        let _ = profile.GetBrowserExtensions(&handler);
    }
}

// ---------- поиск по странице ----------

/// Подписывается на число совпадений и текущее совпадение.
pub fn hook_find(webview: &WebView, tab: u32, proxy: Proxy) {
    unsafe {
        let Ok(find) = webview.webview().cast::<ICoreWebView2_28>().and_then(|c| c.Find()) else {
            return;
        };
        let report = move |find: &ICoreWebView2Find, proxy: &Proxy| {
            let (mut index, mut count) = (-1i32, 0i32);
            let _ = find.ActiveMatchIndex(&mut index);
            let _ = find.MatchCount(&mut count);
            let _ = proxy.send_event(UserEvent::Find(tab, index, count));
        };

        let counted = proxy.clone();
        let handler = FindMatchCountChangedEventHandler::create(Box::new(move |sender, _| {
            if let Some(find) = sender {
                report(&find, &counted);
            }
            Ok(())
        }));
        let _ = find.add_MatchCountChanged(&handler, &mut 0i64);

        let moved = proxy.clone();
        let handler = FindActiveMatchIndexChangedEventHandler::create(Box::new(move |sender, _| {
            if let Some(find) = sender {
                report(&find, &moved);
            }
            Ok(())
        }));
        let _ = find.add_ActiveMatchIndexChanged(&handler, &mut 0i64);
    }
}

pub fn find_start(webview: &WebView, term: &str) {
    unsafe {
        let Ok(find) = webview.webview().cast::<ICoreWebView2_28>().and_then(|c| c.Find()) else {
            return;
        };
        let Ok(env) = webview.environment().cast::<ICoreWebView2Environment15>() else {
            return;
        };
        let Ok(options) = env.CreateFindOptions() else { return };
        let _ = options.SetFindTerm(&HSTRING::from(term));
        let _ = options.SetSuppressDefaultFindDialog(true);
        let _ = options.SetShouldHighlightAllMatches(true);
        let done = FindStartCompletedHandler::create(Box::new(|_| Ok(())));
        let _ = find.Start(&options, &done);
    }
}

pub fn find_step(webview: &WebView, forward: bool) {
    unsafe {
        if let Ok(find) = webview.webview().cast::<ICoreWebView2_28>().and_then(|c| c.Find()) {
            let _ = if forward { find.FindNext() } else { find.FindPrevious() };
        }
    }
}

pub fn find_stop(webview: &WebView) {
    unsafe {
        if let Ok(find) = webview.webview().cast::<ICoreWebView2_28>().and_then(|c| c.Find()) {
            let _ = find.Stop();
        }
    }
}

// ---------- загрузки ----------

thread_local! {
    /// Идущие загрузки: нужны, чтобы ставить их на паузу и отменять.
    static DOWNLOADS: RefCell<HashMap<u64, ICoreWebView2DownloadOperation>> = RefCell::new(HashMap::new());
}

static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

/// Номера новых загрузок продолжают сохранённый список, чтобы не совпасть с ним.
pub fn continue_download_ids(after: u64) {
    NEXT_DOWNLOAD_ID.fetch_max(after + 1, Ordering::Relaxed);
}

/// "pause" | "resume" | "cancel" для идущей загрузки.
pub fn control_download(id: u64, action: &str) -> bool {
    DOWNLOADS.with(|map| {
        let map = map.borrow();
        let Some(operation) = map.get(&id) else { return false };
        unsafe {
            match action {
                "pause" => operation.Pause().is_ok(),
                "resume" => operation.Resume().is_ok(),
                "cancel" => operation.Cancel().is_ok(),
                _ => false,
            }
        }
    })
}

/// Ссылку открыли с Ctrl или колёсиком — такая вкладка открывается в фоне, как в Edge.
pub fn background_click() -> bool {
    unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 || GetAsyncKeyState(VK_MBUTTON.0 as i32) < 0 }
}

/// Следит за загрузками: начало, прогресс и завершение.
/// `folder` — папка из настроек; пустая строка оставляет выбор WebView2.
pub fn hook_downloads(webview: &WebView, folder: Arc<Mutex<String>>, proxy: Proxy) {
    let Ok(core) = webview.webview().cast::<ICoreWebView2_4>() else {
        return;
    };
    let handler = DownloadStartingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let id = NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed);
        unsafe {
            let operation = args.DownloadOperation()?;
            DOWNLOADS.with(|map| map.borrow_mut().insert(id, operation.clone()));

            let mut uri = PWSTR::null();
            operation.Uri(&mut uri)?;
            let mut path = PWSTR::null();
            args.ResultFilePath(&mut path)?;
            let mut path = PathBuf::from(take_pwstr(path));

            let folder = folder.lock().map(|f| f.clone()).unwrap_or_default();
            if !folder.is_empty() {
                if let Some(name) = path.file_name() {
                    path = unique_path(&Path::new(&folder).join(name));
                    args.SetResultFilePath(&HSTRING::from(path.as_os_str()))?;
                }
            }

            // Свой список загрузок вместо встроенного всплывающего окна.
            args.SetHandled(true)?;
            let _ = proxy.send_event(UserEvent::Download(DownloadEvent::Started {
                id,
                url: take_pwstr(uri),
                path: path.to_string_lossy().into_owned(),
            }));

            let progress = proxy.clone();
            let last_report = Cell::new(Instant::now());
            operation.add_BytesReceivedChanged(
                &BytesReceivedChangedEventHandler::create(Box::new(move |operation, _| {
                    let Some(operation) = operation else { return Ok(()) };
                    if last_report.get().elapsed() < Duration::from_millis(150) {
                        return Ok(());
                    }
                    last_report.set(Instant::now());
                    let (mut received, mut total) = (0i64, 0i64);
                    operation.BytesReceived(&mut received)?;
                    operation.TotalBytesToReceive(&mut total)?;
                    let _ = progress.send_event(UserEvent::Download(DownloadEvent::Progress {
                        id,
                        received,
                        total,
                    }));
                    Ok(())
                })),
                &mut 0i64,
            )?;

            let finished = proxy.clone();
            operation.add_StateChanged(
                &StateChangedEventHandler::create(Box::new(move |operation, _| {
                    let Some(operation) = operation else { return Ok(()) };
                    let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
                    operation.State(&mut state)?;
                    let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
                    let _ = operation.InterruptReason(&mut reason);
                    let mut received = 0i64;
                    let _ = operation.BytesReceived(&mut received);
                    let status = match state {
                        COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS => "progress",
                        COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED => "done",
                        _ if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_PAUSED => "paused",
                        _ if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED => "canceled",
                        _ => "failed",
                    };
                    if matches!(status, "done" | "canceled" | "failed") {
                        DOWNLOADS.with(|map| map.borrow_mut().remove(&id));
                    }
                    let _ = finished.send_event(UserEvent::Download(DownloadEvent::State {
                        id,
                        state: status,
                        received,
                    }));
                    Ok(())
                })),
                &mut 0i64,
            )?;
        }
        Ok(())
    }));
    unsafe {
        let _ = core.add_DownloadStarting(&handler, &mut 0i64);
    }
}

/// Добавляет « (1)», « (2)»… к имени, если такой файл уже есть.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new("."));
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|candidate| !candidate.exists())
        .expect("бесконечный перебор всегда находит свободное имя")
}

// ---------- навигация, сон, снимок ----------

/// (можно назад, можно вперёд)
pub fn nav_state(webview: &WebView) -> (bool, bool) {
    let core = webview.webview();
    let (mut back, mut forward) = (BOOL::default(), BOOL::default());
    unsafe {
        let _ = core.CanGoBack(&mut back);
        let _ = core.CanGoForward(&mut forward);
    }
    (back.as_bool(), forward.as_bool())
}

pub fn go_back(webview: &WebView) {
    unsafe {
        let _ = webview.webview().GoBack();
    }
}

pub fn go_forward(webview: &WebView) {
    unsafe {
        let _ = webview.webview().GoForward();
    }
}

pub fn stop(webview: &WebView) {
    unsafe {
        let _ = webview.webview().Stop();
    }
}

/// Фоновая вкладка: меньше памяти. Видимая: обычный режим.
pub fn set_background(webview: &WebView, background: bool) {
    let level = if background { MemoryUsageLevel::Low } else { MemoryUsageLevel::Normal };
    let _ = webview.set_memory_usage_level(level);
}

/// Усыпляет скрытую вкладку: скрипты и таймеры останавливаются, память освобождается.
/// Просыпается она сама, когда снова становится видимой.
pub fn suspend(webview: &WebView) {
    unsafe {
        if let Ok(core3) = webview.webview().cast::<ICoreWebView2_3>() {
            let done = TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(())));
            let _ = core3.TrySuspend(&done);
        }
    }
}

/// Делает JPEG-снимок страницы и передаёт байты в `done` (None — не вышло).
pub fn capture(webview: &WebView, done: impl FnOnce(Option<Vec<u8>>) + 'static) {
    unsafe {
        let Ok(stream) = CreateStreamOnHGlobal(HGLOBAL::default(), true) else {
            return done(None);
        };
        let reader = stream.clone();
        let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
            done(result.ok().and_then(|_| read_stream(&reader)));
            Ok(())
        }));
        // Если вызов не принят, обработчик не сработает и снимок просто не появится.
        let _ = webview.webview().CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_JPEG,
            &stream,
            &handler,
        );
    }
}

unsafe fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    let mut stat = STATSTG::default();
    stream.Stat(&mut stat, STATFLAG_NONAME).ok()?;
    let size = usize::try_from(stat.cbSize).ok()?;
    stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
    let mut bytes = vec![0u8; size];
    let mut read = 0u32;
    stream.Read(bytes.as_mut_ptr() as *mut c_void, size as u32, Some(&mut read)).ok().ok()?;
    bytes.truncate(read as usize);
    Some(bytes)
}

// ---------- окно ----------

/// Скруглённые углы окна и тёмная или светлая системная рамка.
pub fn style_window(hwnd: isize, dark: bool) {
    let hwnd = HWND(hwnd as *mut c_void);
    unsafe {
        let set = |attribute, value: i32| {
            let _ = DwmSetWindowAttribute(
                hwnd,
                attribute,
                &value as *const i32 as *const c_void,
                std::mem::size_of::<i32>() as u32,
            );
        };
        set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND.0);
        set(DWMWA_USE_IMMERSIVE_DARK_MODE, dark as i32);
    }
}

/// Стандартный диалог выбора папки. None — пользователь отменил выбор.
pub fn pick_folder(owner: isize) -> Option<String> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        dialog.Show(Some(HWND(owner as *mut c_void))).ok()?;
        let item = dialog.GetResult().ok()?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const c_void));
        text
    }
}

/// Стандартный диалог выбора изображения. None — пользователь отменил выбор.
pub fn pick_image(owner: isize) -> Option<String> {
    pick_file(owner, "Изображения", "*.jpg;*.jpeg;*.png;*.webp;*.gif;*.bmp")
}

/// Стандартный диалог выбора файла с одним фильтром ("Видео", "*.mp4;*.webm").
pub fn pick_file(owner: isize, kind: &str, pattern: &str) -> Option<String> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let name = HSTRING::from(kind);
        let spec = HSTRING::from(pattern);
        let filter = COMDLG_FILTERSPEC { pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(spec.as_ptr()) };
        dialog.SetFileTypes(&[filter]).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST).ok()?;
        dialog.Show(Some(HWND(owner as *mut c_void))).ok()?;
        let item = dialog.GetResult().ok()?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const c_void));
        text
    }
}

/// Текст из буфера обмена Windows.
pub fn clipboard_text() -> Option<String> {
    unsafe {
        OpenClipboard(None).ok()?;
        let text = (|| {
            let handle: HANDLE = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            let memory = HGLOBAL(handle.0);
            let ptr = GlobalLock(memory) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(memory);
            Some(text)
        })();
        let _ = CloseClipboard();
        text
    }
}

/// Системные панели Windows: эмодзи (Win+.) и голосовой ввод (Win+H).
/// Их нельзя открыть напрямую, поэтому нажимаем то же сочетание, что и человек.
pub fn press_system_keys(panel: &str) {
    let key = match panel {
        "emoji" => VK_OEM_PERIOD.0 as u8,
        "voice" => b'H',
        _ => return,
    };
    let win = VK_LWIN.0 as u8;
    unsafe {
        keybd_event(win, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(key, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(key, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(win, 0, KEYEVENTF_KEYUP, 0);
    }
}

/// Открывает окно InPrivate отдельным процессом.
pub fn open_inprivate() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg("--inprivate").spawn();
    }
}

/// Открывает файл программой по умолчанию.
pub fn open_path(path: &str) {
    let _ = Command::new("explorer").arg(path).spawn();
}

/// Показывает файл в Проводнике.
pub fn reveal_path(path: &str) {
    let _ = Command::new("explorer").raw_arg(format!("/select,\"{path}\"")).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_hotkeys() {
        let key = |c: char| c as u32;
        assert_eq!(Hotkey::from_keys(key('T'), true, false, false), Some(Hotkey::NewTab));
        assert_eq!(Hotkey::from_keys(key('T'), true, true, false), Some(Hotkey::ReopenTab));
        assert_eq!(Hotkey::from_keys(key('N'), true, true, false), Some(Hotkey::InPrivate));
        assert_eq!(Hotkey::from_keys(key('N'), true, false, false), Some(Hotkey::NewWindow));
        assert_eq!(Hotkey::from_keys(key('W'), true, false, false), Some(Hotkey::CloseTab));
        assert_eq!(Hotkey::from_keys(key('D'), true, false, false), Some(Hotkey::Bookmark));
        assert_eq!(Hotkey::from_keys(key('D'), false, false, true), Some(Hotkey::FocusUrl));
        assert_eq!(Hotkey::from_keys(key('F'), true, false, false), Some(Hotkey::Find));
        assert_eq!(Hotkey::from_keys(key('F'), false, false, true), Some(Hotkey::Menu));
        assert_eq!(Hotkey::from_keys(key('L'), true, true, false), Some(Hotkey::PasteGo));
        assert_eq!(Hotkey::from_keys(0x1B, false, true, false), Some(Hotkey::TaskManager));
        assert_eq!(Hotkey::from_keys(0x09, true, false, false), Some(Hotkey::NextTab));
        assert_eq!(Hotkey::from_keys(0x09, true, true, false), Some(Hotkey::PrevTab));
        assert_eq!(Hotkey::from_keys(0x25, false, false, true), Some(Hotkey::Back));
        assert_eq!(Hotkey::from_keys(0x74, false, false, false), Some(Hotkey::Reload));
        assert_eq!(Hotkey::from_keys(0x7A, false, false, false), Some(Hotkey::Fullscreen));
        assert_eq!(Hotkey::from_keys(0x7B, false, false, false), Some(Hotkey::DevTools));
        assert_eq!(Hotkey::from_keys(0x33, true, false, false), Some(Hotkey::SelectTab(3)));
    }

    #[test]
    fn zoom_keys_on_both_keyboards() {
        assert_eq!(Hotkey::from_keys(0xBB, true, false, false), Some(Hotkey::ZoomIn));
        assert_eq!(Hotkey::from_keys(0x6B, true, false, false), Some(Hotkey::ZoomIn));
        assert_eq!(Hotkey::from_keys(0xBD, true, false, false), Some(Hotkey::ZoomOut));
        assert_eq!(Hotkey::from_keys(0x6D, true, false, false), Some(Hotkey::ZoomOut));
        assert_eq!(Hotkey::from_keys(0x30, true, false, false), Some(Hotkey::ZoomReset));
        assert_eq!(Hotkey::from_keys(0x60, true, false, false), Some(Hotkey::ZoomReset));
    }

    #[test]
    fn leaves_other_keys_to_the_page() {
        let key = |c: char| c as u32;
        assert_eq!(Hotkey::from_keys(key('T'), false, false, false), None);
        assert_eq!(Hotkey::from_keys(key('C'), true, false, false), None);
        assert_eq!(Hotkey::from_keys(key('V'), true, false, false), None);
        assert_eq!(Hotkey::from_keys(key('T'), true, false, true), None); // AltGr
    }

    #[test]
    fn stopping_a_page_is_not_an_error() {
        assert_eq!(error_kind(COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED), None);
        assert_eq!(error_kind(COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED), Some("dns"));
        assert_eq!(error_kind(COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET), Some("connection"));
    }

    #[test]
    fn unique_path_adds_number() {
        let dir = std::env::temp_dir().join(format!("rb-unique-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("report.pdf");
        assert_eq!(unique_path(&file), file);
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(unique_path(&file), dir.join("report (1).pdf"));
        std::fs::write(dir.join("report (1).pdf"), b"x").unwrap();
        assert_eq!(unique_path(&file), dir.join("report (2).pdf"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
