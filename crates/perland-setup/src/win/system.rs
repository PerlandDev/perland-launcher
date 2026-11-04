//! The Windows calls the setup needs: the registry, processes, the UI language, the clipboard and
//! starting the launcher.

use crate::payload::installed_entry;
use crate::text::{lang_from_langid, Lang};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, GlobalFree, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE,
    INVALID_HANDLE_VALUE,
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
use windows_sys::Win32::System::Threading::{
    CreateMutexW, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow};

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
    installed_entry(
        uninstall_value("DisplayVersion"),
        uninstall_value("InstallLocation"),
        uninstall_value("MainBinaryName"),
        |exe| exe.is_file(),
    )
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

/// Puts `text` on the clipboard, owned by the window `hwnd` (with no owner, SetClipboardData fails).
pub fn copy_to_clipboard(hwnd: isize, text: &str) -> bool {
    let data = wide(text);
    // SAFETY: the clipboard is opened and closed on this thread; the global memory is filled while
    // locked and handed to the clipboard, which then owns it (freed by us only if that fails).
    unsafe {
        if OpenClipboard(hwnd as _) == 0 {
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

/// A named mutex held while this setup runs, so a second copy can step aside.
pub struct SingleInstance(HANDLE);

impl SingleInstance {
    pub fn claim() -> Option<SingleInstance> {
        Self::claim_named("Local\\PerlandLauncherSetup")
    }

    pub fn claim_named(name: &str) -> Option<SingleInstance> {
        let name = wide(name);
        // SAFETY: the name is a valid NUL-terminated string; the handle is closed in Drop.
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if handle.is_null() {
                // Without a mutex there is no way to tell; run rather than refuse.
                return Some(SingleInstance(handle));
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                return None;
            }
            Some(SingleInstance(handle))
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle came from CreateMutexW and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Brings the other setup's window forward, found by its title.
pub fn focus_window(title: &str) {
    let title = wide(title);
    // SAFETY: the title is a valid NUL-terminated string; a null result is checked.
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            SetForegroundWindow(hwnd);
        }
    }
}

/// A known Windows folder (`SHGetKnownFolderPath`).
fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    let mut raw: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: `raw` receives a CoTaskMem string that is read once and freed here.
    unsafe {
        let hr = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut raw);
        if hr < 0 || raw.is_null() {
            if !raw.is_null() {
                CoTaskMemFree(raw as *const _);
            }
            return None;
        }
        let len = (0..).take_while(|&i| *raw.add(i) != 0).count();
        let path = String::from_utf16_lossy(std::slice::from_raw_parts(raw, len));
        CoTaskMemFree(raw as *const _);
        Some(PathBuf::from(path))
    }
}

/// The folders the setup and the uninstaller name, from Windows.
pub struct Folders {
    pub local_appdata: PathBuf,
    pub roaming_appdata: PathBuf,
    pub desktop: PathBuf,
    pub programs: PathBuf,
}

pub fn folders() -> Folders {
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_LocalAppData, FOLDERID_Programs, FOLDERID_RoamingAppData,
    };
    let env = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_default()
    };
    Folders {
        local_appdata: known_folder(&FOLDERID_LocalAppData).unwrap_or_else(|| env("LOCALAPPDATA")),
        roaming_appdata: known_folder(&FOLDERID_RoamingAppData).unwrap_or_else(|| env("APPDATA")),
        desktop: known_folder(&FOLDERID_Desktop)
            .unwrap_or_else(|| env("USERPROFILE").join("Desktop")),
        programs: known_folder(&FOLDERID_Programs)
            .unwrap_or_else(|| env("APPDATA").join("Microsoft\\Windows\\Start Menu\\Programs")),
    }
}

/// Folders a per-user install can't write to without admin: Program Files (both) and Windows.
pub fn protected_roots() -> Vec<PathBuf> {
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_Windows,
    };
    [
        &FOLDERID_ProgramFiles,
        &FOLDERID_ProgramFilesX86,
        &FOLDERID_Windows,
    ]
    .into_iter()
    .filter_map(known_folder)
    .collect()
}

/// Deletes every Credential Manager entry matching `filter`; how many, or why it failed.
pub fn delete_credentials(filter: &str) -> Result<usize, String> {
    use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredEnumerateW, CredFree, CREDENTIALW,
    };
    let filter = wide(filter);
    let mut count = 0u32;
    let mut creds: *mut *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: CredEnumerateW fills `count` pointers to valid entries, freed once with CredFree;
    // each entry's TargetName is a NUL-terminated string owned by that buffer.
    unsafe {
        if CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut creds) == 0 {
            return if GetLastError() == ERROR_NOT_FOUND {
                Ok(0)
            } else {
                Err(format!("CredEnumerateW failed ({})", GetLastError()))
            };
        }
        let mut deleted = 0;
        let mut failed = 0;
        for i in 0..count as usize {
            let cred = *creds.add(i);
            if CredDeleteW((*cred).TargetName, (*cred).Type, 0) != 0 {
                deleted += 1;
            } else {
                failed += 1;
            }
        }
        CredFree(creds as *const _);
        if failed > 0 {
            Err(format!(
                "{failed} of {count} credentials could not be deleted"
            ))
        } else {
            Ok(deleted)
        }
    }
}

/// Starts a hidden helper (no console window), not waiting for it.
pub fn spawn_hidden(program: &str, args: &[String]) -> bool {
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
    std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .is_ok()
}
