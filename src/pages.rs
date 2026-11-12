//! Импорт избранного из других браузеров и своя страница ошибки загрузки.

use std::path::PathBuf;

use serde_json::Value;

use crate::store::Bookmark;

// ---------- импорт избранного ----------

/// Браузеры на движке Chromium, из которых можно забрать избранное:
/// (идентификатор, название, путь к файлу Bookmarks относительно %LOCALAPPDATA%).
const SOURCES: [(&str, &str, &str); 3] = [
    ("edge", "Edge", r"Microsoft\Edge\User Data\Default\Bookmarks"),
    ("chrome", "Chrome", r"Google\Chrome\User Data\Default\Bookmarks"),
    ("yandex", "Яндекс Браузер", r"Yandex\YandexBrowser\User Data\Default\Bookmarks"),
];

fn source_path(relative: &str) -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join(relative))
}

/// Установленные браузеры, у которых есть файл избранного: (id, название).
pub fn import_sources() -> Vec<(&'static str, &'static str)> {
    SOURCES
        .iter()
        .filter(|(_, _, path)| source_path(path).is_some_and(|p| p.exists()))
        .map(|(id, name, _)| (*id, *name))
        .collect()
}

/// Читает избранное браузера; папки разворачиваются в общий список.
pub fn read_bookmarks(source: &str) -> Vec<Bookmark> {
    let Some((_, _, relative)) = SOURCES.iter().find(|(id, ..)| *id == source) else {
        return Vec::new();
    };
    let Some(text) = source_path(relative).and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Vec::new();
    };
    parse_chromium_bookmarks(&text)
}

pub fn parse_chromium_bookmarks(json: &str) -> Vec<Bookmark> {
    fn walk(node: &Value, out: &mut Vec<Bookmark>) {
        match node["type"].as_str() {
            Some("url") => {
                let url = node["url"].as_str().unwrap_or("");
                // закладки-скрипты (javascript:) и служебные адреса не переносим
                if url.starts_with("http://") || url.starts_with("https://") {
                    let name = node["name"].as_str().unwrap_or("").trim();
                    out.push(Bookmark {
                        title: if name.is_empty() { url.to_string() } else { name.to_string() },
                        url: url.to_string(),
                    });
                }
            }
            _ => {
                if let Some(children) = node["children"].as_array() {
                    for child in children {
                        walk(child, out);
                    }
                }
            }
        }
    }
    let Ok(root) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let mut out = Vec::new();
    if let Some(roots) = root["roots"].as_object() {
        // сначала панель избранного, потом остальное — как их видит человек
        for key in ["bookmark_bar", "other", "synced"] {
            if let Some(node) = roots.get(key) {
                walk(node, &mut out);
            }
        }
    }
    out
}

// ---------- страница ошибки ----------

/// Что пошло не так — на человеческом языке. Получает код WebErrorStatus из WebView2.
pub fn describe_error(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "dns" => ("Не удалось найти сайт", "Проверьте адрес. Если он верный — возможно, сайт недоступен через ваш VPN или сеть."),
        "timeout" => ("Сайт слишком долго не отвечает", "Возможно, сеть или VPN сейчас работают медленно. Попробуйте ещё раз."),
        "connection" => ("Не удалось подключиться", "Соединение оборвалось. Проверьте интернет и VPN, затем повторите."),
        "certificate" => ("Небезопасное подключение", "У сайта проблема с сертификатом, поэтому браузер не стал открывать его."),
        "offline" => ("Нет подключения к интернету", "Проверьте Wi-Fi или кабель и попробуйте ещё раз."),
        _ => ("Сайт не открылся", "Что-то пошло не так при загрузке страницы."),
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Своя страница вместо стандартной страницы ошибки WebView2.
pub fn error_page(url: &str, kind: &str, dark: bool) -> String {
    let (title, hint) = describe_error(kind);
    let host = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url);
    // в JS адрес попадает через JSON — кавычки и скобки в нём ничего не сломают
    let url_js = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into()).replace("</", "<\\/");
    let (bg, surface, text, muted, border) = if dark {
        ("#121318", "#23242c", "#eceef5", "#9a9fb1", "rgba(255,255,255,.08)")
    } else {
        ("#f3f4f9", "#ffffff", "#1b1c22", "#697084", "rgba(20,24,48,.09)")
    };
    format!(
        r##"<!doctype html><html lang="ru"><head><meta charset="utf-8"><title>{title}</title>
<style>
  * {{ box-sizing: border-box; }}
  html, body {{ height: 100%; margin: 0; }}
  body {{ display: grid; place-items: center; background: {bg}; color: {text};
    font: 15px/1.5 "Segoe UI Variable Text", "Segoe UI", sans-serif; overflow: hidden; }}
  .glow {{ position: fixed; inset: -30%; opacity: .25; pointer-events: none;
    background: radial-gradient(40% 40% at 30% 30%, #4f5bf5, transparent 70%),
                radial-gradient(40% 40% at 70% 65%, #e04a9a, transparent 70%); }}
  .card {{ position: relative; width: min(520px, calc(100% - 40px)); padding: 40px 36px 32px;
    background: {surface}; border: 1px solid {border}; border-radius: 22px;
    box-shadow: 0 20px 60px rgba(0,0,0,.25); text-align: center; animation: rise .5s cubic-bezier(.2,.8,.2,1); }}
  @keyframes rise {{ from {{ opacity: 0; transform: translateY(16px) scale(.98); }} }}
  .icon {{ width: 64px; height: 64px; margin: 0 auto 18px; display: grid; place-items: center;
    border-radius: 20px; background: linear-gradient(135deg, #4f5bf5, #e04a9a); color: #fff; }}
  h1 {{ margin: 0 0 8px; font-size: 22px; font-weight: 600; }}
  .host {{ display: inline-block; margin-bottom: 14px; padding: 3px 12px; border-radius: 12px;
    background: {border}; color: {muted}; font-size: 13px; word-break: break-all; }}
  p {{ margin: 0 0 24px; color: {muted}; }}
  .row {{ display: flex; gap: 10px; justify-content: center; }}
  button {{ height: 38px; padding: 0 20px; border: 0; border-radius: 10px; font: inherit; cursor: pointer;
    transition: transform .12s, filter .15s; }}
  button:active {{ transform: scale(.96); }}
  .primary {{ background: #4f5bf5; color: #fff; font-weight: 600; }}
  .primary:hover {{ filter: brightness(1.1); }}
  .ghost {{ background: transparent; color: {text}; border: 1px solid {border}; }}
</style></head><body>
<div class="glow"></div>
<div class="card">
  <div class="icon"><svg width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5a10 10 0 0 1 14 0"/>
    <path d="M8.5 16a5 5 0 0 1 7 0"/><path d="M12 19.5h.01"/><path d="m3 3 18 18"/></svg></div>
  <h1>{title}</h1>
  <div class="host">{host}</div>
  <p>{hint}</p>
  <div class="row">
    <button class="ghost" onclick="history.back()">Назад</button>
    <button class="primary" id="retry" autofocus>Повторить</button>
  </div>
</div>
<script>
  const url = {url_js};
  document.getElementById('retry').onclick = () => location.href = url;
  // как только сеть вернулась — пробуем сами
  addEventListener('online', () => location.href = url);
</script>
</body></html>"##,
        title = title,
        hint = hint,
        host = escape_html(host),
        url_js = url_js,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_chromium_bookmarks() {
        let json = r#"{"roots":{
            "bookmark_bar":{"type":"folder","children":[
                {"type":"url","name":"Rust","url":"https://www.rust-lang.org/"},
                {"type":"folder","name":"Работа","children":[
                    {"type":"url","name":"","url":"https://github.com/"},
                    {"type":"url","name":"скрипт","url":"javascript:alert(1)"}]}]},
            "other":{"type":"folder","children":[{"type":"url","name":"Вики","url":"https://ru.wikipedia.org/"}]},
            "synced":{"type":"folder","children":[]}}}"#;
        let list = parse_chromium_bookmarks(json);
        let urls: Vec<&str> = list.iter().map(|b| b.url.as_str()).collect();
        assert_eq!(urls, ["https://www.rust-lang.org/", "https://github.com/", "https://ru.wikipedia.org/"]);
        assert_eq!(list[1].title, "https://github.com/"); // пустое имя — показываем адрес
        assert!(parse_chromium_bookmarks("не json").is_empty());
    }

    #[test]
    fn error_page_escapes_the_address() {
        let html = error_page("https://evil.com/</script><script>alert(1)</script>", "dns", true);
        assert!(!html.contains("</script><script>alert"));
        assert!(html.contains("Не удалось найти сайт"));
        let host = error_page("https://a<b>.com/", "timeout", false);
        assert!(host.contains("a&lt;b&gt;.com"));
    }
}
