//! Синхронизация Rust Browser: телефон и компьютеры одной «связки».
//!
//! Связка — это случайный ключ AES-256 и её номер. Создаётся на компьютере одной кнопкой,
//! на телефон (или другой компьютер) переходит по QR-коду — без аккаунтов и паролей.
//! Доставка двумя путями, данные всегда зашифрованы ключом связки (AES-256-GCM):
//! * по Wi‑Fi напрямую: компьютер слушает порт в домашней сети, телефон приходит к нему
//!   со своими данными и уносит слитые — изменения за секунды и без облака;
//! * через WebDAV (Яндекс Диск, Nextcloud…): один зашифрованный файл на диске — когда
//!   телефон не дома или компьютер выключен. Диск видит только шум.
//!
//! Слияние трёхстороннее, и «точка отсчёта» своя для каждого собеседника (диск, каждый
//! телефон): иначе свежие правки, пришедшие по Wi‑Fi, затирались бы старым файлом с диска.

use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, TcpListener, TcpStream, UdpSocket},
    os::windows::process::CommandExt,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

pub const FORMAT: &str = "rust-browser-sync/2";
/// Порт в домашней сети (TCP — обмен, UDP — поиск компьютера телефоном).
pub const LAN_PORT: u16 = 47801;
const PAIR_PREFIX: &str = "RBSYNC1.";

/// Синхронизация на этом компьютере (account.json в папке данных).
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Account {
    /// Номер связки — одинаковый у всех её устройств.
    #[serde(default)]
    pub group: String,
    /// Ключ связки под DPAPI (hex).
    #[serde(default)]
    pub key: String,
    /// Номер этого компьютера в связке.
    #[serde(default)]
    pub device: String,
    /// Когда последний раз что-то синхронизировалось (секунды Unix).
    #[serde(default)]
    pub synced: u64,
    /// Отпечатки разделов после прошлого обмена — отдельно для каждого собеседника.
    #[serde(default)]
    pub bases: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    pub webdav: Option<WebDav>,
    /// Телефоны, приходившие по Wi‑Fi: номер → имя и когда были.
    #[serde(default)]
    pub peers: BTreeMap<String, Peer>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct WebDav {
    pub url: String,
    pub user: String,
    /// Пароль приложения под DPAPI.
    pub pass: String,
    /// Метка версии файла на диске после прошлого обмена — чтобы зря не скачивать.
    #[serde(default)]
    pub etag: String,
    #[serde(default)]
    pub synced: u64,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Peer {
    pub name: String,
    pub seen: u64,
}

/// Что синхронизируется. Каждый раздел — произвольный JSON, сливает его main.rs.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Bundle {
    #[serde(default)]
    pub sections: BTreeMap<String, Value>,
    /// Кто записал: имя и номер устройства, время.
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub time: u64,
}

/// Зашифрованный пакет: открыт только номер связки.
#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    group: String,
    nonce: String,
    data: String,
}

/// Код для QR: всё, что нужно телефону, чтобы войти в связку.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Pair {
    /// номер связки
    pub g: String,
    /// ключ (base64)
    pub k: String,
    /// имя компьютера
    #[serde(default)]
    pub n: String,
    /// адреса компьютера в домашней сети и порт
    #[serde(default)]
    pub a: Vec<String>,
    #[serde(default)]
    pub p: u16,
    /// WebDAV, если подключён: адрес, логин, пароль приложения
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<PairDav>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct PairDav {
    pub u: String,
    pub l: String,
    pub p: String,
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    let mut bytes = [0u8; N];
    unsafe {
        let _ = BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    }
    bytes
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn encrypt(bundle: &Bundle, key: &[u8; 32], group: &str) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = random_bytes::<12>();
    let plain = serde_json::to_vec(bundle).map_err(|e| e.to_string())?;
    let sealed = cipher.encrypt(Nonce::from_slice(&nonce), plain.as_slice()).map_err(|_| "не удалось зашифровать")?;
    let envelope = Envelope {
        format: FORMAT.into(),
        group: group.into(),
        nonce: crate::base64(&nonce),
        data: crate::base64(&sealed),
    };
    serde_json::to_vec(&envelope).map_err(|e| e.to_string())
}

pub fn decrypt(bytes: &[u8], key: &[u8; 32]) -> Result<Bundle, String> {
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| "файл синхронизации повреждён")?;
    if envelope.format != FORMAT {
        return Err("данные от другой версии браузера — обновите его".into());
    }
    let nonce = crate::base64_decode(&envelope.nonce).filter(|n| n.len() == 12).ok_or("файл повреждён")?;
    let sealed = crate::base64_decode(&envelope.data).ok_or("файл повреждён")?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce), sealed.as_slice())
        .map_err(|_| "чужой ключ — устройство не из этой связки")?;
    serde_json::from_slice(&plain).map_err(|e| e.to_string())
}

// ---------- код для QR ----------

pub fn pair_code(pair: &Pair) -> String {
    let json = serde_json::to_vec(pair).unwrap_or_default();
    let b64 = crate::base64(&json).replace('+', "-").replace('/', "_");
    format!("{PAIR_PREFIX}{}", b64.trim_end_matches('='))
}

pub fn parse_pair(text: &str) -> Option<Pair> {
    let body = text.trim().strip_prefix(PAIR_PREFIX)?;
    let mut b64 = body.replace('-', "+").replace('_', "/");
    while b64.len() % 4 != 0 {
        b64.push('=');
    }
    let pair: Pair = serde_json::from_slice(&crate::base64_decode(&b64)?).ok()?;
    let key = crate::base64_decode(&pair.k)?;
    (key.len() == 32 && !pair.g.is_empty()).then_some(pair)
}

/// QR-код картинкой SVG (цвета задаёт интерфейс).
pub fn qr_svg(text: &str) -> String {
    use qrcode::{render::svg, EcLevel, QrCode};
    let Ok(code) = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::L) else { return String::new() };
    code.render::<svg::Color>()
        .min_dimensions(220, 220)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .quiet_zone(true)
        .build()
}

/// Адрес этого компьютера в домашней сети (пакеты при этом никуда не уходят).
pub fn local_ips() -> Vec<String> {
    let mut ips = Vec::new();
    for probe in ["192.168.0.1:80", "10.0.0.1:80", "8.8.8.8:80"] {
        if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
            if socket.connect(probe).is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    let ip = addr.ip().to_string();
                    if !addr.ip().is_loopback() && !addr.ip().is_unspecified() && !ips.contains(&ip) {
                        ips.push(ip);
                    }
                }
            }
        }
    }
    ips
}

// ---------- WebDAV ----------

/// Файл связки на диске: в папке по адресу, имя по номеру связки.
pub fn dav_file(url: &str, group: &str) -> String {
    format!("{}/rust-browser-{group}.rbsync", url.trim().trim_end_matches('/'))
}

/// curl с логином и паролем через stdin — в списке процессов их не видно.
fn dav_curl(args: &[&str], user: &str, pass: &str) -> Result<(Vec<u8>, u16, String), String> {
    let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut child = Command::new("curl.exe")
        .args(["-s", "-g", "--proto", "=https,http", "--max-time", "20", "-K", "-", "-D", "-"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|_| "нет curl.exe")?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = write!(stdin, "user = \"{}:{}\"\n", escape(user), escape(pass));
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("диск не отвечает — проверьте адрес и интернет".into());
    }
    // заголовки (-D -) идут перед телом; после переадресаций — несколько блоков
    let raw = output.stdout;
    let mut start = 0;
    let mut status = 0;
    let mut etag = String::new();
    while raw[start..].starts_with(b"HTTP/") {
        let Some(end) = raw[start..].windows(4).position(|w| w == b"\r\n\r\n") else { break };
        let head = String::from_utf8_lossy(&raw[start..start + end]).to_string();
        status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        for line in head.lines() {
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("etag:") || (etag.is_empty() && lower.starts_with("last-modified:")) {
                etag = line.split_once(':').map(|x| x.1.trim().to_string()).unwrap_or_default();
            }
        }
        start += end + 4;
    }
    Ok((raw[start..].to_vec(), status, etag))
}

fn dav_status(status: u16) -> String {
    match status {
        401 | 403 => "диск не пустил: проверьте логин и пароль приложения".into(),
        404 | 409 => "на диске нет такой папки".into(),
        507 => "на диске закончилось место".into(),
        _ => format!("диск ответил ошибкой {status}"),
    }
}

/// Скачать файл связки: None — его ещё нет.
pub fn dav_get(url: &str, user: &str, pass: &str) -> Result<Option<(Vec<u8>, String)>, String> {
    let (body, status, etag) = dav_curl(&[url], user, pass)?;
    match status {
        200 => Ok(Some((body, etag))),
        404 => Ok(None),
        _ => Err(dav_status(status)),
    }
}

/// Только метка версии файла — дёшево узнать, поменялся ли он.
pub fn dav_etag(url: &str, user: &str, pass: &str) -> Result<Option<String>, String> {
    let (_, status, etag) = dav_curl(&["-I", url], user, pass)?;
    match status {
        200 => Ok(Some(etag)),
        404 => Ok(None),
        _ => Err(dav_status(status)),
    }
}

pub fn dav_put(url: &str, user: &str, pass: &str, bytes: &[u8]) -> Result<(), String> {
    let temp = std::env::temp_dir().join(format!("rb-sync-{}.tmp", hex(&random_bytes::<6>())));
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    let result = dav_curl(&["-T", &temp.to_string_lossy(), url], user, pass);
    let _ = std::fs::remove_file(&temp);
    let (_, status, _) = result?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(dav_status(status))
    }
}

// ---------- домашняя сеть ----------

/// Что сервер отдаёт телефону.
pub struct Lan {
    /// Номер связки (пустой — синхронизация выключена, всем отказ).
    pub group: Arc<Mutex<String>>,
    /// Растёт, когда на компьютере поменялось то, что нужно телефону: телефон
    /// спрашивает его раз в несколько секунд и приходит за данными, только если вырос.
    pub rev: Arc<AtomicU64>,
}

/// Сервер синхронизации в домашней сети. `exchange` получает пакет телефона и
/// возвращает ответный (или None — отказ). Порт — первый свободный из нескольких.
pub fn serve<F>(bind: IpAddr, lan: Lan, exchange: F) -> Option<u16>
where
    F: Fn(Vec<u8>) -> Option<Vec<u8>> + Send + Sync + 'static,
{
    let (listener, port) = (LAN_PORT..LAN_PORT + 5).find_map(|p| TcpListener::bind((bind, p)).ok().map(|l| (l, p)))?;
    let exchange = Arc::new(exchange);
    let (group, rev) = (lan.group.clone(), lan.rev.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (exchange, group, rev) = (exchange.clone(), group.clone(), rev.clone());
            std::thread::spawn(move || {
                let _ = handle(stream, &group, &rev, &*exchange);
            });
        }
    });
    // поиск по Wi‑Fi: телефон кричит «rb-find <связка>», отвечаем портом
    let group = lan.group;
    std::thread::spawn(move || {
        let Ok(socket) = UdpSocket::bind((bind, LAN_PORT)) else { return };
        let mut buf = [0u8; 256];
        while let Ok((n, from)) = socket.recv_from(&mut buf) {
            let text = String::from_utf8_lossy(&buf[..n]);
            let current = group.lock().map(|g| g.clone()).unwrap_or_default();
            if !current.is_empty() && text.trim() == format!("rb-find {current}") {
                let _ = socket.send_to(format!("rb-here {port}").as_bytes(), from);
            }
        }
    });
    Some(port)
}

fn handle(mut stream: TcpStream, group: &Mutex<String>, rev: &AtomicU64, exchange: &dyn Fn(Vec<u8>) -> Option<Vec<u8>>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    let mut data = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    let head_end = loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        if data.len() > 64 * 1024 {
            return Ok(());
        }
    };
    let head = String::from_utf8_lossy(&data[..head_end]).to_string();
    let mut lines = head.lines();
    let request = lines.next().unwrap_or("");
    let header = |name: &str| {
        head.lines()
            .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
            .unwrap_or_default()
    };
    let current = group.lock().map(|g| g.clone()).unwrap_or_default();
    let reply = |stream: &mut TcpStream, code: &str, body: &[u8]| -> std::io::Result<()> {
        write!(stream, "HTTP/1.1 {code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())?;
        stream.write_all(body)
    };
    if current.is_empty() || header("X-RB-Group") != current {
        return reply(&mut stream, "403 Forbidden", b"");
    }
    if request.starts_with("GET /rb-rev") {
        return reply(&mut stream, "200 OK", rev.load(Ordering::Relaxed).to_string().as_bytes());
    }
    if !request.starts_with("POST /rb-sync") {
        return reply(&mut stream, "404 Not Found", b"");
    }
    let length: usize = header("Content-Length").parse().unwrap_or(0);
    if length > 64 * 1024 * 1024 {
        return reply(&mut stream, "413 Payload Too Large", b"");
    }
    let mut body = data[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    body.truncate(length);
    match exchange(body) {
        Some(answer) => reply(&mut stream, "200 OK", &answer),
        None => reply(&mut stream, "403 Forbidden", b""),
    }
}

/// Где слушать: во всей домашней сети, а для проверок — только на этом компьютере.
pub fn bind_address() -> IpAddr {
    std::env::var("RB_LAN_BIND").ok().and_then(|s| s.parse().ok()).unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
}

// ---------- слияние ----------

pub fn fingerprint(value: &Value) -> String {
    let digest = Sha256::digest(value.to_string().as_bytes());
    digest[..12].iter().map(|b| format!("{b:02x}")).collect()
}

/// Чья версия раздела побеждает.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Choice {
    Local,
    Remote,
    /// Поменялось и здесь, и там — объединить.
    Merge,
}

pub fn choose(local: &Value, remote: Option<&Value>, last: Option<&String>) -> Choice {
    let Some(remote) = remote else { return Choice::Local };
    let (local_hash, remote_hash) = (fingerprint(local), fingerprint(remote));
    if local_hash == remote_hash {
        return Choice::Local;
    }
    match last {
        Some(last) if *last == local_hash => Choice::Remote,
        Some(last) if *last == remote_hash => Choice::Local,
        // первая встреча с этим собеседником или правки с обеих сторон
        _ => Choice::Merge,
    }
}

/// Объединение списков по ключу: сначала чужие, потом свои, которых там нет.
pub fn union_by<F: Fn(&Value) -> String>(remote: &Value, local: &Value, key: F) -> Value {
    let mut out: Vec<Value> = remote.as_array().cloned().unwrap_or_default();
    let mut seen: std::collections::HashSet<String> = out.iter().map(&key).collect();
    for item in local.as_array().into_iter().flatten() {
        if seen.insert(key(item)) {
            out.push(item.clone());
        }
    }
    Value::Array(out)
}

/// Объединение объектов: чужие ключи поверх своих.
pub fn merge_objects(remote: &Value, local: &Value) -> Value {
    let mut out = local.as_object().cloned().unwrap_or_default();
    for (k, v) in remote.as_object().into_iter().flatten() {
        out.insert(k.clone(), v.clone());
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip_and_foreign_key() {
        let key = [7u8; 32];
        let mut bundle = Bundle::default();
        bundle.sections.insert("bookmarks".into(), json!([{"url": "https://a.org", "title": "A"}]));
        let bytes = encrypt(&bundle, &key, "g1").unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("a.org"));
        assert_eq!(decrypt(&bytes, &key).unwrap(), bundle);
        assert!(decrypt(&bytes, &[8u8; 32]).unwrap_err().contains("чужой ключ"));
    }

    #[test]
    fn pair_code_round_trip() {
        let pair = Pair {
            g: "abc123".into(),
            k: crate::base64(&[5u8; 32]),
            n: "ДОМ-ПК".into(),
            a: vec!["192.168.1.5".into()],
            p: LAN_PORT,
            w: Some(PairDav { u: "https://webdav.yandex.ru".into(), l: "me".into(), p: "app-pass".into() }),
        };
        let code = pair_code(&pair);
        assert!(code.starts_with("RBSYNC1.") && !code.contains('+') && !code.contains('/') && !code.contains('='));
        assert_eq!(parse_pair(&code), Some(pair));
        assert_eq!(parse_pair("RBSYNC1.мусор"), None);
        assert_eq!(parse_pair("что-то другое"), None);
        assert!(qr_svg(&code).starts_with("<?xml") || qr_svg(&code).contains("<svg"));
    }

    #[test]
    fn three_way_choice() {
        let (old, new) = (json!([1]), json!([1, 2]));
        let last = fingerprint(&old);
        assert_eq!(choose(&old, Some(&new), Some(&last)), Choice::Remote);
        assert_eq!(choose(&new, Some(&old), Some(&last)), Choice::Local);
        assert_eq!(choose(&new, Some(&json!([3])), Some(&last)), Choice::Merge);
        assert_eq!(choose(&new, None, Some(&last)), Choice::Local);
        assert_eq!(choose(&new, Some(&new), None), Choice::Local);
    }

    #[test]
    fn union_keeps_remote_order_and_adds_local() {
        let remote = json!([{"url": "a"}, {"url": "b"}]);
        let local = json!([{"url": "b"}, {"url": "c"}]);
        let merged = union_by(&remote, &local, |v| v["url"].to_string());
        assert_eq!(merged, json!([{"url": "a"}, {"url": "b"}, {"url": "c"}]));
    }

    #[test]
    fn dav_file_is_inside_folder() {
        assert_eq!(dav_file("https://webdav.yandex.ru/", "ab12"), "https://webdav.yandex.ru/rust-browser-ab12.rbsync");
        assert_eq!(dav_file(" https://x.org/dav ", "g"), "https://x.org/dav/rust-browser-g.rbsync");
    }

    #[test]
    fn lan_server_rejects_strangers_and_answers_members() {
        let lan = Lan { group: Arc::new(Mutex::new("grp".into())), rev: Arc::new(AtomicU64::new(7)) };
        let port = serve("127.0.0.1".parse().unwrap(), lan, |body| Some([b"ok:".as_slice(), &body].concat())).unwrap();
        let ask = |head: &str, body: &[u8]| {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "{head}Content-Length: {}\r\n\r\n", body.len()).unwrap();
            s.write_all(body).unwrap();
            let mut out = Vec::new();
            s.read_to_end(&mut out).unwrap();
            String::from_utf8_lossy(&out).to_string()
        };
        assert!(ask("GET /rb-rev HTTP/1.1\r\nX-RB-Group: other\r\n", b"").starts_with("HTTP/1.1 403"));
        assert!(ask("GET /rb-rev HTTP/1.1\r\nX-RB-Group: grp\r\n", b"").ends_with("\r\n\r\n7"));
        assert!(ask("POST /rb-sync HTTP/1.1\r\nX-RB-Group: grp\r\n", b"hello").ends_with("ok:hello"));
    }
}
