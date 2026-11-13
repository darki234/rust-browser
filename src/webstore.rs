//! Расширения из Chrome Web Store и из других браузеров.
//!
//! WebView2 сам не умеет ставить расширения из магазина, поэтому браузер скачивает
//! пакет CRX с сервера обновлений Google, проверяет, что ключ внутри пакета даёт
//! тот самый id, распаковывает его и ставит в профиль как распакованное расширение.
//! Ключ дописывается в manifest.json — так у расширения остаётся родной id
//! (на нём держатся вход через Google и синхронизация настроек в самих расширениях).
//! Из Chrome, Edge, Яндекса, Brave, Opera и Vivaldi расширения копируются из их профиля.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    process::Command,
};

/// Сведения о поставленном нами расширении: где лежит и что у него открывать.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Installed {
    pub id: String,
    pub name: String,
    pub version: String,
    pub folder: PathBuf,
    /// Страница кнопки расширения (action.default_popup), относительно папки.
    #[serde(default)]
    pub popup: String,
    /// Страница настроек.
    #[serde(default)]
    pub options: String,
    /// Самый крупный значок.
    #[serde(default)]
    pub icon: String,
    /// Откуда взялось: «Chrome Web Store», «Chrome», «Edge»…
    #[serde(default)]
    pub source: String,
    /// manifest_version: 2 в Chromium 157 уже не работает.
    #[serde(default)]
    pub mv: u64,
}

/// id расширения: 32 буквы от a до p.
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// id из адреса страницы расширения в магазине:
/// chromewebstore.google.com/detail/<имя>/<id> или старый chrome.google.com/webstore/detail/…
pub fn store_id(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let path = path.split(['?', '#']).next()?;
    let detail = match host {
        "chromewebstore.google.com" => path.strip_prefix("detail/")?,
        "chrome.google.com" => path.strip_prefix("webstore/detail/")?,
        _ => return None,
    };
    detail.split('/').filter(|part| valid_id(part)).last().map(str::to_string)
}

/// id из открытого ключа расширения: первые 16 байт SHA-256, цифры 0–f заменены буквами a–p.
pub fn id_from_key(public_key: &[u8]) -> String {
    Sha256::digest(public_key)[..16]
        .iter()
        .flat_map(|b| [b >> 4, b & 15])
        .map(|n| (b'a' + n) as char)
        .collect()
}

// ---------- пакет CRX ----------

/// Разбирает CRX3: возвращает (архив zip, открытый ключ, дающий ожидаемый id).
/// Пакет, подписанный не тем ключом, отбрасывается — подменить расширение не выйдет.
pub fn open_crx(bytes: &[u8], expected_id: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    if bytes.len() < 12 || &bytes[..4] != b"Cr24" {
        return Err("это не пакет расширения".into());
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != 3 {
        return Err(format!("неизвестная версия пакета {version}"));
    }
    let header_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let header = bytes.get(12..12 + header_len).ok_or("пакет обрезан")?;
    let archive = bytes[12 + header_len..].to_vec();
    // CrxFileHeader: поле 2 — sha256_with_rsa, поле 3 — sha256_with_ecdsa (AsymmetricKeyProof),
    // в каждом поле 1 — открытый ключ.
    let mut key = None;
    for (field, value) in proto_fields(header) {
        if field == 2 || field == 3 {
            for (inner, public_key) in proto_fields(value) {
                if inner == 1 && id_from_key(public_key) == expected_id {
                    key = Some(public_key.to_vec());
                }
            }
        }
    }
    let key = key.ok_or("подпись пакета не совпадает с расширением")?;
    Ok((archive, key))
}

/// Поля protobuf с длиной (wire type 2); остальные типы пропускаются.
fn proto_fields(mut data: &[u8]) -> Vec<(u64, &[u8])> {
    let mut out = Vec::new();
    while !data.is_empty() {
        let Some((tag, rest)) = varint(data) else { break };
        data = rest;
        match tag & 7 {
            0 => {
                let Some((_, rest)) = varint(data) else { break };
                data = rest;
            }
            1 => data = data.get(8..).unwrap_or_default(),
            5 => data = data.get(4..).unwrap_or_default(),
            2 => {
                let Some((len, rest)) = varint(data) else { break };
                let len = len as usize;
                if rest.len() < len {
                    break;
                }
                out.push((tag >> 3, &rest[..len]));
                data = &rest[len..];
            }
            _ => break,
        }
    }
    out
}

fn varint(data: &[u8]) -> Option<(u64, &[u8])> {
    let mut value = 0u64;
    for (i, byte) in data.iter().enumerate().take(10) {
        value |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((value, &data[i + 1..]));
        }
    }
    None
}

/// Скачивает расширение из магазина и распаковывает в `extensions/<id>`.
/// Работает в фоновом потоке: качает curl.exe, который есть в каждой Windows 10/11.
pub fn download(id: &str, extensions_dir: &Path, chrome_version: &str) -> Result<Installed, String> {
    if !valid_id(id) {
        return Err("неверный id расширения".into());
    }
    let major = chrome_version.split('.').next().unwrap_or("130");
    let url = format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&prodversion={major}.0.0.0\
         &acceptformat=crx3&x=id%3D{id}%26installsource%3Dondemand%26uc"
    );
    fs::create_dir_all(extensions_dir).map_err(|e| e.to_string())?;
    let file = extensions_dir.join(format!("{id}.crx"));
    let status = Command::new("curl.exe")
        .args(["-sSLf", "--retry", "2", "--max-time", "120", "-o"])
        .arg(&file)
        .arg(&url)
        .creation_flags_no_window()
        .status()
        .map_err(|e| format!("не запустился curl: {e}"))?;
    let bytes = fs::read(&file).unwrap_or_default();
    let _ = fs::remove_file(&file);
    if !status.success() || bytes.is_empty() {
        return Err("магазин не отдал расширение — проверьте интернет".into());
    }
    let (archive, key) = open_crx(&bytes, id)?;
    let folder = extensions_dir.join(id);
    install_folder(&folder, |partial| unzip(&archive, partial))?;
    set_key(&folder, &key)?;
    let mut info = describe(&folder).ok_or("в пакете нет manifest.json")?;
    info.id = id.to_string();
    info.source = "Chrome Web Store".into();
    Ok(info)
}

/// Готовит папку через временную: недоделанная установка не испортит прежнюю версию.
fn install_folder(folder: &Path, fill: impl FnOnce(&Path) -> std::io::Result<()>) -> Result<(), String> {
    let partial = folder.with_extension("partial");
    let _ = fs::remove_dir_all(&partial);
    fill(&partial).map_err(|e| e.to_string())?;
    if !partial.join("manifest.json").exists() {
        let _ = fs::remove_dir_all(&partial);
        return Err("в пакете нет manifest.json".into());
    }
    let _ = fs::remove_dir_all(folder);
    fs::rename(&partial, folder).map_err(|e| e.to_string())
}

fn unzip(archive: &[u8], into: &Path) -> std::io::Result<()> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).map_err(std::io::Error::other)?;
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).map_err(std::io::Error::other)?;
        let Some(relative) = file.enclosed_name() else { continue };
        let path = into.join(relative);
        if file.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)?;
        fs::write(path, bytes)?;
    }
    Ok(())
}

/// Дописывает ключ в manifest.json, чтобы id расширения не поменялся.
fn set_key(folder: &Path, key: &[u8]) -> Result<(), String> {
    let path = folder.join("manifest.json");
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut manifest: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    if manifest.get("key").is_none() {
        manifest["key"] = Value::String(crate::base64(key));
        fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap_or(text)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- сведения из manifest.json ----------

pub fn describe(folder: &Path) -> Option<Installed> {
    let text = fs::read_to_string(folder.join("manifest.json")).ok()?;
    let manifest: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let text_of = |key: &str| manifest[key].as_str().unwrap_or("").to_string();
    let action = if manifest["action"].is_object() { &manifest["action"] } else { &manifest["browser_action"] };
    let options = manifest["options_ui"]["page"].as_str().or(manifest["options_page"].as_str()).unwrap_or("");
    let icon = manifest["icons"]
        .as_object()
        .and_then(|icons| icons.iter().max_by_key(|(size, _)| size.parse::<u32>().unwrap_or(0)))
        .and_then(|(_, path)| path.as_str())
        .unwrap_or("");
    let id = manifest["key"]
        .as_str()
        .and_then(crate::base64_decode)
        .map(|key| id_from_key(&key))
        .unwrap_or_default();
    Some(Installed {
        id,
        name: localized(folder, &manifest, &text_of("name")),
        version: text_of("version"),
        folder: folder.to_path_buf(),
        popup: action["default_popup"].as_str().unwrap_or("").trim_start_matches('/').to_string(),
        options: options.trim_start_matches('/').to_string(),
        icon: icon.trim_start_matches('/').to_string(),
        source: String::new(),
        mv: manifest["manifest_version"].as_u64().unwrap_or(0),
    })
}

/// «__MSG_appName__» → строка из _locales: русская, иначе язык по умолчанию.
fn localized(folder: &Path, manifest: &Value, text: &str) -> String {
    let Some(key) = text.strip_prefix("__MSG_").and_then(|t| t.strip_suffix("__")) else {
        return text.to_string();
    };
    let default = manifest["default_locale"].as_str().unwrap_or("en");
    for locale in ["ru", default, "en"] {
        let path = folder.join("_locales").join(locale).join("messages.json");
        let Ok(raw) = fs::read_to_string(path) else { continue };
        let Ok(messages) = serde_json::from_str::<Value>(raw.trim_start_matches('\u{feff}')) else { continue };
        if let Some(found) = messages
            .as_object()
            .and_then(|m| m.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)))
            .and_then(|(_, v)| v["message"].as_str())
        {
            return found.to_string();
        }
    }
    key.to_string()
}

// ---------- перенос из других браузеров ----------

/// Найденное в другом браузере расширение.
#[derive(Serialize, Clone, Debug)]
pub struct Found {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(skip)]
    pub folder: PathBuf,
}

/// Браузеры, у которых есть папка расширений: (название, папки профилей).
pub fn browser_profiles() -> Vec<(&'static str, Vec<PathBuf>)> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    let candidates: [(&str, PathBuf); 7] = [
        ("Chrome", local.join(r"Google\Chrome\User Data")),
        ("Edge", local.join(r"Microsoft\Edge\User Data")),
        ("Яндекс Браузер", local.join(r"Yandex\YandexBrowser\User Data")),
        ("Brave", local.join(r"BraveSoftware\Brave-Browser\User Data")),
        ("Vivaldi", local.join(r"Vivaldi\User Data")),
        ("Opera", roaming.join(r"Opera Software\Opera Stable")),
        ("Opera GX", roaming.join(r"Opera Software\Opera GX Stable")),
    ];
    candidates
        .into_iter()
        .filter_map(|(name, root)| {
            let mut profiles = Vec::new();
            if root.join("Extensions").is_dir() {
                profiles.push(root.clone()); // у Opera профиль прямо в корне
            }
            if let Ok(entries) = fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if (file_name == "Default" || file_name.starts_with("Profile ")) && entry.path().join("Extensions").is_dir() {
                        profiles.push(entry.path());
                    }
                }
            }
            (!profiles.is_empty()).then_some((name, profiles))
        })
        .collect()
}

/// Расширения браузера из всех его профилей, без встроенных: только поставленные из магазина.
pub fn scan(profiles: &[PathBuf]) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    for profile in profiles {
        let Ok(entries) = fs::read_dir(profile.join("Extensions")) else { continue };
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().to_string();
            if !valid_id(&id) || found.iter().any(|f| f.id == id) {
                continue;
            }
            // внутри — папки версий; берём самую новую
            let Some(version_dir) = fs::read_dir(entry.path())
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.join("manifest.json").is_file())
                .max_by(|a, b| version_key(a).cmp(&version_key(b)))
            else {
                continue;
            };
            let Ok(text) = fs::read_to_string(version_dir.join("manifest.json")) else { continue };
            let Ok(manifest) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else { continue };
            // встроенные и служебные расширения не обновляются из магазина
            if manifest.get("update_url").is_none() || manifest["theme"].is_object() {
                continue;
            }
            let Some(info) = describe(&version_dir) else { continue };
            // служебные дополнения самого Edge ставятся через его магазин, но вне Edge бесполезны
            let edge_store = manifest["update_url"].as_str().unwrap_or("").contains("edge.microsoft.com");
            if edge_store && (info.name.starts_with("Microsoft Edge") || info.name.starts_with("Edge ")) {
                continue;
            }
            found.push(Found { id, name: info.name, version: info.version, folder: version_dir });
        }
    }
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found
}

fn version_key(path: &Path) -> Vec<u64> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    name.split(['.', '_']).map(|part| part.parse().unwrap_or(0)).collect()
}

/// Копирует расширение другого браузера к себе (его папку тот браузер может обновить или удалить).
pub fn copy_from(found: &Found, extensions_dir: &Path, browser: &str) -> Result<Installed, String> {
    let folder = extensions_dir.join(&found.id);
    install_folder(&folder, |partial| copy_dir(&found.folder, partial))?;
    let _ = fs::remove_dir_all(folder.join("_metadata")); // подписи того браузера нам не нужны
    let mut info = describe(&folder).ok_or("нет manifest.json")?;
    info.id = found.id.clone();
    info.source = browser.to_string();
    Ok(info)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

// ---------- кнопка «Установить» на страницах магазина ----------

/// Встраивается во все вкладки, но работает только на chromewebstore.google.com:
/// ставит свою кнопку на странице расширения. id браузер берёт из адреса вкладки сам.
pub const STORE_SCRIPT: &str = r#"(() => {
  if (location.hostname !== 'chromewebstore.google.com' || window.top !== window) return;
  const ID = /\/detail\/(?:[^/]+\/)?([a-p]{32})/;
  let button;
  const place = () => {
    const match = location.pathname.match(ID);
    if (!match) { button?.remove(); button = null; return; }
    if (button?.isConnected) return;
    button = document.createElement('button');
    button.textContent = 'Установить в Rust Browser';
    button.style.cssText = 'position:fixed;z-index:2147483647;right:24px;bottom:24px;padding:14px 22px;border:0;border-radius:24px;' +
      'background:linear-gradient(135deg,#6d7cff,#a855f7);color:#fff;font:600 15px "Segoe UI",sans-serif;cursor:pointer;' +
      'box-shadow:0 10px 30px rgba(90,70,220,.45);transition:transform .2s';
    button.onmouseenter = () => { button.style.transform = 'translateY(-2px)'; };
    button.onmouseleave = () => { button.style.transform = ''; };
    button.onclick = () => {
      button.textContent = 'Устанавливается…';
      window.ipc.postMessage(JSON.stringify({ cmd: 'ext_store_install' }));
    };
    document.documentElement.append(button);
    window.ipc.postMessage(JSON.stringify({ cmd: 'ext_store_check' }));
  };
  window.__rbStoreDone = (text) => { if (button) button.textContent = text; };
  // магазин — одностраничное приложение: следим за сменой адреса
  setInterval(place, 800);
  document.addEventListener('DOMContentLoaded', place);
})();"#;

/// В CommandExt нет «без окна» по умолчанию: без флага мелькнуло бы чёрное окно консоли.
trait NoWindow {
    fn creation_flags_no_window(&mut self) -> &mut Self;
}

impl NoWindow for Command {
    fn creation_flags_no_window(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_urls() {
        let id = "cjpalhdlnbpafiamejdnhcphjbkeiagm";
        assert_eq!(store_id(&format!("https://chromewebstore.google.com/detail/ublock-origin/{id}")).as_deref(), Some(id));
        assert_eq!(store_id(&format!("https://chromewebstore.google.com/detail/{id}?hl=ru")).as_deref(), Some(id));
        assert_eq!(store_id(&format!("https://chrome.google.com/webstore/detail/x/{id}")).as_deref(), Some(id));
        assert_eq!(store_id(&format!("https://evil.com/detail/x/{id}")), None);
        assert_eq!(store_id("https://chromewebstore.google.com/detail/x/notanid"), None);
    }

    #[test]
    fn id_from_key_maps_sha256_to_letters() {
        // SHA-256 пустой строки начинается с e3b0c442…: e→o, 3→d, b→l, 0→a…
        assert_eq!(id_from_key(b""), "odlameecjipmbmbejkplpemijjgpljce");
        assert!(valid_id(&id_from_key(b"any key")));
    }

    #[test]
    fn crx_with_wrong_key_is_rejected() {
        let key = b"some-public-key";
        let id = id_from_key(key);
        // заголовок: поле 2 { поле 1 = ключ }
        let mut proof = vec![0x0a, key.len() as u8];
        proof.extend_from_slice(key);
        let mut header = vec![0x12, proof.len() as u8];
        header.extend_from_slice(&proof);
        let mut crx = b"Cr24".to_vec();
        crx.extend_from_slice(&3u32.to_le_bytes());
        crx.extend_from_slice(&(header.len() as u32).to_le_bytes());
        crx.extend_from_slice(&header);
        crx.extend_from_slice(b"PK-zip");
        let (archive, found) = open_crx(&crx, &id).unwrap();
        assert_eq!(archive, b"PK-zip");
        assert_eq!(found, key);
        assert!(open_crx(&crx, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_err());
        assert!(open_crx(b"PK-not-crx", &id).is_err());
    }
}
