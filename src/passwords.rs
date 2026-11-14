//! Свой менеджер паролей.
//!
//! Защита:
//! * пароли на диске зашифрованы DPAPI — расшифровать их может только эта учётная
//!   запись Windows на этом компьютере (плюс собственная «соль» браузера);
//! * в интерфейс и на сайты уходят только адрес и логин; пароль подставляется
//!   в страницу лишь по щелчку в интерфейсе браузера и только на тот же сайт (origin),
//!   причём сайт должен открываться по HTTPS;
//! * показать или скопировать пароль можно только после Windows Hello (ПИН, палец, лицо).

use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Login {
    /// https://example.com[:port] — пароль подставляется только сюда.
    pub origin: String,
    pub username: String,
    /// Пароль, зашифрованный DPAPI, в шестнадцатеричном виде.
    pub secret: String,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub used: u64,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Vault {
    #[serde(default)]
    pub logins: Vec<Login>,
    /// Сайты, для которых пользователь выбрал «Никогда не сохранять».
    #[serde(default)]
    pub never: Vec<String>,
}

impl Vault {
    pub fn for_origin<'a>(&'a self, origin: &'a str) -> impl Iterator<Item = &'a Login> + 'a {
        self.logins.iter().filter(move |l| l.origin == origin)
    }

    /// Уже сохранён ровно такой логин с таким же паролем?
    pub fn has_same(&self, origin: &str, username: &str, password: &str) -> bool {
        self.for_origin(origin)
            .any(|l| l.username == username && unprotect(&l.secret).as_deref() == Some(password))
    }

    /// Сохраняет или обновляет пароль; false — не получилось зашифровать.
    pub fn upsert(&mut self, origin: &str, username: &str, password: &str, now: u64) -> bool {
        let Some(secret) = protect(password) else { return false };
        match self.logins.iter_mut().find(|l| l.origin == origin && l.username == username) {
            Some(login) => {
                login.secret = secret;
                login.used = now;
            }
            None => self.logins.push(Login {
                origin: origin.to_string(),
                username: username.to_string(),
                secret,
                created: now,
                used: now,
            }),
        }
        true
    }
}

/// Origin страницы, на которой разрешено сохранять и подставлять пароли:
/// только HTTPS (и http://localhost для своих разработок). Логин и пароль
/// в самом адресе (https://bank.ru@evil.com) отбрасываются — важен настоящий хост.
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?.to_ascii_lowercase();
    if host_port.is_empty() || host_port.contains(char::is_whitespace) {
        return None;
    }
    // [::1]:3000 — у IPv6 двоеточия внутри скобок
    let host = match host_port.strip_prefix('[') {
        Some(rest) => &host_port[..rest.find(']').map_or(host_port.len(), |i| i + 2)],
        None => host_port.split(':').next().unwrap_or(""),
    };
    let local = matches!(host, "localhost" | "127.0.0.1" | "[::1]") || host.ends_with(".localhost");
    match scheme.as_str() {
        "https" => Some(format!("https://{}", host_port.trim_end_matches(":443"))),
        "http" if local => Some(format!("http://{host_port}")),
        _ => None,
    }
}

/// Имя сайта для интерфейса: «example.com» без схемы.
pub fn display_host(origin: &str) -> &str {
    origin.split_once("://").map(|(_, h)| h).unwrap_or(origin)
}

// ---------- шифрование ----------

const ENTROPY: &[u8] = b"RustBrowser.passwords.v1";

pub fn protect(text: &str) -> Option<String> {
    dpapi(text.as_bytes(), true).map(|bytes| hex(&bytes))
}

pub fn unprotect(secret: &str) -> Option<String> {
    let bytes = unhex(secret)?;
    String::from_utf8(dpapi(&bytes, false)?).ok()
}

fn dpapi(data: &[u8], encrypt: bool) -> Option<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let entropy = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
        let mut output = CRYPT_INTEGER_BLOB::default();
        let result = if encrypt {
            CryptProtectData(&input, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        } else {
            CryptUnprotectData(&input, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        };
        result.ok()?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Some(bytes)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

// ---------- Windows Hello ----------

/// Чем закончилась проверка личности.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    Verified,
    Canceled,
    /// Windows Hello не настроен (нет ПИН-кода) или запрещён политикой.
    Unavailable,
}

/// Спрашивает Windows Hello (ПИН, отпечаток, лицо) поверх окна браузера.
/// Ответ приходит в `done` из другого потока.
pub fn verify(hwnd: isize, message: &str, done: impl FnOnce(Verdict) + Send + 'static) {
    use windows::core::{factory, HSTRING};
    use windows_future::IAsyncOperation;
    use windows::Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;

    let operation: windows::core::Result<IAsyncOperation<UserConsentVerificationResult>> = (|| unsafe {
        let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()?;
        interop.RequestVerificationForWindowAsync(HWND(hwnd as *mut _), &HSTRING::from(message))
    })();
    let Ok(operation) = operation else {
        done(Verdict::Unavailable);
        return;
    };
    std::thread::spawn(move || {
        let verdict = match operation.get() {
            Ok(UserConsentVerificationResult::Verified) => Verdict::Verified,
            Ok(UserConsentVerificationResult::Canceled | UserConsentVerificationResult::RetriesExhausted) => {
                Verdict::Canceled
            }
            Ok(_) | Err(_) => Verdict::Unavailable,
        };
        done(verdict);
    });
}

// ---------- перенос из других браузеров ----------

/// Пароли из CSV, который выгружают Chrome, Edge, Яндекс и Firefox:
/// столбцы ищем по заголовку (url/origin, username/login, password).
pub fn parse_csv(text: &str) -> Vec<(String, String, String)> {
    let rows = csv_rows(text.trim_start_matches('\u{feff}'));
    let Some((header, rows)) = rows.split_first() else { return Vec::new() };
    let column = |names: &[&str]| {
        header.iter().position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()))
    };
    let (Some(url), Some(user), Some(pass)) = (
        column(&["url", "origin", "website", "login_uri"]),
        column(&["username", "login", "user", "login_username"]),
        column(&["password", "login_password"]),
    ) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let field = |i: usize| row.get(i).cloned().unwrap_or_default();
            let password = field(pass);
            (!password.is_empty()).then(|| (field(url), field(user), password))
        })
        .collect()
}

/// Простой разбор CSV по RFC 4180: кавычки, удвоенные кавычки, переносы внутри поля.
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                if row.iter().any(|f| !f.is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

// ---------- скрипты страницы ----------

/// Встраивается во все страницы: замечает формы входа и отправку пароля.
/// Сам пароль браузеру сообщается только при отправке формы — для предложения сохранить.
pub const PAGE_SCRIPT: &str = r#"(() => {
  if (window.top !== window || window.__rbpw) return;
  window.__rbpw = true;
  const post = (m) => { try { window.chrome.webview.postMessage(JSON.stringify(m)); } catch (e) {} };
  const visible = (i) => !!(i.offsetWidth || i.offsetHeight || i.getClientRects().length);
  const userField = (pw) => {
    const scope = pw.form || document;
    let found = null;
    for (const i of scope.querySelectorAll('input')) {
      if (i === pw) break;
      if (/^(text|email|tel|)$/.test(i.type) && visible(i)) found = i;
    }
    return found;
  };
  // логин из прошлого шага (Google, Microsoft: сначала адрес, потом пароль)
  document.addEventListener('change', (e) => {
    const i = e.target;
    if (i instanceof HTMLInputElement && /^(text|email|tel)$/.test(i.type) && i.value && /user|login|mail|phone|name|identifier/i.test(i.name + i.id + i.autocomplete + i.type)) {
      try { sessionStorage.setItem('__rb_user', i.value); } catch (e) {}
    }
  }, true);
  let announced = false;
  const scan = () => {
    if (!announced && document.querySelector('input[type=password]')) {
      announced = true;
      post({ cmd: 'pw_form' });
    }
  };
  let sentKey = '';
  const capture = () => {
    const pw = [...document.querySelectorAll('input[type=password]')].find((p) => p.value && visible(p));
    if (!pw) return;
    // форма регистрации или смены пароля: два поля пароля — сохраняем новый
    const u = userField(pw);
    let username = u ? u.value : '';
    if (!username) { try { username = sessionStorage.getItem('__rb_user') || ''; } catch (e) {} }
    const key = username + '\n' + pw.value;
    if (key === sentKey) return;
    sentKey = key;
    post({ cmd: 'pw_capture', username, password: pw.value });
  };
  document.addEventListener('submit', capture, true);
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && e.target instanceof HTMLInputElement) setTimeout(capture, 0);
  }, true);
  document.addEventListener('click', (e) => {
    if (e.target instanceof Element && e.target.closest('button, input[type=submit], input[type=button], [role=button]')) setTimeout(capture, 0);
  }, true);
  new MutationObserver(scan).observe(document, { childList: true, subtree: true });
  document.addEventListener('DOMContentLoaded', scan);
  scan();
})();"#;

/// Подставляет логин и пароль в форму. Функция целиком внутри скрипта, а не в
/// странице: сайт не может её подменить, чтобы перехватить пароль чужого сайта.
/// `origin` проверяется ещё раз уже внутри страницы: если между щелчком и запуском
/// скрипта вкладка успела уйти на другой сайт, пароль туда не попадёт
/// (location подменить страница не может).
pub fn fill_script(origin: &str, username: &str, password: &str) -> String {
    let data = json!({ "o": origin, "u": username, "p": password });
    format!(
        r#"((d) => {{
  if (window.top !== window || location.origin !== d.o) return false;
  const visible = (i) => !!(i.offsetWidth || i.offsetHeight || i.getClientRects().length);
  const put = (input, value) => {{
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    input.focus();
    setter.call(input, value);
    input.dispatchEvent(new Event('input', {{ bubbles: true }}));
    input.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }};
  const inputs = [...document.querySelectorAll('input')].filter(visible);
  const pw = inputs.find((i) => i.type === 'password');
  let user = null;
  for (const i of inputs) {{
    if (i === pw) break;
    if (/^(text|email|tel|)$/.test(i.type)) user = i;
  }}
  if (!user && !pw) user = inputs.find((i) => /^(text|email|tel)$/.test(i.type));
  if (user && d.u) put(user, d.u);
  if (pw) put(pw, d.p);
  return !!(user || pw);
}})({data})"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_requires_https_and_ignores_userinfo() {
        assert_eq!(origin_of("https://Example.com/login?x=1").as_deref(), Some("https://example.com"));
        assert_eq!(origin_of("https://example.com:443/").as_deref(), Some("https://example.com"));
        assert_eq!(origin_of("https://example.com:8443/a").as_deref(), Some("https://example.com:8443"));
        assert_eq!(origin_of("https://bank.ru@evil.com/").as_deref(), Some("https://evil.com"));
        assert_eq!(origin_of("http://example.com/"), None);
        assert_eq!(origin_of("http://localhost:3000/x").as_deref(), Some("http://localhost:3000"));
        assert_eq!(origin_of("file:///C:/a.html"), None);
        assert_eq!(origin_of("about:blank"), None);
        assert_eq!(origin_of("http://[::1]:3000/").as_deref(), Some("http://[::1]:3000"));
        assert_eq!(origin_of("http://[::2]:3000/"), None);
        assert_eq!(origin_of("http://localhost.evil.com/"), None);
    }

    #[test]
    fn fill_script_checks_origin_inside_page_and_escapes_data() {
        let script = fill_script("https://bank.ru", "bob\"</script>", "p'\"\\\u{2028}x");
        // проверка адреса — внутри самой страницы, до подстановки
        let check = script.find("location.origin !== d.o").unwrap();
        assert!(check < script.find("put(pw").unwrap());
        // данные — одним JSON-литералом: кавычки не разрывают скрипт
        let data = script.rsplit_once("})(").unwrap().1.trim_end_matches(')');
        let parsed: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(parsed["o"], "https://bank.ru");
        assert_eq!(parsed["u"], "bob\"</script>");
        assert_eq!(parsed["p"], "p'\"\\\u{2028}x");
    }

    #[test]
    fn dpapi_round_trip() {
        let secret = protect("пароль 123").unwrap();
        // в шифре нет самого пароля (в hex он выглядел бы так); «123» в шифре может встретиться случайно
        assert!(!secret.contains(&hex("пароль 123".as_bytes())));
        assert_eq!(unprotect(&secret).as_deref(), Some("пароль 123"));
        assert_eq!(unprotect("zz"), None);
    }

    #[test]
    fn csv_from_chrome_and_firefox() {
        let chrome = "name,url,username,password,note\r\nex,https://ex.com/,bob,\"p,a\"\"ss\",\r\n";
        assert_eq!(parse_csv(chrome), vec![("https://ex.com/".into(), "bob".into(), "p,a\"ss".into())]);
        let firefox = "\"url\",\"username\",\"password\",\"httpRealm\"\n\"https://a.org\",\"amy\",\"x\ny\",\"\"\n";
        assert_eq!(parse_csv(firefox), vec![("https://a.org".into(), "amy".into(), "x\ny".into())]);
        assert!(parse_csv("a,b\n1,2\n").is_empty());
    }

    #[test]
    fn upsert_updates_existing() {
        let mut vault = Vault::default();
        assert!(vault.upsert("https://a.org", "amy", "one", 1));
        assert!(vault.upsert("https://a.org", "amy", "two", 2));
        assert_eq!(vault.logins.len(), 1);
        assert!(vault.has_same("https://a.org", "amy", "two"));
        assert!(!vault.has_same("https://b.org", "amy", "two"));
    }
}
