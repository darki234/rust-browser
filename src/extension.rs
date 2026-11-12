//! uBlock Origin Lite: встроенный блокировщик рекламы.
//!
//! Архив расширения лежит внутри exe и при первом запуске распаковывается в папку
//! данных. Классический uBlock Origin в Chromium 157 уже не работает (там отключены
//! расширения формата MV2), поэтому используется его версия для MV3 от того же автора.

use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

/// Официальный выпуск: github.com/uBlockOrigin/uBOL-home, uBOLite_2026.1110.1816.chromium.zip
pub const VERSION: &str = "2026.1110.1816";
static ARCHIVE: &[u8] = include_bytes!("../assets/ubol.zip");

/// Папка распакованного расширения для этой версии.
pub fn folder(data_dir: &Path) -> PathBuf {
    data_dir.join("extensions").join(format!("ubol-{VERSION}"))
}

/// Распаковывает расширение, если этой версии ещё нет на диске.
/// Старые версии удаляет, чтобы не копились.
pub fn ensure_unpacked(data_dir: &Path) -> std::io::Result<PathBuf> {
    let target = folder(data_dir);
    if target.join("manifest.json").exists() {
        return Ok(target);
    }
    let parent = target.parent().expect("у папки расширения всегда есть родитель");
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with("ubol-") {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    // Распаковываем во временную папку и переименовываем: недораспакованное
    // расширение (если браузер закрыли посреди процесса) не попадёт в профиль.
    let partial = parent.join(format!("ubol-{VERSION}.partial"));
    let _ = fs::remove_dir_all(&partial);
    unpack(ARCHIVE, &partial)?;
    fs::rename(&partial, &target)?;
    Ok(target)
}

fn unpack(archive: &[u8], into: &Path) -> std::io::Result<()> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).map_err(std::io::Error::other)?;
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).map_err(std::io::Error::other)?;
        // enclosed_name отбрасывает пути вида ../ — архив не сможет писать вне папки
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_archive_is_the_declared_version() {
        let mut zip = zip::ZipArchive::new(Cursor::new(ARCHIVE)).unwrap();
        let mut manifest = String::new();
        zip.by_name("manifest.json").unwrap().read_to_string(&mut manifest).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(manifest["version"], VERSION);
        assert_eq!(manifest["manifest_version"], 3);
    }

    #[test]
    fn unpacks_once_and_replaces_old_versions() {
        let dir = std::env::temp_dir().join(format!("rb-ext-{}", std::process::id()));
        let old = dir.join("extensions").join("ubol-old");
        fs::create_dir_all(&old).unwrap();

        let path = ensure_unpacked(&dir).unwrap();
        assert!(path.join("manifest.json").exists());
        assert!(!old.exists(), "старая версия удалена");

        // второй вызов ничего не распаковывает заново
        let marker = path.join("marker");
        fs::write(&marker, b"x").unwrap();
        assert_eq!(ensure_unpacked(&dir).unwrap(), path);
        assert!(marker.exists());
        let _ = fs::remove_dir_all(dir);
    }
}
