//! Связь с Windows: регистрация браузером по умолчанию, один экземпляр программы
//! (ссылки из других программ приходят в уже открытое окно) и новые окна.

use std::{
    ffi::c_void,
    fs::OpenOptions,
    io::{Read, Write},
    os::windows::io::FromRawHandle,
    path::Path,
    process::Command,
    time::Duration,
};

use windows::{
    core::{HSTRING, PCWSTR},
    Win32::{
        Foundation::ERROR_SUCCESS,
        Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND},
        System::{
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
                PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
            },
            Registry::{
                RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW, HKEY,
                HKEY_CURRENT_USER, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_DWORD,
                RRF_RT_REG_SZ,
            },
        },
        UI::{
            Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST},
            WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY},
        },
    },
};

const APP_KEY: &str = r"Software\Clients\StartMenuInternet\RustBrowser";
const URL_PROGID: &str = "RustBrowserURL";
const HTML_PROGID: &str = "RustBrowserHTML";
const DISPLAY_NAME: &str = "Rust Browser";

// ---------- один экземпляр ----------

/// Имя канала зависит от папки данных: у тестовой копии свой канал.
pub fn pipe_name(data_dir: &Path) -> String {
    // FNV-1a: короткий устойчивый хэш без лишних зависимостей
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data_dir.to_string_lossy().to_lowercase().bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!(r"\\.\pipe\RustBrowser-{hash:016x}")
}

/// Передаёт адреса уже запущенному браузеру. true — он их принял, этому процессу
/// можно завершаться. Пустой список — просьба открыть новое окно.
pub fn forward_to_running(pipe: &str, urls: &[String]) -> bool {
    for _ in 0..10 {
        match OpenOptions::new().write(true).open(pipe) {
            Ok(mut channel) => {
                // разрешаем основному процессу вывести своё окно на передний план
                unsafe {
                    let _ = AllowSetForegroundWindow(ASFW_ANY);
                }
                return channel.write_all(urls.join("\n").as_bytes()).is_ok();
            }
            // 231 — канал занят другим клиентом: подождать и попробовать снова
            Err(error) if error.raw_os_error() == Some(231) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return false,
        }
    }
    false
}

/// Слушает канал в отдельном потоке и передаёт каждое сообщение в `on_message`.
/// false — канал уже занят другим основным процессом.
pub fn serve(pipe: &str, on_message: impl Fn(Vec<String>) + Send + 'static) -> bool {
    let name = HSTRING::from(pipe);
    let create = move |first: bool| unsafe {
        let mut mode = PIPE_ACCESS_INBOUND;
        if first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            0,
            64 * 1024,
            0,
            None,
        )
    };
    let first = create(true);
    if first.is_invalid() {
        return false;
    }
    let first = first.0 as isize;
    std::thread::spawn(move || {
        let mut handle = first;
        loop {
            let pipe_handle = windows::Win32::Foundation::HANDLE(handle as *mut c_void);
            // Ошибка тут не значит, что сообщения нет: отправитель мог успеть записать его
            // и отключиться раньше, чем мы начали ждать (ERROR_PIPE_CONNECTED или
            // ERROR_NO_DATA), — данные всё равно лежат в канале. Поэтому читаем всегда.
            let _ = unsafe { ConnectNamedPipe(pipe_handle, None) };
            // Следующий канал открываем сразу, пока этот ещё не закрыт: иначе была бы
            // щель, когда канала нет совсем, и ссылка, присланная в этот миг, потерялась бы.
            let next = create(false);
            // File забирает дескриптор и закроет его сам
            let mut channel = unsafe { std::fs::File::from_raw_handle(handle as *mut c_void) };
            let mut text = String::new();
            if channel.read_to_string(&mut text).is_ok() {
                on_message(text.lines().map(str::to_string).filter(|l| !l.is_empty()).collect());
            }
            drop(channel);
            if next.is_invalid() {
                return;
            }
            handle = next.0 as isize;
        }
    });
    true
}

/// Ждёт, пока процесс завершится (не дольше `timeout`). Нужно при перезапуске:
/// новый процесс должен стартовать, когда старый уже освободил профиль.
pub fn wait_for_exit(pid: u32, timeout: Duration) {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let _ = WaitForSingleObject(process, timeout.as_millis() as u32);
            let _ = CloseHandle(process);
        }
    }
}

/// Перезапуск браузера: новый процесс дождётся завершения этого и его движка
/// WebView2 (он закрывается на секунду-две позже). Иначе новый процесс подхватил
/// бы ещё живой движок со старыми настройками — например, с прежним прокси.
pub fn restart(engine_pid: Option<u32>) {
    let mut pids = vec![std::process::id()];
    pids.extend(engine_pid);
    let list: Vec<String> = pids.iter().map(u32::to_string).collect();
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg(format!("--wait-pid={}", list.join(","))).spawn();
    }
}

/// Новое обычное окно — отдельный процесс с тем же профилем.
pub fn open_new_window(urls: &[String]) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg("--new-window").args(urls).spawn();
    }
}

// ---------- браузер по умолчанию ----------

fn set_value(path: &str, name: Option<&str>, value: &str) -> bool {
    unsafe {
        let mut key = HKEY::default();
        let created = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        );
        if created != ERROR_SUCCESS {
            return false;
        }
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
        let name = name.map(HSTRING::from);
        let name_ptr = name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
        let written = RegSetValueExW(key, name_ptr, None, REG_SZ, Some(bytes));
        let _ = RegCloseKey(key);
        written == ERROR_SUCCESS
    }
}

pub fn set_string(path: &str, name: Option<&str>, value: &str) -> bool {
    set_value(path, name, value)
}

pub fn set_dword(path: &str, name: &str, value: u32) -> bool {
    unsafe {
        let mut key = HKEY::default();
        if RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None)
            != ERROR_SUCCESS
        {
            return false;
        }
        let written = RegSetValueExW(key, &HSTRING::from(name), None, REG_DWORD, Some(&value.to_le_bytes()));
        let _ = RegCloseKey(key);
        written == ERROR_SUCCESS
    }
}

pub fn delete_key(path: &str) {
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path));
    }
}

/// Снимает регистрацию браузера в Windows (при удалении).
/// Снять регистрацию, только если она указывает на этот exe: удаление одной копии браузера
/// (например, поставленной установщиком в другую папку) не трогает регистрацию другой.
pub fn unregister_if_mine() {
    let Ok(exe) = std::env::current_exe() else { return };
    let command = get_value(&format!(r"{APP_KEY}\shell\open\command"), "").unwrap_or_default();
    let registered = command.trim().trim_matches('"').to_lowercase();
    if registered.is_empty() || registered == exe.to_string_lossy().to_lowercase() {
        unregister();
    }
}

pub fn unregister() {
    for path in [format!(r"Software\Classes\{URL_PROGID}"), format!(r"Software\Classes\{HTML_PROGID}"), APP_KEY.to_string()] {
        delete_key(&path);
    }
    delete_key_value(r"Software\RegisteredApplications", DISPLAY_NAME);
}

fn delete_key_value(path: &str, name: &str) {
    use windows::Win32::System::Registry::RegDeleteKeyValueW;
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(path), &HSTRING::from(name));
    }
}

fn get_value(path: &str, name: &str) -> Option<String> {
    unsafe {
        let mut buffer = [0u16; 512];
        let mut size = (buffer.len() * 2) as u32;
        let read = RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            &HSTRING::from(name),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        );
        if read != ERROR_SUCCESS {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buffer[..len]))
    }
}

/// Тёмная ли тема приложений в Windows (параметр «Режим приложений»).
pub fn system_prefers_dark() -> bool {
    let personalize = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    unsafe {
        let mut value = 1u32;
        let mut size = 4u32;
        let read = RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(personalize),
            &HSTRING::from("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        );
        read == ERROR_SUCCESS && value == 0
    }
}

/// Зарегистрирован ли браузер в Windows (тогда при запуске обновляем путь к exe).
pub fn is_registered() -> bool {
    get_value(APP_KEY, "").is_some()
}

/// Открывает ли Windows ссылки https в Rust Browser.
pub fn is_default() -> bool {
    let choice = r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice";
    get_value(choice, "ProgId").as_deref() == Some(URL_PROGID)
}

/// Регистрирует Rust Browser как браузер в Windows — только для текущего пользователя,
/// права администратора не нужны. Сам выбор «по умолчанию» Windows оставляет человеку.
pub fn register() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let exe = exe.to_string_lossy().to_string();
    let icon = format!("{exe},0");
    // %1 — после --single-argument и без кавычек, как у Chrome: см. split_args
    let open = format!("\"{exe}\" {SINGLE_ARGUMENT} %1");
    let caps = format!(r"{APP_KEY}\Capabilities");

    let mut ok = true;
    for progid in [URL_PROGID, HTML_PROGID] {
        let base = format!(r"Software\Classes\{progid}");
        let title = if progid == URL_PROGID { "Ссылка Rust Browser" } else { "Документ Rust Browser" };
        ok &= set_value(&base, None, title);
        ok &= set_value(&format!(r"{base}\DefaultIcon"), None, &icon);
        ok &= set_value(&format!(r"{base}\shell\open\command"), None, &open);
        if progid == URL_PROGID {
            ok &= set_value(&base, Some("URL Protocol"), "");
        }
    }
    ok &= set_value(APP_KEY, None, DISPLAY_NAME);
    ok &= set_value(&format!(r"{APP_KEY}\DefaultIcon"), None, &icon);
    ok &= set_value(&format!(r"{APP_KEY}\shell\open\command"), None, &format!("\"{exe}\""));
    ok &= set_value(&caps, Some("ApplicationName"), DISPLAY_NAME);
    ok &= set_value(&caps, Some("ApplicationDescription"), "Быстрый браузер на Rust");
    ok &= set_value(&caps, Some("ApplicationIcon"), &icon);
    ok &= set_value(&format!(r"{caps}\StartMenu"), Some("StartMenuInternet"), "RustBrowser");
    for scheme in ["http", "https"] {
        ok &= set_value(&format!(r"{caps}\URLAssociations"), Some(scheme), URL_PROGID);
    }
    for ext in [".htm", ".html", ".xhtml", ".shtml"] {
        ok &= set_value(&format!(r"{caps}\FileAssociations"), Some(ext), HTML_PROGID);
    }
    ok &= set_value(r"Software\RegisteredApplications", Some(DISPLAY_NAME), &caps);
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    ok
}

/// Страница «Приложения по умолчанию» Windows, сразу на Rust Browser.
pub fn open_default_apps_settings() {
    let _ = Command::new("explorer")
        .arg("ms-settings:defaultapps?registeredAppUser=Rust%20Browser")
        .spawn();
}

// ---------- адреса из командной строки ----------

/// Путь к файлу превращает в file:///, остальное оставляет как есть.
/// Разбирает аргументы запуска на флаги («--inprivate», «--uninstall»…) и адреса.
/// Ссылки и файлы Windows передаёт после «--single-argument» (так их регистрирует
/// `register`): всё, что за ним, — один адрес и флагом не считается. Иначе ссылка
/// с кавычкой внутри (`https://x/" --uninstall "`) превратилась бы в отдельный флаг.
pub fn split_args(args: &[String]) -> (Vec<String>, Vec<String>) {
    let tail = args.iter().position(|a| a == SINGLE_ARGUMENT);
    let head = &args[..tail.unwrap_or(args.len())];
    let flags = head.iter().filter(|a| a.starts_with("--")).cloned().collect();
    let mut addresses: Vec<String> = head.iter().filter(|a| !a.starts_with("--")).cloned().collect();
    if let Some(i) = tail {
        let single = args[i + 1..].join(" ");
        if !single.trim().is_empty() {
            addresses.push(single);
        }
    }
    (flags, addresses)
}

const SINGLE_ARGUMENT: &str = "--single-argument";

pub fn arg_to_address(arg: &str) -> String {
    let path = Path::new(arg);
    if arg.contains("://") || !path.is_absolute() || !path.exists() {
        return arg.to_string();
    }
    let mut url = String::from("file:///");
    for (i, part) in arg.replace('\\', "/").split('/').enumerate() {
        if i > 0 {
            url.push('/');
        }
        for ch in part.chars() {
            match ch {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | ':' => url.push(ch),
                _ => {
                    let mut buf = [0u8; 4];
                    for byte in ch.encode_utf8(&mut buf).bytes() {
                        url.push_str(&format!("%{byte:02X}"));
                    }
                }
            }
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_name_is_stable_and_case_insensitive() {
        let a = pipe_name(Path::new(r"C:\Users\Me\AppData\Roaming\RustBrowser"));
        let b = pipe_name(Path::new(r"c:\users\me\appdata\roaming\rustbrowser"));
        let other = pipe_name(Path::new(r"C:\Temp\test-data"));
        assert_eq!(a, b);
        assert_ne!(a, other);
        assert!(a.starts_with(r"\\.\pipe\RustBrowser-"));
    }

    #[test]
    fn link_after_single_argument_never_becomes_a_flag() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // так Windows разобьёт `"exe" --single-argument https://x/" --uninstall "`
        let (flags, urls) = split_args(&args(&["--single-argument", "https://x/", "--uninstall", ""]));
        assert!(flags.is_empty());
        assert_eq!(urls, ["https://x/ --uninstall "]);
        // путь к файлу с пробелами без кавычек склеивается обратно
        let (_, urls) = split_args(&args(&["--single-argument", r"C:\Мои", r"файлы\a.html"]));
        assert_eq!(urls, [r"C:\Мои файлы\a.html"]);
        // обычный запуск: флаги и адреса как раньше
        let (flags, urls) = split_args(&args(&["--inprivate", "https://a.com/"]));
        assert_eq!((flags, urls), (args(&["--inprivate"]), args(&["https://a.com/"])));
        assert_eq!(split_args(&args(&["--single-argument"])), (Vec::new(), Vec::new()));
    }

    #[test]
    fn file_paths_become_file_urls() {
        let dir = std::env::temp_dir().join("rb файл тест");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("стр 1.html");
        std::fs::write(&file, b"<p>x").unwrap();
        let url = arg_to_address(&file.to_string_lossy());
        assert!(url.starts_with("file:///"));
        assert!(url.ends_with("/%D1%81%D1%82%D1%80%201.html"), "{url}");
        assert!(!url.contains('\\'));
        let _ = std::fs::remove_dir_all(dir);

        assert_eq!(arg_to_address("https://a.com/"), "https://a.com/");
        assert_eq!(arg_to_address("example.com"), "example.com");
    }

    #[test]
    fn messages_pass_through_the_pipe() {
        let pipe = pipe_name(Path::new(&format!("rb-pipe-test-{}", std::process::id())));
        let (tx, rx) = std::sync::mpsc::channel();
        assert!(serve(&pipe, move |urls| {
            let _ = tx.send(urls);
        }));
        assert!(!serve(&pipe, |_| {}), "второй основной процесс на том же канале не нужен");
        assert!(forward_to_running(&pipe, &["https://a.com/".into(), "https://b.com/".into()]));
        let got = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(got, vec!["https://a.com/", "https://b.com/"]);
        assert!(forward_to_running(&pipe, &[]));
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), Vec::<String>::new());
    }
}

