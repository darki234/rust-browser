//! Закладки, история и настройки: хранение в JSON-файлах.

use std::{
    collections::{BTreeMap, HashMap},
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
    /// Режим эффективности Windows для процессов фоновых вкладок.
    pub efficiency_mode: bool,
    /// От батареи вкладки засыпают через 5 минут.
    pub battery_saver: bool,
    /// Обои новой вкладки: имя готового варианта или "image" — своё фото.
    pub wallpaper: String,
    /// Затемнение своего фото, 0–85 %, чтобы текст на нём читался.
    pub wallpaper_dim: u32,
    /// Растёт при каждой смене фото: интерфейс не возьмёт старую картинку из кэша.
    pub wallpaper_version: u64,
    /// Какая версия uBlock Origin Lite установлена в профиль.
    pub ubol_version: String,
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
            efficiency_mode: true,
            battery_saver: true,
            wallpaper: "aurora".to_string(),
            wallpaper_dim: 35,
            wallpaper_version: 0,
            ubol_version: String::new(),
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
    /// Посещения, ещё не записанные на диск. При сохранении с диском сливаются
    /// только они: так удалённое в другом окне не возвращается.
    pending: Vec<Visit>,
    pub settings: Settings,
    /// Масштаб по сайтам, как в Edge: сайт → коэффициент (100 % не храним).
    pub site_zoom: BTreeMap<String, f64>,
}

impl Store {
    pub fn load(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Self {
            bookmarks: read_json(&dir.join("bookmarks.json")),
            history: read_json(&dir.join("history.json")),
            pending: Vec::new(),
            settings: read_json(&dir.join("settings.json")),
            site_zoom: read_json(&dir.join("zoom.json")),
            dir: Some(dir),
        }
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self {
            dir: None,
            bookmarks: Vec::new(),
            history: Vec::new(),
            pending: Vec::new(),
            settings: Settings::default(),
            site_zoom: BTreeMap::new(),
        }
    }

    pub fn save_bookmarks(&self) {
        self.write("bookmarks.json", &self.bookmarks);
    }

    /// Сохраняет журнал, сливая его с тем, что на диске: в других окнах
    /// браузера (отдельных процессах) тоже могли появиться посещения.
    pub fn save_history(&mut self) {
        if let Some(dir) = &self.dir {
            let disk: Vec<Visit> = read_json(&dir.join("history.json"));
            self.history = merge_history(disk, std::mem::take(&mut self.pending));
        }
        self.write("history.json", &self.history);
    }

    /// Перезаписывает журнал на диске ровно тем, что в памяти (после удаления записей).
    fn overwrite_history(&self) {
        self.write("history.json", &self.history);
    }

    pub fn save_settings(&self) {
        self.write("settings.json", &self.settings);
    }

    pub fn save_all(&mut self) {
        self.save_bookmarks();
        self.save_history();
        self.save_settings();
    }

    /// Запоминает масштаб сайта; true — значение изменилось.
    pub fn set_site_zoom(&mut self, host: &str, zoom: f64) -> bool {
        let changed = if (zoom - 1.0).abs() < 0.005 {
            self.site_zoom.remove(host).is_some()
        } else {
            self.site_zoom.insert(host.to_string(), zoom) != Some(zoom)
        };
        if changed {
            self.write("zoom.json", &self.site_zoom);
        }
        changed
    }

    pub fn zoom_for(&self, host: &str) -> f64 {
        self.site_zoom.get(host).copied().unwrap_or(1.0)
    }

    /// Произвольный JSON-файл в папке данных (например, список загрузок).
    pub fn read_named<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        match &self.dir {
            Some(dir) => read_json(&dir.join(name)),
            None => T::default(),
        }
    }

    pub fn write_named<T: Serialize>(&self, name: &str, value: &T) {
        self.write(name, value);
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

    /// Добавляет закладки, которых ещё нет. Возвращает, сколько добавлено.
    pub fn add_bookmarks(&mut self, list: Vec<Bookmark>) -> usize {
        let mut added = 0;
        for bookmark in list {
            if !self.is_bookmarked(&bookmark.url) {
                self.bookmarks.push(bookmark);
                added += 1;
            }
        }
        added
    }

    pub fn remove_bookmark(&mut self, url: &str) {
        self.bookmarks.retain(|b| b.url != url);
    }

    pub fn add_visit(&mut self, url: &str, title: &str) {
        self.add_visit_at(url, title, now());
    }

    fn add_visit_at(&mut self, url: &str, title: &str, time: u64) {
        match self.pending.last_mut() {
            Some(last) if last.url == url => {
                last.time = time;
                if !title.is_empty() {
                    last.title = title.to_string();
                }
            }
            _ => self.pending.push(Visit { url: url.to_string(), title: title.to_string(), time }),
        }
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
        if let Some(visit) = self.pending.iter_mut().rev().find(|v| v.url == url) {
            visit.title = title.to_string();
        }
        if let Some(visit) = self.history.iter_mut().rev().find(|v| v.url == url) {
            visit.title = title.to_string();
        }
    }

    /// Удаление сразу пишется на диск: иначе слияние с диском вернуло бы запись.
    pub fn remove_visit(&mut self, url: &str, time: u64) {
        if let Some(dir) = &self.dir {
            self.history = read_json(&dir.join("history.json"));
        }
        self.history.retain(|v| !(v.url == url && v.time == time));
        self.pending.retain(|v| !(v.url == url && v.time == time));
        self.overwrite_history();
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.pending.clear();
        self.overwrite_history();
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

/// Объединяет два журнала: без повторов, по времени, обновление той же страницы
/// в пределах 10 минут считается одним посещением.
fn merge_history(disk: Vec<Visit>, memory: Vec<Visit>) -> Vec<Visit> {
    let mut all: Vec<Visit> = disk;
    for visit in memory {
        match all.iter_mut().find(|v| v.url == visit.url && v.time == visit.time) {
            Some(existing) => {
                if !visit.title.is_empty() {
                    existing.title = visit.title;
                }
            }
            None => all.push(visit),
        }
    }
    all.sort_by_key(|v| v.time);
    let mut merged: Vec<Visit> = Vec::with_capacity(all.len());
    for visit in all {
        if let Some(last) = merged.last_mut() {
            if last.url == visit.url && visit.time.saturating_sub(last.time) < 600 {
                last.time = visit.time;
                if !visit.title.is_empty() {
                    last.title = visit.title;
                }
                continue;
            }
        }
        merged.push(visit);
    }
    if merged.len() > MAX_HISTORY {
        let extra = merged.len() - MAX_HISTORY;
        merged.drain(..extra);
    }
    merged
}

fn read_json<T: DeserializeOwned + Default>(path: &PathBuf) -> T {
    fs::read_to_string(path)
        .ok()
        // Блокнот и PowerShell пишут UTF-8 с меткой BOM — без её снятия файл не прочитать
        .and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok())
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
    fn reads_files_saved_with_bom() {
        let dir = std::env::temp_dir().join(format!("rust-browser-bom-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("settings.json"), "\u{feff}{\"search\":\"yandex\"}").unwrap();
        assert_eq!(Store::load(dir.clone()).settings.search, "yandex");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn site_zoom_is_remembered_and_100_is_not_stored() {
        let mut store = Store::in_memory();
        assert_eq!(store.zoom_for("a.com"), 1.0);
        assert!(store.set_site_zoom("a.com", 1.25));
        assert!(!store.set_site_zoom("a.com", 1.25), "то же значение — без записи");
        assert_eq!(store.zoom_for("a.com"), 1.25);
        assert!(store.set_site_zoom("a.com", 1.0));
        assert!(store.site_zoom.is_empty());
    }

    #[test]
    fn import_skips_existing_bookmarks() {
        let mut store = Store::in_memory();
        store.toggle_bookmark("https://a.com/", "A");
        let list = vec![
            Bookmark { title: "A".into(), url: "https://a.com/".into() },
            Bookmark { title: "B".into(), url: "https://b.com/".into() },
            Bookmark { title: "B2".into(), url: "https://b.com/".into() },
        ];
        assert_eq!(store.add_bookmarks(list), 1);
        assert_eq!(store.bookmarks.len(), 2);
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
    fn two_windows_do_not_lose_each_others_history() {
        let dir = std::env::temp_dir().join(format!("rust-browser-merge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut first = Store::load(dir.clone());
        let mut second = Store::load(dir.clone());
        first.add_visit_at("https://a.com/", "A", 100);
        second.add_visit_at("https://b.com/", "B", 200);
        first.save_history();
        second.save_history();
        first.add_visit_at("https://c.com/", "C", 300);
        first.save_history();
        let urls: Vec<String> = Store::load(dir.clone()).history.into_iter().map(|v| v.url).collect();
        assert_eq!(urls, ["https://a.com/", "https://b.com/", "https://c.com/"]);

        // удалённое в одном окне не возвращается при сохранении из другого
        first.remove_visit("https://b.com/", 200);
        second.save_history();
        let urls: Vec<String> = Store::load(dir.clone()).history.into_iter().map(|v| v.url).collect();
        assert!(!urls.contains(&"https://b.com/".to_string()), "{urls:?}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn merge_folds_reloads_of_the_same_page() {
        let visit = |url: &str, time| Visit { url: url.into(), title: String::new(), time };
        let merged = merge_history(vec![visit("a", 100)], vec![visit("a", 300), visit("b", 400)]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].time, 300);
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
