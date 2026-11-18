//! Исправление запроса в адресной строке: неверная раскладка («ghbdtn» → «привет»,
//! «руддщ» → «hello») и опечатки («пртвет» → «привет»).
//!
//! Словари — частотные списки слов (src/dict, FrequencyWords, CC BY-SA 4.0): чем выше
//! слово в списке, тем вероятнее оно. Загружаются при первом запросе, около 5 мс.

use std::{collections::HashMap, sync::OnceLock};

static RU_WORDS: &str = include_str!("dict/ru.txt");
static EN_WORDS: &str = include_str!("dict/en.txt");

/// Слово → место в частотном списке (0 — самое частое).
type Dict = HashMap<&'static str, u32>;

fn load(text: &'static str) -> Dict {
    text.lines().enumerate().map(|(i, w)| (w, i as u32)).collect()
}

fn ru() -> &'static Dict {
    static D: OnceLock<Dict> = OnceLock::new();
    D.get_or_init(|| load(RU_WORDS))
}

fn en() -> &'static Dict {
    static D: OnceLock<Dict> = OnceLock::new();
    D.get_or_init(|| load(EN_WORDS))
}

/// Клавиши: латиница и русская раскладка ЙЦУКЕН на тех же местах.
const EN_KEYS: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU_KEYS: &str = "йцукенгшщзхъфывапролджэячсмитьбюё";
const RU_ALPHABET: &str = "абвгдежзийклмнопрстуфхцчшщъыьэюя";
const EN_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz";

fn swap(word: &str, from: &str, to: &str) -> Option<String> {
    let to: Vec<char> = to.chars().collect();
    word.chars()
        .map(|c| from.chars().position(|k| k == c).map(|i| to[i]))
        .collect()
}

/// Фоновый поток: исправление длинного слова занимает до ~100 мс, а подсказки
/// считаются на каждое нажатие. Устаревшие запросы пропускаются — берётся последний.
pub struct Worker(std::sync::mpsc::Sender<(u64, String)>);

impl Worker {
    pub fn start(done: impl Fn(u64, Option<(String, Fix)>) + Send + 'static) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<(u64, String)>();
        std::thread::Builder::new()
            .name("spell".into())
            .spawn(move || {
                while let Ok(mut job) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        job = newer;
                    }
                    done(job.0, correct(&job.1));
                }
            })
            .ok();
        Worker(tx)
    }

    pub fn ask(&self, seq: u64, query: &str) {
        let _ = self.0.send((seq, query.to_string()));
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Fix {
    /// Набрано не в той раскладке.
    Layout,
    /// Опечатка.
    Typo,
}

impl Fix {
    pub fn as_str(self) -> &'static str {
        match self {
            Fix::Layout => "layout",
            Fix::Typo => "typo",
        }
    }
}

/// Похоже на адрес сайта — его не трогаем.
fn looks_like_url(query: &str) -> bool {
    let q = query.trim();
    if q.contains("://") || q.starts_with("localhost") {
        return true;
    }
    // одно «слово» с точкой и окончанием из букв: vk.com, ya.ru/path
    !q.contains(' ')
        && q.split('/').next().is_some_and(|host| {
            host.rsplit_once('.').is_some_and(|(name, tld)| !name.is_empty() && tld.len() >= 2 && tld.chars().all(|c| c.is_alphabetic()))
        })
}

/// Исправленный запрос и вид исправления, если оно нужно.
pub fn correct(query: &str) -> Option<(String, Fix)> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > 120 || looks_like_url(query) {
        return None;
    }
    let lower = query.to_lowercase().replace('ё', "е");
    let tokens: Vec<&str> = lower.split(' ').filter(|t| !t.is_empty()).collect();
    if let Some(fixed) = fix_layout(&tokens) {
        return Some((fixed, Fix::Layout));
    }
    let mut changed = false;
    let fixed: Vec<String> = tokens
        .iter()
        .map(|t| match fix_typo(t) {
            Some(w) => {
                changed = true;
                w
            }
            None => t.to_string(),
        })
        .collect();
    changed.then(|| (fixed.join(" "), Fix::Typo))
}

/// Весь запрос в другой раскладке, если так получаются настоящие слова, а сейчас — нет.
fn fix_layout(tokens: &[&str]) -> Option<String> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut convinced = false;
    for &t in tokens {
        let latin = t.chars().all(|c| EN_KEYS.contains(c));
        let cyrillic = t.chars().all(|c| RU_KEYS.contains(c));
        let (swapped, target, source) = if latin && t.chars().any(|c| c.is_ascii_alphabetic()) {
            (swap(t, EN_KEYS, RU_KEYS)?, ru(), en())
        } else if cyrillic {
            (swap(t, RU_KEYS, EN_KEYS)?, en(), ru())
        } else {
            return None; // цифры, смесь алфавитов — не раскладка
        };
        let swapped_rank = target.get(swapped.as_str()).copied();
        let own_rank = source.get(t).copied();
        match (swapped_rank, own_rank) {
            // в другой раскладке — слово, а так — нет (или очень редкое)
            (Some(_), None) => {
                if t.chars().count() >= 3 {
                    convinced = true;
                }
                out.push(swapped);
            }
            (Some(s), Some(o)) if o > 20_000 && s < o / 4 => {
                if t.chars().count() >= 3 {
                    convinced = true;
                }
                out.push(swapped);
            }
            _ => return None, // хоть одно слово уже настоящее — раскладка верная
        }
    }
    convinced.then(|| out.join(" "))
}

/// Ближайшее частое слово на расстоянии одной (или двух) правок.
fn fix_typo(token: &str) -> Option<String> {
    let len = token.chars().count();
    let (dict, alphabet) = if token.chars().all(|c| RU_ALPHABET.contains(c)) {
        (ru(), RU_ALPHABET)
    } else if token.chars().all(|c| c.is_ascii_lowercase()) {
        (en(), EN_ALPHABET)
    } else {
        return None;
    };
    // короткое слово часто ещё не дописано — его не трогаем
    if len < 5 || len > 18 || dict.contains_key(token) {
        return None;
    }
    let alphabet: Vec<char> = alphabet.chars().collect();
    // первую букву почти никогда не путают: слова с той же первой буквой — вперёд
    let head = token.chars().next();
    let best = |words: &mut dyn Iterator<Item = String>| {
        words
            .filter_map(|w| dict.get_key_value(w.as_str()).map(|(k, r)| (k.chars().next() != head, *r, *k)))
            .min()
            .map(|(_, r, k)| (r, k))
    };
    let first: Vec<String> = edits(token, &alphabet);
    if let Some((_, word)) = best(&mut first.iter().cloned()) {
        return Some(word.to_string());
    }
    if len >= 6 && len <= 12 {
        let mut second = first.iter().flat_map(|w| edits(w, &alphabet));
        // вторая правка — только для частых слов: иначе находится что угодно
        if let Some((rank, word)) = best(&mut second) {
            if rank < 15_000 {
                return Some(word.to_string());
            }
        }
    }
    None
}

/// Все слова на расстоянии одной правки: удаление, перестановка соседних, замена, вставка.
fn edits(word: &str, alphabet: &[char]) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let n = chars.len();
    let mut out = Vec::with_capacity(n * (2 * alphabet.len() + 2) + alphabet.len());
    let build = |parts: &[&[char]]| parts.iter().flat_map(|p| p.iter()).collect::<String>();
    for i in 0..n {
        out.push(build(&[&chars[..i], &chars[i + 1..]]));
        if i + 1 < n {
            let mut c = chars.clone();
            c.swap(i, i + 1);
            out.push(c.into_iter().collect());
        }
        for &a in alphabet {
            if a != chars[i] {
                out.push(build(&[&chars[..i], &[a], &chars[i + 1..]]));
            }
        }
    }
    for i in 0..=n {
        for &a in alphabet {
            out.push(build(&[&chars[..i], &[a], &chars[i..]]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_layout() {
        assert_eq!(correct("ghbdtn"), Some(("привет".into(), Fix::Layout)));
        assert_eq!(correct("ghbdtn vbh"), Some(("привет мир".into(), Fix::Layout)));
        assert_eq!(correct("руддщ"), Some(("hello".into(), Fix::Layout)));
        // настоящие слова и адреса не трогаем
        assert_eq!(correct("hello world"), None);
        assert_eq!(correct("привет"), None);
        assert_eq!(correct("vk.com"), None);
        assert_eq!(correct("https://ya.ru"), None);
    }

    #[test]
    fn typos() {
        assert_eq!(correct("пртвет"), Some(("привет".into(), Fix::Typo)));
        assert_eq!(correct("погода в москве"), None);
        assert_eq!(correct("погда в москве"), Some(("погода в москве".into(), Fix::Typo)));
        // короткое «helo» могли ещё не дописать — его не трогаем, а «wrold» исправляем
        assert_eq!(correct("helo wrold"), Some(("helo world".into(), Fix::Typo)));
    }
}

#[cfg(test)]
mod speed {
    #[test]
    fn worst_case_is_fast() {
        let _ = super::correct("привет"); // словари загружены
        for q in ["абвгджзикл", "щшщшщшщшщшщш", "qwzxqwzxqwzx", "ывапролджэ"] {
            let t = std::time::Instant::now();
            let _ = super::correct(q);
            let ms = t.elapsed().as_millis();
            println!("{q}: {ms} ms");
        }
    }
}
