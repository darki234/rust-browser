//! Автозаполнение адресов и банковских карт, проверка паролей на утечки.
//!
//! Адреса и карты лежат в autofill.json одним блоком, зашифрованным DPAPI.
//! Номер карты подставляется только после Windows Hello и только на HTTPS-сайт;
//! CVC не сохраняется никогда — его вводят руками, как и требуют банки.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Address {
    pub name: String,
    pub phone: String,
    pub email: String,
    pub country: String,
    pub city: String,
    pub street: String,
    pub zip: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Card {
    pub holder: String,
    pub number: String,
    /// ММ/ГГ
    pub month: String,
    pub year: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Profile {
    #[serde(default)]
    pub addresses: Vec<Address>,
    #[serde(default)]
    pub cards: Vec<Card>,
}

/// Файл на диске: весь профиль — один зашифрованный блок.
#[derive(Serialize, Deserialize, Default)]
pub struct Stored {
    #[serde(default)]
    pub data: String,
}

impl Profile {
    pub fn load(stored: &Stored) -> Profile {
        if stored.data.is_empty() {
            return Profile::default();
        }
        crate::passwords::unprotect(&stored.data)
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn store(&self) -> Option<Stored> {
        let json = serde_json::to_string(self).ok()?;
        Some(Stored { data: crate::passwords::protect(&json)? })
    }

    /// Для интерфейса: без полных номеров карт.
    pub fn summary(&self) -> Value {
        json!({
            "addresses": self.addresses.iter().map(|a| {
                let label = [a.name.as_str(), a.city.as_str(), a.street.as_str()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                json!({ "label": label, "address": a })
            }).collect::<Vec<_>>(),
            "cards": self.cards.iter().map(|c| json!({
                "label": format!("{} •••• {}", card_brand(&c.number), last4(&c.number)),
                "holder": c.holder,
                "expires": format!("{}/{}", c.month, c.year),
            })).collect::<Vec<_>>(),
        })
    }
}

pub fn digits(number: &str) -> String {
    number.chars().filter(char::is_ascii_digit).collect()
}

fn last4(number: &str) -> String {
    let d = digits(number);
    d[d.len().saturating_sub(4)..].to_string()
}

pub fn card_brand(number: &str) -> &'static str {
    let d = digits(number);
    match d.as_bytes().first() {
        Some(b'2') if d.starts_with("220") => "Мир",
        Some(b'4') => "Visa",
        Some(b'5') | Some(b'2') => "Mastercard",
        Some(b'3') => "Amex",
        Some(b'6') => "UnionPay",
        _ => "Карта",
    }
}

/// Проверка номера по алгоритму Луна — опечатку в номере видно сразу.
pub fn luhn_ok(number: &str) -> bool {
    let d = digits(number);
    if !(12..=19).contains(&d.len()) {
        return false;
    }
    let sum: u32 = d
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, b)| {
            let n = u32::from(b - b'0');
            if i % 2 == 1 { if n * 2 > 9 { n * 2 - 9 } else { n * 2 } } else { n }
        })
        .sum();
    sum % 10 == 0
}

/// Встраивается в страницы: есть ли на ней поля адреса или карты.
pub const DETECT_SCRIPT: &str = r#"(() => {
  if (window.top !== window || window.__rbAf) return;
  window.__rbAf = true;
  let sent = '';
  const kinds = () => {
    let card = false, address = false;
    for (const i of document.querySelectorAll('input, select')) {
      const hint = ((i.autocomplete || '') + ' ' + (i.name || '') + ' ' + (i.id || '')).toLowerCase();
      if (/cc-|card.?num|cardnumber|ccnum|cc_num/.test(hint)) card = true;
      if (/street|address-line|postal|zip|shipping|billing|city|tel|phone/.test(hint)) address = true;
    }
    return (card ? 'c' : '') + (address ? 'a' : '');
  };
  const scan = () => {
    const k = kinds();
    if (k && k !== sent) {
      sent = k;
      try { window.chrome.webview.postMessage(JSON.stringify({ cmd: 'af_form', card: k.includes('c'), address: k.includes('a') })); } catch (e) {}
    }
  };
  new MutationObserver(() => { clearTimeout(window.__rbAfT); window.__rbAfT = setTimeout(scan, 300); })
    .observe(document, { childList: true, subtree: true });
  document.addEventListener('DOMContentLoaded', scan);
  scan();
})();"#;

/// Заполняет форму: сначала по autocomplete, потом по имени и подписи поля.
/// Только на сайте `origin` — тот, что был на экране, когда попросили подставить
/// (карта подставляется после Windows Hello, вкладка за это время могла уйти).
pub fn fill_script(origin: &str, fields: &Value) -> String {
    let origin = Value::from(origin);
    format!(
        r#"((d) => {{
  if (window.top !== window || location.origin !== {origin}) return 0;
  const visible = (i) => !!(i.offsetWidth || i.offsetHeight || i.getClientRects().length);
  const put = (input, value) => {{
    if (!value) return;
    if (input.tagName === 'SELECT') {{
      const opt = [...input.options].find((o) => o.value === value || o.textContent.trim() === value || o.value === value.replace(/^0/, ''));
      if (!opt) return;
      input.value = opt.value;
    }} else {{
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
      input.dispatchEvent(new Event('input', {{ bubbles: true }}));
    }}
    input.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }};
  const rules = [
    ['cc-number', /card.?num|cardnumber|ccnum|cc_num|номер.?карт/], ['cc-name', /card.?holder|cc-?name|name.?on.?card|владел/],
    ['cc-exp', /cc-?exp$|expir|exp.?date|срок/], ['cc-exp-month', /exp.?m|month|мес/], ['cc-exp-year', /exp.?y|year|год/],
    ['name', /full.?name|^name$|fio|фио|имя/], ['tel', /tel|phone|телефон/], ['email', /e-?mail|почт/],
    ['country', /country|стран/], ['address-level2', /city|town|город/], ['street-address', /street|address|адрес|улиц/],
    ['postal-code', /postal|zip|индекс/],
  ];
  let filled = 0;
  for (const input of document.querySelectorAll('input, select')) {{
    if (!visible(input) || input.type === 'hidden' || input.type === 'password') continue;
    const ac = (input.autocomplete || '').toLowerCase().split(' ').pop();
    const hint = [input.name, input.id, input.placeholder, input.getAttribute('aria-label'), input.labels?.[0]?.textContent].join(' ').toLowerCase();
    let key = rules.find(([k]) => k === ac)?.[0] || (ac === 'address-line1' ? 'street-address' : '');
    if (!key) key = rules.find(([k, re]) => k in d && re.test(hint))?.[0];
    if (key && d[key]) {{ put(input, d[key]); filled++; }}
  }}
  return filled;
}})({fields})"#
    )
}

pub fn address_fields(a: &Address) -> Value {
    json!({
        "name": a.name, "tel": a.phone, "email": a.email, "country": a.country,
        "address-level2": a.city, "street-address": a.street, "postal-code": a.zip,
    })
}

pub fn card_fields(c: &Card) -> Value {
    json!({
        "cc-number": digits(&c.number), "cc-name": c.holder,
        "cc-exp": format!("{}/{}", c.month, c.year), "cc-exp-month": c.month, "cc-exp-year": c.year,
    })
}

// ---------- проверка паролей ----------

/// Итог проверки одного пароля.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Health {
    /// Сколько раз пароль встречался в утечках (0 — не встречался, -1 — не проверили).
    pub leaked: i64,
    /// Тот же пароль стоит ещё на каком-то сайте.
    pub reused: bool,
    /// Короткий или простой.
    pub weak: bool,
}

pub fn sha1_hex(password: &str) -> String {
    Sha1::digest(password.as_bytes()).iter().map(|b| format!("{b:02X}")).collect()
}

pub fn weak(password: &str) -> bool {
    let len = password.chars().count();
    let kinds = [
        password.chars().any(|c| c.is_lowercase()),
        password.chars().any(|c| c.is_uppercase()),
        password.chars().any(|c| c.is_ascii_digit()),
        password.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|k| **k)
    .count();
    len < 8 || (len < 12 && kinds < 2)
}

/// Утечки — через Have I Been Pwned по k-анонимности: наружу уходят только первые
/// 5 знаков SHA-1, сравнение остальных — здесь. Сам пароль никуда не отправляется.
pub fn check(passwords: &[String]) -> Vec<Health> {
    let mut cache: std::collections::HashMap<String, Option<String>> = Default::default();
    passwords
        .iter()
        .map(|password| {
            let hash = sha1_hex(password);
            let (prefix, suffix) = hash.split_at(5);
            let body = cache.entry(prefix.to_string()).or_insert_with(|| range(prefix));
            let leaked = match body {
                Some(text) => text
                    .lines()
                    .find_map(|line| {
                        let (s, count) = line.trim().split_once(':')?;
                        (s.eq_ignore_ascii_case(suffix)).then(|| count.trim().parse::<i64>().unwrap_or(1))
                    })
                    .unwrap_or(0),
                None => -1,
            };
            Health { leaked, reused: passwords.iter().filter(|p| *p == password).count() > 1, weak: weak(password) }
        })
        .collect()
}

fn range(prefix: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("curl.exe")
        .args(["-sSf", "--max-time", "20", "-H", "Add-Padding: true", "-A", "RustBrowser-password-check"])
        .arg(format!("https://api.pwnedpasswords.com/range/{prefix}"))
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luhn_and_brand() {
        assert!(luhn_ok("4111 1111 1111 1111"));
        assert!(!luhn_ok("4111 1111 1111 1112"));
        assert_eq!(card_brand("2200 1234"), "Мир");
        assert_eq!(card_brand("5500 0000"), "Mastercard");
        assert_eq!(last4("4111-1111-1111-1234"), "1234");
    }

    #[test]
    fn sha1_and_weakness() {
        assert_eq!(sha1_hex("password"), "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8");
        assert!(weak("qwerty"));
        assert!(weak("12345678901"));
        assert!(!weak("Rust-Browser-2026"));
    }

    #[test]
    fn profile_round_trip_is_encrypted() {
        let profile = Profile {
            addresses: vec![Address { name: "Иван".into(), city: "Москва".into(), ..Default::default() }],
            cards: vec![Card { holder: "IVAN".into(), number: "4111111111111111".into(), month: "12".into(), year: "29".into() }],
        };
        let stored = profile.store().unwrap();
        assert!(!stored.data.contains("4111"));
        assert_eq!(Profile::load(&stored), profile);
        let summary = profile.summary().to_string();
        assert!(summary.contains("•••• 1111") && !summary.contains("411111"));
    }

    #[test]
    fn fill_script_is_bound_to_origin() {
        let card = Card { holder: "IVAN".into(), number: "4111111111111111".into(), month: "12".into(), year: "29".into() };
        let script = fill_script("https://shop.ru\"; alert(1); \"", &card_fields(&card));
        // адрес попадает в скрипт JSON-строкой и проверяется до подстановки
        assert!(script.contains(r#"location.origin !== "https://shop.ru\"; alert(1); \"") return 0;"#));
        assert!(script.find("location.origin").unwrap() < script.find("put(input").unwrap());
    }
}
