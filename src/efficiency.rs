//! Экономия ресурсов: какие процессы отрисовывают какие вкладки, сколько они
//! занимают памяти, режим эффективности Windows (EcoQoS) для фоновых вкладок
//! и работа от батареи.

use std::{collections::HashMap, ffi::c_void};

use webview2_com::{GetProcessExtendedInfosCompletedHandler, Microsoft::Web::WebView2::Win32::*};
use windows::{
    core::Interface,
    Win32::{
        Foundation::CloseHandle,
        System::{
            Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
            SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
            Threading::{
                GetProcessInformation, OpenProcess, ProcessPowerThrottling, SetProcessInformation,
                PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
            },
        },
    },
};
use wry::{WebView, WebViewExtWindows};

/// Процесс WebView2: pid, вид и фреймы, которые он отрисовывает.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub renderer: bool,
    pub frames: Vec<u32>,
}

/// id главного фрейма вкладки — по нему вкладка находится в списке процессов.
/// До первой загрузки бывает 0.
pub fn main_frame_id(webview: &WebView) -> Option<u32> {
    unsafe {
        let core = webview.webview().cast::<ICoreWebView2_20>().ok()?;
        let mut id = 0u32;
        core.FrameId(&mut id).ok()?;
        (id != 0).then_some(id)
    }
}

/// Все процессы профиля WebView2 с их фреймами (асинхронно).
pub fn processes(webview: &WebView, done: impl FnOnce(Vec<ProcessInfo>) + 'static) {
    let Ok(env) = webview.environment().cast::<ICoreWebView2Environment13>() else {
        return done(Vec::new());
    };
    let handler = GetProcessExtendedInfosCompletedHandler::create(Box::new(move |result, list| {
        let mut out = Vec::new();
        if let (Ok(()), Some(list)) = (result, list) {
            unsafe { collect(&list, &mut out) };
        }
        done(out);
        Ok(())
    }));
    unsafe {
        let _ = env.GetProcessExtendedInfos(&handler);
    }
}

unsafe fn collect(list: &ICoreWebView2ProcessExtendedInfoCollection, out: &mut Vec<ProcessInfo>) {
    let mut count = 0u32;
    if list.Count(&mut count).is_err() {
        return;
    }
    for index in 0..count {
        let Ok(extended) = list.GetValueAtIndex(index) else { continue };
        let Ok(info) = extended.ProcessInfo() else { continue };
        let (mut pid, mut kind) = (0i32, COREWEBVIEW2_PROCESS_KIND::default());
        let _ = info.ProcessId(&mut pid);
        let _ = info.Kind(&mut kind);
        let mut frames = Vec::new();
        // Список фреймов держим живым весь обход: итератор WebView2 его не удерживает,
        // и без этого обращается к уже освобождённой памяти (процесс падает).
        let frames_list = extended.AssociatedFrameInfos();
        let iterator = frames_list.as_ref().ok().and_then(|f| f.GetIterator().ok());
        if let Some(iterator) = iterator {
            loop {
                let mut has = windows::core::BOOL::default();
                if iterator.HasCurrent(&mut has).is_err() || !has.as_bool() {
                    break;
                }
                if let Ok(frame) = iterator.GetCurrent().and_then(|f| f.cast::<ICoreWebView2FrameInfo2>()) {
                    let mut id = 0u32;
                    if frame.FrameId(&mut id).is_ok() {
                        frames.push(id);
                    }
                }
                let mut more = windows::core::BOOL::default();
                if iterator.MoveNext(&mut more).is_err() || !more.as_bool() {
                    break;
                }
            }
        }
        out.push(ProcessInfo {
            pid: pid as u32,
            renderer: kind == COREWEBVIEW2_PROCESS_KIND_RENDERER,
            frames,
        });
    }
}

/// Частная память процесса в байтах (то, что он занимает сам, без общих библиотек).
pub fn private_memory(pid: u32) -> Option<u64> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        let read = GetProcessMemoryInfo(
            process,
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            counters.cb,
        );
        let _ = CloseHandle(process);
        read.ok().map(|_| counters.PrivateUsage as u64)
    }
}

/// Режим эффективности Windows: процесс получает меньше процессорного времени
/// и работает на экономичных ядрах. Так Edge обходится с фоновыми вкладками.
pub fn set_efficiency(pid: u32, on: bool) -> bool {
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_SET_INFORMATION, false, pid) else { return false };
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: 1,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
        };
        let done = SetProcessInformation(
            process,
            ProcessPowerThrottling,
            &state as *const PROCESS_POWER_THROTTLING_STATE as *const c_void,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
        let _ = CloseHandle(process);
        done.is_ok()
    }
}

/// Включён ли режим эффективности у процесса (для проверок).
#[allow(dead_code)]
pub fn is_efficient(pid: u32) -> Option<bool> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut state = PROCESS_POWER_THROTTLING_STATE { Version: 1, ..Default::default() };
        let read = GetProcessInformation(
            process,
            ProcessPowerThrottling,
            &mut state as *mut PROCESS_POWER_THROTTLING_STATE as *mut c_void,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
        let _ = CloseHandle(process);
        read.ok().map(|_| state.StateMask & PROCESS_POWER_THROTTLING_EXECUTION_SPEED != 0)
    }
}

/// Насколько занята оперативка всей системы, в процентах (0 — не удалось узнать).
pub fn memory_load() -> u32 {
    unsafe {
        let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        if GlobalMemoryStatusEx(&mut status).is_ok() { status.dwMemoryLoad } else { 0 }
    }
}

/// Ноутбук работает от батареи (а не от сети).
pub fn on_battery() -> bool {
    unsafe {
        let mut status = SYSTEM_POWER_STATUS::default();
        GetSystemPowerStatus(&mut status).is_ok() && status.ACLineStatus == 0
    }
}

/// Каким процессам отрисовки включить режим эффективности, а каким выключить.
/// Процесс бывает общим у нескольких вкладок одного сайта: если хоть одна из них
/// на экране или играет звук, процесс не замедляем.
pub fn efficiency_plan(
    renderers: &[u32],
    tab_pids: &[(bool, Vec<u32>)], // (вкладку нельзя замедлять, её процессы)
    enabled: bool,
) -> HashMap<u32, bool> {
    let protected: Vec<u32> = tab_pids
        .iter()
        .filter(|(keep, _)| *keep)
        .flat_map(|(_, pids)| pids.iter().copied())
        .collect();
    let used_by_tabs: Vec<u32> = tab_pids.iter().flat_map(|(_, pids)| pids.iter().copied()).collect();
    renderers
        .iter()
        .map(|pid| {
            let slow = enabled && used_by_tabs.contains(pid) && !protected.contains(pid);
            (*pid, slow)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_background_tab_processes_are_slowed() {
        // 10 — активная вкладка, 20 — фоновая, 30 — фоновая со звуком, 40 — интерфейс
        let tabs = vec![(true, vec![10]), (false, vec![20]), (true, vec![30])];
        let plan = efficiency_plan(&[10, 20, 30, 40], &tabs, true);
        assert_eq!(plan[&10], false);
        assert_eq!(plan[&20], true);
        assert_eq!(plan[&30], false);
        assert_eq!(plan[&40], false, "процессы не вкладок (интерфейс) не трогаем");
    }

    #[test]
    fn shared_process_follows_the_visible_tab() {
        // две вкладки одного сайта в общем процессе 10, одна из них на экране
        let tabs = vec![(true, vec![10]), (false, vec![10])];
        assert_eq!(efficiency_plan(&[10], &tabs, true)[&10], false);
    }

    #[test]
    fn disabled_mode_releases_everything() {
        let tabs = vec![(false, vec![20])];
        assert_eq!(efficiency_plan(&[20], &tabs, false)[&20], false);
    }

    #[test]
    fn own_process_can_be_slowed_and_restored() {
        let pid = std::process::id();
        assert!(set_efficiency(pid, true));
        assert_eq!(is_efficient(pid), Some(true));
        assert!(set_efficiency(pid, false));
        assert_eq!(is_efficient(pid), Some(false));
        assert!(private_memory(pid).unwrap() > 0);
    }
}
