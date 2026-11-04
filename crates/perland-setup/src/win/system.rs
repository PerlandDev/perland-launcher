//! The Windows calls the setup needs: the registry, processes, the UI language, the clipboard and
//! starting the launcher.

use crate::payload::installed_exe;
use crate::text::{lang_from_langid, Lang};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::{
    CloseHandle, GlobalFree, ERROR_SUCCESS, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

/// Where Tauri's NSIS template records a per-user install (tauri-bundler 2.x `UNINSTKEY`).
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Perland Launcher";
const LAUNCHER_EXE: &str = "perland-launcher.exe";
const CF_UNICODETEXT: u32 = 13;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn uninstall_value(name: &str) -> Option<String> {
    let (key, value) = (wide(UNINSTALL_KEY), wide(name));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: the key, value and buffer pointers are valid for the call; `size` is the buffer's
    // size in bytes and RegGetValueW writes at most that many.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then(|| from_wide(&buf))
}

/// The installed launcher's version and exe, from the registry.
pub fn installed() -> Option<(String, PathBuf)> {
    let version = uninstall_value("DisplayVersion")?;
    let location = uninstall_value("InstallLocation")?;
    let binary = uninstall_value("MainBinaryName");
    Some((version, installed_exe(&location, binary.as_deref())))
}

/// Whether a `perland-launcher.exe` runs, from anywhere. The NSIS installer would force-close it.
pub fn launcher_running() -> bool {
    // SAFETY: a snapshot handle is checked before use and closed once; the entry struct is sized
    // as the API requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut ok = Process32FirstW(snapshot, &mut entry) != 0;
        while ok {
            if from_wide(&entry.szExeFile).eq_ignore_ascii_case(LAUNCHER_EXE) {
                found = true;
                break;
            }
            ok = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        found
    }
}

pub fn ui_lang() -> Lang {
    // SAFETY: no arguments, no preconditions.
    lang_from_langid(unsafe { GetUserDefaultUILanguage() })
}

/// Starts the launcher on its own, with no arguments, so it outlives the setup.
pub fn launch(exe: &Path) -> bool {
    let mut command = std::process::Command::new(exe);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .is_ok()
}

pub fn copy_to_clipboard(text: &str) -> bool {
    let data = wide(text);
    // SAFETY: the clipboard is opened and closed on this thread; the global memory is filled while
    // locked and handed to the clipboard, which then owns it (freed by us only if that fails).
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        EmptyClipboard();
        let bytes = data.len() * 2;
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        let mut ok = false;
        if !handle.is_null() {
            let target = GlobalLock(handle) as *mut u16;
            if !target.is_null() {
                std::ptr::copy_nonoverlapping(data.as_ptr(), target, data.len());
                GlobalUnlock(handle);
                ok = !SetClipboardData(CF_UNICODETEXT, handle).is_null();
            }
            if !ok {
                GlobalFree(handle);
            }
        }
        CloseClipboard();
        ok
    }
}
