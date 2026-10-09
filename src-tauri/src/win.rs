//! Windows-only: Job Object launch/wait, display mode changes, script runner.

use crate::{target_mode, Display};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsExW, EnumDisplaySettingsW, DEVMODEW, DISP_CHANGE_SUCCESSFUL, DM_DISPLAYFREQUENCY,
    DM_DISPLAYORIENTATION, DM_PELSHEIGHT, DM_PELSWIDTH, ENUM_CURRENT_SETTINGS,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation, QueryInformationJobObject, TerminateJobObject,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, ResumeThread, TerminateProcess, CREATE_NO_WINDOW, CREATE_SUSPENDED, PROCESS_INFORMATION, STARTUPINFOW,
};

fn last_error() -> String {
    std::io::Error::last_os_error().to_string()
}

/// The mod owns this event; absent in other games. Never inject keyboard input.
pub fn request_track_skip() {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> HANDLE;
        fn SetEvent(event: HANDLE) -> i32;
    }
    let name: Vec<u16> = "Local\\RythmFun.ChuniQOL.TrackSkip".encode_utf16().chain([0]).collect();
    unsafe {
        let event = OpenEventW(2, 0, name.as_ptr());
        if !event.is_null() { SetEvent(event); CloseHandle(event); }
    }
}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

fn folder(path: &str) -> Result<&Path, String> {
    Path::new(path).parent().filter(|p| !p.as_os_str().is_empty()).ok_or_else(|| format!("ruta inválida: {path}"))
}

/// Runs a .bat/.cmd/.exe from its folder with no console window and waits for it.
pub fn run_script(path: &str) -> Result<(), String> {
    // ponytail: paths containing `"` break cmd quoting; same limit as the game launch.
    let status = std::process::Command::new("cmd")
        .raw_arg(format!("/S /C \"\"{path}\"\""))
        .current_dir(folder(path)?)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{path} terminó con código {}", status.code().unwrap_or(-1)))
    }
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Starts the game inside a Job Object and blocks until every process it spawned has exited.
/// `.bat`/`.lnk` hand off to other processes and exit early; children inherit the job, so we still see them.
/// Processes that elevate through UAC leave the job: run the launcher as admin if games need it.
pub fn run_and_wait(path: &str, stop_requested: &AtomicBool) -> Result<(), String> {
    let job = unsafe { CreateJobObjectW(null(), null()) };
    if job.is_null() {
        return Err(last_error());
    }
    let job = Handle(job);

    // Same `start` as before (own console for .bat, ShellExecute for .lnk), but suspended until it is in the job.
    let mut cmdline = wide(format!("cmd /C start \"\" \"{path}\"").as_ref());
    let dir = wide(folder(path)?.as_os_str());
    let mut si: STARTUPINFOW = unsafe { zeroed() };
    si.cb = size_of::<STARTUPINFOW>() as u32;
    let mut pi: PROCESS_INFORMATION = unsafe { zeroed() };
    let ok = unsafe {
        CreateProcessW(
            null(),
            cmdline.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_SUSPENDED | CREATE_NO_WINDOW,
            null(),
            dir.as_ptr(),
            &si,
            &mut pi,
        )
    };
    if ok == 0 {
        return Err(last_error());
    }
    let (process, thread) = (Handle(pi.hProcess), Handle(pi.hThread));
    if unsafe { AssignProcessToJobObject(job.0, process.0) } == 0 {
        let e = last_error();
        unsafe { TerminateProcess(process.0, 1) };
        return Err(e);
    }
    unsafe { ResumeThread(thread.0) };

    // Poll stop requests and job completion independently of WebView focus/timers.
    loop {
        if stop_requested.swap(false, Ordering::SeqCst) && unsafe { TerminateJobObject(job.0, 0) } == 0 {
            return Err(last_error());
        }
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        let ok = unsafe {
            QueryInformationJobObject(
                job.0,
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut c_void,
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        if info.ActiveProcesses == 0 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Puts the primary display back to its saved (registry) mode when dropped.
pub struct DisplayGuard(PathBuf);

impl Drop for DisplayGuard {
    fn drop(&mut self) {
        restore_display(&self.0);
    }
}

/// Changes the primary display for the game. The change is dynamic (not saved to the registry),
/// so a reboot always comes back to the normal mode; `marker` covers a launcher crash.
pub fn apply_display(d: &Display, marker: &Path) -> Result<Option<DisplayGuard>, String> {
    if *d == Display::default() {
        return Ok(None);
    }
    let mut dm: DEVMODEW = unsafe { zeroed() };
    dm.dmSize = size_of::<DEVMODEW>() as u16;
    if unsafe { EnumDisplaySettingsW(null(), ENUM_CURRENT_SETTINGS, &mut dm) } == 0 {
        return Err("No se pudo leer el modo de pantalla actual".into());
    }
    let orientation = unsafe { dm.Anonymous1.Anonymous2.dmDisplayOrientation };
    let (w, h, o) = target_mode((dm.dmPelsWidth, dm.dmPelsHeight, orientation), d);
    let hz = if d.hz > 0 { d.hz } else { dm.dmDisplayFrequency };
    if (w, h, o, hz) == (dm.dmPelsWidth, dm.dmPelsHeight, orientation, dm.dmDisplayFrequency) {
        return Ok(None);
    }

    // Marker first: if we die after the change, the next start restores.
    std::fs::write(marker, "").map_err(|e| e.to_string())?;
    let guard = DisplayGuard(marker.to_owned());
    dm.dmPelsWidth = w;
    dm.dmPelsHeight = h;
    dm.dmDisplayFrequency = hz;
    dm.Anonymous1.Anonymous2.dmDisplayOrientation = o;
    dm.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_DISPLAYORIENTATION;
    let r = unsafe { ChangeDisplaySettingsExW(null(), &dm, null_mut(), 0, null()) };
    if r != DISP_CHANGE_SUCCESSFUL {
        return Err(format!("La pantalla no acepta {w}×{h} a {hz} Hz, rotación {}° (código {r})", o * 90));
    }
    Ok(Some(guard))
}

/// Back to the registry mode, if a game changed it. Safe to call when nothing changed.
pub fn restore_display(marker: &Path) {
    if marker.exists() {
        // NULL devmode = return to the mode saved in the registry.
        unsafe { ChangeDisplaySettingsExW(null(), null(), null_mut(), 0, null()) };
        let _ = std::fs::remove_file(marker);
    }
}
