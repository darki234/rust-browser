//! Иконки сайтов для плиток и своих кнопок.
//!
//! favicon.ico у многих сайтов нет, он крошечный или тёмный на прозрачном фоне.
//! Поэтому браузер сам открывает главную страницу сайта, ищет в ней крупную
//! иконку (apple-touch-icon, `<link rel="icon" sizes=…>`, svg), скачивает её и
//! кладёт в папку данных. Следующий показ — мгновенный и без сети.
//! Если иконки нет, запоминаем это на неделю, чтобы не стучаться зря.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime},
};

const RETRY_AFTER: Duration = Duration::from_secs(24 * 3600);
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

/// Прокси браузера: значки качаем тем же путём, что и страницы.
static PROXY: Mutex<Option<String>> = Mutex::new(None);

pub fn set_proxy(proxy: Option<String>) {
    if let Ok(mut guard) = PROXY.lock() {
        *guard = proxy;
    }
}

/// Хост пригоден для имени файла и запроса: буквы, цифры, точки, дефисы.
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && !host.starts_with('.')
}

/// Значок из кэша или с сайта. Блокирует поток — вызывать не из главного.
pub fn get(dir: &Path, host: &str) -> Option<(Vec<u8>, &'static str)> {
    let host = host.to_ascii_lowercase();
    let host = host.trim_start_matches("www.");
    if !valid_host(host) {
        return None;
    }
    // один сайт — одна загрузка, даже если плиток с ним несколько
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    let lock = {
        let mut map = LOCKS.get_or_init(Default::default).lock().ok()?;
        map.entry(host.to_string()).or_default().clone()
    };
    let _guard = lock.lock().ok()?;

    if let Some(found) = cached(dir, host) {
        return found;
    }
    let _ = fs::create_dir_all(dir);
    match fetch(host) {
        Some((bytes, ext)) => {
            let _ = fs::write(dir.join(format!("{host}.{ext}")), &bytes);
            Some((bytes, mime(ext)))
        }
        None => {
            let _ = fs::write(dir.join(format!("{host}.none")), b"");
            None
        }
    }
}

/// Some(None) — недавно искали и не нашли; None — в кэше ничего.
fn cached(dir: &Path, host: &str) -> Option<Option<(Vec<u8>, &'static str)>> {
    for ext in ["png", "svg", "ico", "jpg", "webp", "gif"] {
        if let Ok(bytes) = fs::read(dir.join(format!("{host}.{ext}"))) {
            return Some(Some((bytes, mime(ext))));
        }
    }
    let none = dir.join(format!("{host}.none"));
    let fresh = fs::metadata(&none)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < RETRY_AFTER);
    fresh.then_some(None)
}

fn mime(ext: &str) -> &'static str {
    match ext {
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "jpg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/png",
    }
}

fn fetch(host: &str) -> Option<(Vec<u8>, &'static str)> {
    // страница не открылась (медленный VPN, защита от ботов) — пробуем стандартные адреса
    let (html, base) = download_page(&format!("https://{host}/")).unwrap_or_else(|| (String::new(), format!("https://{host}/")));
    let mut candidates = find_icons(&html, &base);
    for fallback in ["/apple-touch-icon.png", "/favicon.ico"] {
        candidates.push(Candidate { url: join(&base, fallback), score: 0 });
    }
    candidates.sort_by_key(|c| std::cmp::Reverse(c.score));
    candidates.dedup_by(|a, b| a.url == b.url);
    candidates.iter().take(5).find_map(|c| {
        let bytes = curl(&c.url, 2_000_000, 15)?.0;
        let ext = image_kind(&bytes)?;
        // крошечные значки (16×16) на плитке выглядят мылом — ищем дальше
        (ext != "ico" || bytes.len() > 1500 || c.score == 0).then_some((bytes, ext))
    })
}

/// curl.exe есть в каждой Windows 10/11; окна консоли не будет.
pub fn curl(url: &str, limit: u64, seconds: u32) -> Option<(Vec<u8>, String)> {
    use std::os::windows::process::CommandExt;
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return None;
    }
    let mut command = Command::new("curl.exe");
    command.args(curl_args(url, limit, seconds, PROXY.lock().ok().and_then(|p| p.clone())));
    let output = command.creation_flags(0x0800_0000).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut body = output.stdout;
    let split = body.iter().rposition(|&b| b == b'\n')?;
    let final_url = String::from_utf8_lossy(&body[split + 1..]).to_string();
    body.truncate(split);
    Some((body, final_url))
}

/// Аргументы curl. Адрес значка берётся со страницы сайта, поэтому:
/// `-g` — без «шаблонов» curl: иначе `https://x/[1-99999999]` или `{a,b,…}` превратился бы
/// в миллионы запросов, а их ответы копились бы в памяти браузера;
/// `--proto` — только http(s), в том числе при переадресациях.
fn curl_args(url: &str, limit: u64, seconds: u32, proxy: Option<String>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-sSLf", "-g", "--proto", "=http,https", "--proto-redir", "=http,https",
        "--max-time", &seconds.to_string(), "--max-redirs", "5", "--max-filesize", &limit.to_string(), "-A", UA,
        "-H", "Accept-Language: ru,en;q=0.8", "-w", "\n%{url_effective}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if let Some(proxy) = proxy {
        args.extend(["-x".to_string(), proxy]);
    }
    // «--» не нужен: адрес всегда начинается с http(s)://, флагом он не станет
    args.push(url.to_string());
    args
}

fn download_page(url: &str) -> Option<(String, String)> {
    let (bytes, final_url) = curl(url, 3_000_000, 40)?;
    Some((String::from_utf8_lossy(&bytes).to_string(), final_url))
}

#[derive(Debug, PartialEq)]
pub struct Candidate {
    pub url: String,
    pub score: u32,
}

/// Ищет `<link rel=…icon… href=… sizes=…>` и оценивает: крупнее и apple-touch — лучше.
pub fn find_icons(html: &str, base: &str) -> Vec<Candidate> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(start) = lower[from..].find("<link").map(|i| from + i) {
        let end = lower[start..].find('>').map(|i| start + i).unwrap_or(lower.len());
        from = end;
        let tag = &html[start..end];
        let Some(rel) = attr(tag, "rel").map(|r| r.to_ascii_lowercase()) else { continue };
        if !rel.split_whitespace().any(|r| r.contains("icon")) || rel.contains("mask-icon") {
            continue;
        }
        let Some(href) = attr(tag, "href").filter(|h| !h.starts_with("data:")) else { continue };
        let size = attr(tag, "sizes")
            .and_then(|s| s.split(['x', 'X']).next().and_then(|n| n.trim().parse::<u32>().ok()))
            .unwrap_or(0);
        let mut score = size.min(512);
        if rel.contains("apple-touch-icon") {
            score += 300; // у них есть фон — на тёмных обоях видно лучше
        }
        if href.to_ascii_lowercase().ends_with(".svg") || attr(tag, "type").is_some_and(|t| t.contains("svg")) {
            score += 250;
        }
        score += 10;
        out.push(Candidate { url: join(base, &decode_entities(&href)), score });
    }
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut search = 0;
    while let Some(pos) = lower[search..].find(name).map(|i| search + i) {
        search = pos + name.len();
        let before = lower[..pos].chars().last().unwrap_or(' ');
        if !before.is_whitespace() {
            continue;
        }
        let rest = tag[search..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let value = match rest.chars().next()? {
            q @ ('"' | '\'') => rest[1..].split(q).next()?.to_string(),
            _ => rest.split(|c: char| c.is_whitespace() || c == '>').next()?.to_string(),
        };
        return Some(value);
    }
    None
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&").replace("&#47;", "/")
}

/// Относительный адрес → полный, относительно страницы.
pub fn join(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    let scheme = if base.starts_with("http://") { "http" } else { "https" };
    if let Some(rest) = href.strip_prefix("//") {
        return format!("{scheme}://{rest}");
    }
    let after = base.split_once("://").map(|(_, r)| r).unwrap_or(base);
    let host = after.split(['/', '?', '#']).next().unwrap_or("");
    if href.starts_with('/') {
        return format!("{scheme}://{host}{href}");
    }
    let path = after[host.len()..].split(['?', '#']).next().unwrap_or("/");
    let dir = &path[..path.rfind('/').map(|i| i + 1).unwrap_or(0)];
    let dir = if dir.is_empty() { "/" } else { dir };
    format!("{scheme}://{host}{dir}{href}")
}

/// Что это за картинка — по первым байтам, а не по расширению в адресе.
pub fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG") {
        Some("png")
    } else if bytes.starts_with(&[0, 0, 1, 0]) {
        Some("ico")
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF8") {
        Some("gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
        (head.contains("<svg") && !head.contains("<html")).then_some("svg")
    }
}

/// Есть ли уже хорошая картинка для сайта. Крошечный favicon.ico (16×16) не в счёт —
/// его заменим крупным значком, когда сайт откроют.
pub fn has(dir: &Path, host: &str) -> bool {
    match cached(dir, host.trim_start_matches("www.")) {
        Some(Some((bytes, mime))) => mime != "image/x-icon" || bytes.len() > 5000,
        _ => false,
    }
}

/// Скрипт для открытой страницы: какие значки она объявляет. Ответ — JSON [[rel, href, sizes]].
pub const PAGE_ICONS: &str = "JSON.stringify([...document.querySelectorAll('link[rel~=icon],link[rel*=apple-touch-icon]')].map(l => [l.rel, l.href, l.getAttribute('sizes') || '', l.type || '']))";

/// Сохраняет лучший значок из тех, что объявила открытая страница. Блокирует — в фоне.
pub fn save_from_page(dir: &Path, host: &str, page_url: &str, list: &[(String, String, String, String)]) -> bool {
    let host = host.to_ascii_lowercase();
    let host = host.trim_start_matches("www.");
    if !valid_host(host) || has(dir, host) {
        return false;
    }
    let html: String = list
        .iter()
        .filter(|(_, href, _, _)| href.starts_with("https://") || href.starts_with("http://"))
        .map(|(rel, href, sizes, kind)| format!("<link rel=\"{rel}\" href=\"{href}\" sizes=\"{sizes}\" type=\"{kind}\">"))
        .collect();
    let mut candidates = find_icons(&html, page_url);
    candidates.sort_by_key(|c| std::cmp::Reverse(c.score));
    let found = candidates.iter().take(4).find_map(|c| {
        let bytes = curl(&c.url, 2_000_000, 15)?.0;
        image_kind(&bytes).map(|ext| (bytes, ext))
    });
    let Some((bytes, ext)) = found else { return false };
    let _ = fs::create_dir_all(dir);
    for old in ["none", "ico", "png", "svg", "jpg", "webp", "gif"] {
        let _ = fs::remove_file(dir.join(format!("{host}.{old}")));
    }
    fs::write(dir.join(format!("{host}.{ext}")), bytes).is_ok()
}

pub fn cache_dir(data: &Path) -> PathBuf {
    data.join("site-icons")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_largest_and_apple_touch() {
        let html = r#"<head>
            <link rel="icon" href="/favicon-16.png" sizes="16x16">
            <LINK REL="shortcut icon" href='/big.png' sizes=192x192>
            <link rel="apple-touch-icon" href="https://cdn.site.ru/touch.png?v=1&amp;x=2">
            <link rel="mask-icon" href="/mask.svg">
            <link rel="stylesheet" href="/a.css">
        </head>"#;
        let mut icons = find_icons(html, "https://site.ru/ru/index.html");
        icons.sort_by_key(|c| std::cmp::Reverse(c.score));
        assert_eq!(icons[0].url, "https://cdn.site.ru/touch.png?v=1&x=2");
        assert_eq!(icons[1].url, "https://site.ru/big.png");
        assert_eq!(icons.len(), 3);
    }

    #[test]
    fn curl_never_expands_page_supplied_globs() {
        let url = "https://victim.ru/[1-99999999]{a,b}.png";
        let args = curl_args(url, 1000, 15, None);
        assert!(args.iter().any(|a| a == "-g"), "без -g curl раскроет шаблон в адресе");
        assert_eq!(args.iter().position(|a| a == "--proto").map(|i| args[i + 1].as_str()), Some("=http,https"));
        assert_eq!(args.iter().position(|a| a == "--proto-redir").map(|i| args[i + 1].as_str()), Some("=http,https"));
        assert_eq!(args.last().map(String::as_str), Some(url));
        // со страницы может прийти что угодно — до curl доходят только http(s)
        assert!(curl("-o C:/x", 10, 1).is_none());
        assert!(curl("file:///C:/Windows/win.ini", 10, 1).is_none());
    }

    #[test]
    fn joins_relative_urls() {
        assert_eq!(join("https://a.ru/x/y.html", "i.png"), "https://a.ru/x/i.png");
        assert_eq!(join("https://a.ru", "i.png"), "https://a.ru/i.png");
        assert_eq!(join("https://a.ru/x/", "/i.png"), "https://a.ru/i.png");
        assert_eq!(join("https://a.ru/", "//cdn.b.ru/i.png"), "https://cdn.b.ru/i.png");
    }

    #[test]
    fn recognises_images_and_rejects_pages() {
        assert_eq!(image_kind(b"\x89PNG\r\n"), Some("png"));
        assert_eq!(image_kind(&[0, 0, 1, 0, 1]), Some("ico"));
        assert_eq!(image_kind(b"<svg xmlns='http://www.w3.org/2000/svg'>"), Some("svg"));
        assert_eq!(image_kind(b"<!doctype html><html><svg>"), None);
        assert!(valid_host("music.yandex.ru"));
        assert!(!valid_host("../etc"));
        assert!(!valid_host("localhost"));
    }
}
