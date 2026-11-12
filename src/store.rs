//! Закладки, история и настройки: хранение в JSON-файлах.

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

const MAX_HISTORY: usize = 5000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Visit {
    pub url: String,
    pub title: String,
    /// Время посещения, секунды Unix.
    pub time: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub adblock: bool,
    pub blocked_total: u64,
    /// Идентификатор поисковой системы, см. ENGINES в main.rs.
    pub search: String,
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Открывать вкладки прошлого сеанса при запуске.
    pub restore_session: bool,
    /// Панель избранного под адресной строкой.
    pub favbar: bool,
    /// Через сколько минут усыплять фоновую вкладку; 0 — никогда.
    pub sleep_minutes: u32,
    /// Папка загрузок; пустая строка — папка «Загрузки» Windows.
    pub download_dir: String,
    /// Предлагать сохранять пароли.
    pub passwords: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            adblock: true,
            blocked_total: 0,
            search: "google".to_string(),
            theme: "system".to_string(),
            restore_session: true,
            favbar: false,
            sleep_minutes: 15,
            download_dir: String::new(),
            passwords: true,
        }
    }
}

/// Вкладка в сохранённом сеансе.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    /// Номер активной вкладки.
    pub active: usize,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Suggestion {
    pub title: String,
    pub url: String,
    /// "bookmark" или "history".
    pub kind: &'static str,
}

pub struct Store {
    /// None — ничего не пишем на диск (тесты).
    dir: Option<PathBuf>,
    pub bookmarks: Vec<Bookmark>,
    /// От старых посещений к новым.
    pub history: Vec<Visit>,
    pub settings: Settings,
}

impl Store {
    pub fn load(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Self {
            bookmarks: read_json(&dir.join("bookmarks.json")),
            history: read_json(&dir.join("history.json")),
            settings: read_json(&dir.join("settings.json")),
            dir: Some(dir),
        }
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self { dir: None, bookmarks: Vec::new(), history: Vec::new(), settings: Settings::default() }
    }

    pub fn save_bookmarks(&self) {
        self.write("bookmarks.json", &self.bookmarks);
    }

    pub fn save_history(&self) {
        self.write("history.json", &self.history);
    }

    pub fn save_settings(&self) {
        self.write("settings.json", &self.settings);
    }

    pub fn save_all(&self) {
        self.save_bookmarks();
        self.save_history();
        self.save_settings();
    }

    pub fn load_session(&self) -> Session {
        match &self.dir {
            Some(dir) => read_json(&dir.join("session.json")),
            None => Session::default(),
        }
    }

    pub fn save_session(&self, session: &Session) {
        self.write("session.json", session);
    }

    /// Перечитывает закладки с диска: их могло изменить окно InPrivate.
    pub fn reload_bookmarks(&mut self) {
        if let Some(dir) = &self.dir {
            self.bookmarks = read_json(&dir.join("bookmarks.json"));
        }
    }

    /// Пишет во временный файл и переименовывает, чтобы не испортить данные при сбое.
    fn write<T: Serialize>(&self, name: &str, value: &T) {
        let Some(dir) = &self.dir else { return };
        let Ok(text) = serde_json::to_string(value) else { return };
        let tmp = dir.join(format!("{name}.tmp"));
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(&tmp, dir.join(name));
        }
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.iter().any(|b| b.url == url)
    }

    /// Добавляет или убирает закладку. Возвращает true, если она теперь есть.
    pub fn toggle_bookmark(&mut self, url: &str, title: &str) -> bool {
        if self.is_bookmarked(url) {
            self.remove_bookmark(url);
            false
        } else {
            let title = if title.is_empty() { url } else { title };
            self.bookmarks.push(Bookmark { title: title.to_string(), url: url.to_string() });
            true
        }
    }

    pub fn remove_bookmark(&mut self, url: &str) {
        self.bookmarks.retain(|b| b.url != url);
    }

    pub fn add_visit(&mut self, url: &str, title: &str) {
        self.add_visit_at(url, title, now());
    }

    fn add_visit_at(&mut self, url: &str, title: &str, time: u64) {
        // Обновление страницы не плодит записи.
        if let Some(last) = self.history.last_mut() {
            if last.url == url {
                last.time = time;
                if !title.is_empty() {
                    last.title = title.to_string();
                }
                return;
            }
        }
        self.history.push(Visit { url: url.to_string(), title: title.to_string(), time });
        if self.history.len() > MAX_HISTORY {
            let extra = self.history.len() - MAX_HISTORY;
            self.history.drain(..extra);
        }
    }

    /// Заголовок приходит позже адреса — дописываем его в последнее посещение.
    pub fn set_title(&mut self, url: &str, title: &str) {
        if let Some(visit) = self.history.iter_mut().rev().find(|v| v.url == url) {
            visit.title = title.to_string();
        }
    }

    pub fn remove_visit(&mut self, url: &str, time: u64) {
        self.history.retain(|v| !(v.url == url && v.time == time));
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Посещения от новых к старым, отфильтрованные по подстроке.
    pub fn search_history(&self, query: &str, limit: usize) -> Vec<Visit> {
        let query = query.trim().to_lowercase();
        self.history
            .iter()
            .rev()
            .filter(|v| query.is_empty() || matches(&query, &v.url, &v.title))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Подсказки для адресной строки: закладки и часто посещаемые страницы.
    pub fn suggest(&self, query: &str, limit: usize) -> Vec<Suggestion> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }

        // url -> (оценка, заголовок, вид)
        let mut found: HashMap<&str, (u64, &str, &'static str)> = HashMap::new();

        for (index, visit) in self.history.iter().enumerate() {
            if !matches(&query, &visit.url, &visit.title) {
                continue;
            }
            let entry = found.entry(&visit.url).or_insert((0, "", "history"));
            entry.0 += 10; // каждое посещение добавляет вес
            entry.0 = entry.0.max(10 + index as u64 / 100); // свежие чуть выше
            if !visit.title.is_empty() {
                entry.1 = &visit.title;
            }
        }
        for bookmark in &self.bookmarks {
            if !matches(&query, &bookmark.url, &bookmark.title) {
                continue;
            }
            let entry = found.entry(&bookmark.url).or_insert((0, "", "bookmark"));
            entry.0 += 500;
            entry.1 = &bookmark.title;
            entry.2 = "bookmark";
        }

        let mut list: Vec<_> = found
            .into_iter()
            .map(|(url, (mut score, title, kind))| {
                if bare(url).starts_with(&query) {
                    score += 1000; // адрес начинается с набранного
                }
                (score, url, title, kind)
            })
            .collect();
        list.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        list.into_iter()
            .take(limit)
            .map(|(_, url, title, kind)| Suggestion {
                title: title.to_string(),
                url: url.to_string(),
                kind,
            })
            .collect()
    }
}

fn read_json<T: DeserializeOwned + Default>(path: &PathBuf) -> T {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Адрес без схемы и www — так его обычно набирают.
fn bare(url: &str) -> &str {
    let url = url.split_once("://").map_or(url, |(_, rest)| rest);
    url.strip_prefix("www.").unwrap_or(url)
}

fn matches(query_lower: &str, url: &str, title: &str) -> bool {
    url.to_lowercase().contains(query_lower) || title.to_lowercase().contains(query_lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_file_gets_new_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"adblock":false,"search":"google"}"#).unwrap();
        assert!(!settings.adblock);
        assert_eq!(settings.search, "google");
        assert_eq!(settings.theme, "system");
        assert!(settings.restore_session);
        assert_eq!(settings.sleep_minutes, 15);
    }

    #[test]
    fn bookmark_toggles() {
        let mut store = Store::in_memory();
        assert!(store.toggle_bookmark("https://a.com/", "A"));
        assert!(store.is_bookmarked("https://a.com/"));
        assert!(!store.toggle_bookmark("https://a.com/", "A"));
        assert!(store.bookmarks.is_empty());
    }

    #[test]
    fn bookmark_without_title_uses_url() {
        let mut store = Store::in_memory();
        store.toggle_bookmark("https://a.com/", "");
        assert_eq!(store.bookmarks[0].title, "https://a.com/");
    }

    #[test]
    fn reload_does_not_duplicate_visit() {
        let mut store = Store::in_memory();
        store.add_visit_at("https://a.com/", "A", 1);
        store.add_visit_at("https://a.com/", "", 2);
        store.add_visit_at("https://b.com/", "B", 3);
        assert_eq!(store.history.len(), 2);
        assert_eq!(store.history[0].time, 2);
        assert_eq!(store.history[0].title, "A");
    }

    #[test]
    fn history_is_capped() {
        let mut store = Store::in_memory();
        for i in 0..MAX_HISTORY + 10 {
            store.add_visit_at(&format!("https://a.com/{i}"), "", i as u64);
        }
        assert_eq!(store.history.len(), MAX_HISTORY);
        assert_eq!(store.history[0].url, "https://a.com/10");
    }

    #[test]
    fn search_is_newest_first_and_case_insensitive() {
        let mut store = Store::in_memory();
        store.add_visit_at("https://a.com/", "Котики", 1);
        store.add_visit_at("https://b.com/", "Собаки", 2);
        store.add_visit_at("https://c.com/", "КОТ учёный", 3);
        let found = store.search_history("кот", 10);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].url, "https://c.com/");
        assert_eq!(store.search_history("", 2).len(), 2);
    }

    #[test]
    fn remove_visit_and_set_title() {
        let mut store = Store::in_memory();
        store.add_visit_at("https://a.com/", "", 1);
        store.set_title("https://a.com/", "A");
        assert_eq!(store.history[0].title, "A");
        store.remove_visit("https://a.com/", 1);
        assert!(store.history.is_empty());
    }

    #[test]
    fn suggestions_rank_prefix_then_bookmarks() {
        let mut store = Store::in_memory();
        store.add_visit_at("https://news.com/rust", "Rust news", 1);
        store.add_visit_at("https://www.rust-lang.org/", "Rust", 2);
        store.toggle_bookmark("https://doc.rust-lang.org/book/", "The Rust Book");
        let list = store.suggest("rust", 5);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].url, "https://www.rust-lang.org/"); // начинается с запроса
        assert_eq!(list[1].kind, "bookmark");
        assert!(store.suggest("  ", 5).is_empty());
    }

    #[test]
    fn suggestions_dedupe_by_url() {
        let mut store = Store::in_memory();
        store.add_visit_at("https://a.com/", "A", 1);
        store.add_visit_at("https://b.com/", "B", 2);
        store.add_visit_at("https://a.com/", "A", 3);
        store.toggle_bookmark("https://a.com/", "A");
        let list = store.suggest("a.com", 5);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, "bookmark");
    }

    #[test]
    fn saves_and_loads_from_disk() {
        let dir = std::env::temp_dir().join(format!("rust-browser-test-{}", std::process::id()));
        let mut store = Store::load(dir.clone());
        store.toggle_bookmark("https://a.com/", "A");
        store.add_visit_at("https://a.com/", "A", 7);
        store.settings.adblock = false;
        store.settings.theme = "dark".into();
        store.save_all();
        let session = Session {
            tabs: vec![SessionTab { url: "https://a.com/".into(), title: "A".into(), pinned: true }],
            active: 0,
        };
        store.save_session(&session);

        let loaded = Store::load(dir.clone());
        assert_eq!(loaded.bookmarks, store.bookmarks);
        assert_eq!(loaded.history, store.history);
        assert!(!loaded.settings.adblock);
        assert_eq!(loaded.settings.theme, "dark");
        assert_eq!(loaded.load_session(), session);
        let _ = fs::remove_dir_all(dir);
    }
}
