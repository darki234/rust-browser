//! Аккаунт Rust Browser: зашифрованная синхронизация через облачную папку.
//!
//! Своего сервера у браузера нет, поэтому «облаком» служит любая синхронизируемая
//! папка — по умолчанию OneDrive. В ней лежит один файл, зашифрованный AES-256-GCM
//! ключом из пароля аккаунта (PBKDF2-SHA256, 600 000 повторов). Облако видит только
//! шум: без пароля файл не расшифровать ни Microsoft, ни тому, кто его утащит.
//! Ключ на этом компьютере хранится под защитой DPAPI — пароль вводится один раз.
//!
//! Слияние трёхстороннее: для каждого раздела помним отпечаток того, что было
//! после прошлой синхронизации. Поменялось только здесь — оставляем своё, только
//! в облаке — берём облачное, и там и там — объединяем.

use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const ITERATIONS: u32 = 600_000;

/// Аккаунт на этом компьютере (account.json в папке данных).
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Account {
    pub name: String,
    pub folder: PathBuf,
    /// Ключ шифрования под DPAPI (hex).
    pub key: String,
    /// Соль из файла в облаке — у всех компьютеров аккаунта одна.
    pub salt: String,
    /// Когда последний раз синхронизировались (секунды Unix).
    #[serde(default)]
    pub synced: u64,
    /// Отпечатки разделов после прошлой синхронизации.
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
}

/// Что синхронизируется. Каждый раздел — произвольный JSON, сливает его main.rs.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Bundle {
    #[serde(default)]
    pub sections: BTreeMap<String, Value>,
    /// Кто и когда записал файл.
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub time: u64,
}

/// Файл в облаке: всё, кроме соли и параметров, зашифровано.
#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    kdf: String,
    iterations: u32,
    salt: String,
    nonce: String,
    data: String,
}

/// Папка по умолчанию: OneDrive\Rust Browser, если OneDrive есть.
pub fn default_folder() -> Option<PathBuf> {
    let onedrive = std::env::var_os("OneDrive").map(PathBuf::from).filter(|p| p.is_dir())?;
    Some(onedrive.join("Rust Browser"))
}

/// Файл аккаунта: имя не видно — только его отпечаток.
pub fn file_for(folder: &Path, name: &str) -> PathBuf {
    let digest = Sha256::digest(name.trim().to_lowercase().as_bytes());
    let short: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    folder.join(format!("RustBrowser-{short}.rbsync"))
}

pub fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, ITERATIONS, &mut key);
    key
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    let mut bytes = [0u8; N];
    unsafe {
        let _ = BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    }
    bytes
}

pub fn encrypt(bundle: &Bundle, key: &[u8; 32], salt: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = random_bytes::<12>();
    let plain = serde_json::to_vec(bundle).map_err(|e| e.to_string())?;
    let sealed = cipher.encrypt(Nonce::from_slice(&nonce), plain.as_slice()).map_err(|_| "не удалось зашифровать")?;
    let envelope = Envelope {
        format: "rust-browser-sync/1".into(),
        kdf: "pbkdf2-sha256".into(),
        iterations: ITERATIONS,
        salt: crate::base64(salt),
        nonce: crate::base64(&nonce),
        data: crate::base64(&sealed),
    };
    serde_json::to_vec(&envelope).map_err(|e| e.to_string())
}

/// Соль из файла — чтобы получить ключ из пароля при входе с другого компьютера.
pub fn read_salt(bytes: &[u8]) -> Option<Vec<u8>> {
    let envelope: Envelope = serde_json::from_slice(bytes).ok()?;
    crate::base64_decode(&envelope.salt)
}

pub fn decrypt(bytes: &[u8], key: &[u8; 32]) -> Result<Bundle, String> {
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| "файл синхронизации повреждён")?;
    if envelope.format != "rust-browser-sync/1" {
        return Err("файл от более новой версии браузера".into());
    }
    let nonce = crate::base64_decode(&envelope.nonce).filter(|n| n.len() == 12).ok_or("файл повреждён")?;
    let sealed = crate::base64_decode(&envelope.data).ok_or("файл повреждён")?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce), sealed.as_slice())
        .map_err(|_| "неверный пароль")?;
    serde_json::from_slice(&plain).map_err(|e| e.to_string())
}

/// Запись через временный файл: облако не увидит недописанный файл.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("папка недоступна: {e}"))?;
    }
    let partial = path.with_extension("partial");
    fs::write(&partial, bytes).map_err(|e| e.to_string())?;
    fs::rename(&partial, path).map_err(|e| e.to_string())
}

pub fn fingerprint(value: &Value) -> String {
    let digest = Sha256::digest(value.to_string().as_bytes());
    digest[..12].iter().map(|b| format!("{b:02x}")).collect()
}

/// Чья версия раздела побеждает.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Choice {
    Local,
    Remote,
    /// Поменялось и здесь, и в облаке — объединить.
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
        // первая синхронизация на этом компьютере или правки с обеих сторон
        _ => Choice::Merge,
    }
}

/// Объединение списков по ключу: сначала облачные, потом свои, которых там нет.
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

/// Объединение объектов: облачные ключи поверх своих.
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
    fn round_trip_and_wrong_password() {
        let salt = random_bytes::<16>();
        let key = [7u8; 32];
        let mut bundle = Bundle::default();
        bundle.sections.insert("bookmarks".into(), json!([{"url": "https://a.org", "title": "A"}]));
        let bytes = encrypt(&bundle, &key, &salt).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("a.org"));
        assert_eq!(read_salt(&bytes).unwrap(), salt);
        assert_eq!(decrypt(&bytes, &key).unwrap(), bundle);
        assert_eq!(decrypt(&bytes, &[8u8; 32]).unwrap_err(), "неверный пароль");
    }

    #[test]
    fn key_depends_on_password_and_salt() {
        let a = derive_key("пароль", b"salt-1");
        assert_eq!(a, derive_key("пароль", b"salt-1"));
        assert_ne!(a, derive_key("пароль", b"salt-2"));
        assert_ne!(a, derive_key("Пароль", b"salt-1"));
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
    fn file_name_hides_account_name() {
        let path = file_for(Path::new("C:/x"), " Миша@Mail.ru ");
        assert_eq!(path, file_for(Path::new("C:/x"), "миша@mail.ru"));
        assert!(!path.to_string_lossy().contains("mail"));
    }
}
