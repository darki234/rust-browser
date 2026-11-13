//! Установка в систему и обновление.
//!
//! Установка — без прав администратора, только для текущего пользователя:
//! %LOCALAPPDATA%\Programs\Rust Browser (exe и видеообои), ярлыки в «Пуске» и на рабочем
//! столе, запись в «Параметры → Приложения» (оттуда же удаление).
//!
//! Обновление: своего сервера нет, поэтому браузер следит за «источником» — exe, из
//! которого его установили (или любым выбранным). Появилась новая сборка — работающий
//! exe переименовывается (Windows это разрешает), на его место кладётся новый; со
//! следующего запуска работает новая версия, старый файл удаляется сам.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const EXE_NAME: &str = "Rust Browser.exe";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\RustBrowser";

pub fn install_dir() -> PathBuf {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
    local.join("Programs").join("Rust Browser")
}

pub fn current_exe() -> PathBuf {
    std::env::current_exe().unwrap_or_default()
}

/// Запущен ли браузер из папки установки.
pub fn is_installed_copy() -> bool {
    let exe = current_exe();
    exe.parent().is_some_and(|dir| same_path(dir, &install_dir()))
}

pub fn is_installed() -> bool {
    install_dir().join(EXE_NAME).is_file()
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
    norm(a) == norm(b)
}

/// Копирует браузер и видеообои, создаёт ярлыки и запись для удаления.
pub fn install(walls_from: Option<&Path>) -> Result<PathBuf, String> {
    let dir = install_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("не создать папку: {e}"))?;
    let target = dir.join(EXE_NAME);
    let source = current_exe();
    if !same_path(&source, &target) {
        replace_exe(&source, &target)?;
    }
    if let Some(walls) = walls_from.filter(|w| w.is_dir()) {
        let into = dir.join("wallpapers");
        let _ = fs::create_dir_all(&into);
        for entry in fs::read_dir(walls).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let lower = name.to_string_lossy().to_lowercase();
            if lower.ends_with(".webm") || lower == "credits.json" {
                let to = into.join(&name);
                let same = fs::metadata(&to).ok().map(|m| m.len()) == entry.metadata().ok().map(|m| m.len());
                if !same {
                    let _ = fs::copy(entry.path(), to);
                }
            }
        }
    }
    let start_menu = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(r"Microsoft\Windows\Start Menu\Programs\Rust Browser.lnk");
    shortcut(&start_menu, &target)?;
    if let Some(desktop) = desktop_dir() {
        shortcut(&desktop.join("Rust Browser.lnk"), &target)?;
    }
    let exe = target.to_string_lossy().to_string();
    let size_kb = fs::metadata(&target).map(|m| m.len() / 1024).unwrap_or(0) as u32;
    let values = [
        ("DisplayName", "Rust Browser".to_string()),
        ("DisplayIcon", format!("{exe},0")),
        ("DisplayVersion", env!("CARGO_PKG_VERSION").to_string()),
        ("Publisher", "Rust Browser".to_string()),
        ("InstallLocation", dir.to_string_lossy().to_string()),
        ("UninstallString", format!("\"{exe}\" --uninstall")),
    ];
    for (name, value) in values {
        crate::system::set_string(UNINSTALL_KEY, Some(name), &value);
    }
    crate::system::set_dword(UNINSTALL_KEY, "EstimatedSize", size_kb);
    crate::system::set_dword(UNINSTALL_KEY, "NoModify", 1);
    crate::system::set_dword(UNINSTALL_KEY, "NoRepair", 1);
    Ok(target)
}

/// Удаление: ярлыки, запись в «Приложениях», регистрация браузера; папку стирает
/// отдельная команда после выхода (работающий exe удалить нельзя). Данные пользователя
/// (%APPDATA%\RustBrowser) остаются.
pub fn uninstall() {
    let start_menu = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(r"Microsoft\Windows\Start Menu\Programs\Rust Browser.lnk");
    let _ = fs::remove_file(start_menu);
    if let Some(desktop) = desktop_dir() {
        let link = desktop.join("Rust Browser.lnk");
        // ярлык на рабочем столе убираем, только если он ведёт в папку установки
        if link_target(&link).is_some_and(|t| t.starts_with(install_dir())) {
            let _ = fs::remove_file(link);
        }
    }
    crate::system::delete_key(UNINSTALL_KEY);
    crate::system::unregister();
    let dir = install_dir();
    let script = format!("ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"", dir.display());
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("cmd").args(["/c", &script]).creation_flags(0x0800_0000).spawn();
}

/// Подмена exe, даже работающего: старый переименовывается в «.old».
pub fn replace_exe(source: &Path, target: &Path) -> Result<(), String> {
    let old = target.with_extension("old");
    let _ = fs::remove_file(&old);
    if target.exists() {
        fs::rename(target, &old).map_err(|e| format!("не переименовать прежнюю версию: {e}"))?;
    }
    let partial = target.with_extension("new");
    let copied = fs::copy(source, &partial).and_then(|_| fs::rename(&partial, target));
    if let Err(error) = copied {
        // вернуть как было
        let _ = fs::remove_file(&partial);
        if old.exists() && !target.exists() {
            let _ = fs::rename(&old, target);
        }
        return Err(format!("не скопировать новую версию: {error}"));
    }
    Ok(())
}

/// Остатки прошлого обновления.
pub fn cleanup_old() {
    let old = current_exe().with_extension("old");
    let _ = fs::remove_file(old);
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Новее ли источник, чем работающий exe (и отличается ли по содержимому).
pub fn update_available(source: &Path) -> bool {
    let exe = current_exe();
    if !source.is_file() || same_path(source, &exe) {
        return false;
    }
    let (Some(src), Some(cur)) = (modified(source), modified(&exe)) else { return false };
    if src <= cur {
        return false;
    }
    let size = |p: &Path| fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    size(source) != size(&exe) || fs::read(source).ok() != fs::read(&exe).ok()
}

/// Ставит обновление: со следующего запуска будет работать новая версия.
pub fn apply_update(source: &Path) -> Result<(), String> {
    let exe = current_exe();
    // проверка, что это действительно наш exe: в нём есть название браузера
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    let marker: Vec<u8> = "RustBrowser".bytes().collect();
    if bytes.len() < 1_000_000 || !bytes.starts_with(b"MZ") || !bytes.windows(marker.len()).any(|w| w == marker.as_slice()) {
        return Err("это не сборка Rust Browser".into());
    }
    replace_exe(source, &exe)?;
    // видеообои, если источник — папка проекта с assets/walls
    if is_installed_copy() {
        let walls = source.ancestors().skip(1).take(5).map(|d| d.join("assets").join("walls")).find(|d| d.is_dir());
        if let Some(walls) = walls {
            let _ = install_walls_only(&walls);
        }
    }
    Ok(())
}

fn install_walls_only(walls: &Path) -> std::io::Result<()> {
    let into = install_dir().join("wallpapers");
    fs::create_dir_all(&into)?;
    for entry in fs::read_dir(walls)?.flatten() {
        let name = entry.file_name();
        let lower = name.to_string_lossy().to_lowercase();
        if (lower.ends_with(".webm") || lower == "credits.json") && !into.join(&name).exists() {
            fs::copy(entry.path(), into.join(&name))?;
        }
    }
    Ok(())
}

fn desktop_dir() -> Option<PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as _));
        text.map(PathBuf::from)
    }
}

/// Ярлык .lnk через IShellLinkW.
fn shortcut(link: &Path, target: &Path) -> Result<(), String> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    unsafe {
        let shell: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        shell.SetPath(&HSTRING::from(target.as_os_str())).map_err(|e| e.to_string())?;
        if let Some(dir) = target.parent() {
            let _ = shell.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()));
        }
        let _ = shell.SetIconLocation(&HSTRING::from(target.as_os_str()), 0);
        let _ = shell.SetDescription(&HSTRING::from("Rust Browser — быстрый браузер"));
        if let Some(dir) = link.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let file: IPersistFile = shell.cast().map_err(|e| e.to_string())?;
        file.Save(&HSTRING::from(link.as_os_str()), true).map_err(|e| e.to_string())
    }
}

fn link_target(link: &Path) -> Option<PathBuf> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    unsafe {
        let shell: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = shell.cast().ok()?;
        file.Load(&HSTRING::from(link.as_os_str()), STGM_READ).ok()?;
        let mut buffer = [0u16; 1024];
        shell.GetPath(&mut buffer, std::ptr::null_mut(), 0).ok()?;
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Some(PathBuf::from(String::from_utf16_lossy(&buffer[..end])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_keeps_old_copy_and_rolls_back() {
        let dir = std::env::temp_dir().join(format!("rb-installer-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let (src, dst) = (dir.join("new.exe"), dir.join("app.exe"));
        fs::write(&src, b"new").unwrap();
        fs::write(&dst, b"old").unwrap();
        replace_exe(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"new");
        assert_eq!(fs::read(dst.with_extension("old")).unwrap(), b"old");
        // источника нет — прежняя версия возвращается на место
        assert!(replace_exe(&dir.join("missing.exe"), &dst).is_err());
        assert_eq!(fs::read(&dst).unwrap(), b"new");
        let _ = fs::remove_dir_all(dir);
    }
}
