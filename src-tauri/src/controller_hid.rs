//! Auxiliary MI_02 diagnostics, independent of the game's WinUSB endpoint.
//! TASOLLER Options V2.8 starts/stops this stream with 68 DD C8 / 68 BB F8.
use std::ffi::c_void;
use std::ptr::null_mut;
use crate::controller_usb::hid_paths;

type Handle = *mut c_void;
#[repr(C)]
#[derive(Default)]
struct Overlapped { internal: usize, internal_high: usize, offset: u32, offset_high: u32, event: Handle }

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(path: *const u16, access: u32, share: u32, security: Handle, disposition: u32, flags: u32, template: Handle) -> Handle;
    fn CreateEventW(security: Handle, manual: i32, initial: i32, name: *const u16) -> Handle;
    fn ResetEvent(event: Handle) -> i32;
    fn ReadFile(file: Handle, buffer: *mut u8, size: u32, done: *mut u32, overlapped: *mut Overlapped) -> i32;
    fn WriteFile(file: Handle, buffer: *const u8, size: u32, done: *mut u32, overlapped: *mut Overlapped) -> i32;
    fn WaitForSingleObject(handle: Handle, timeout: u32) -> u32;
    fn CancelIoEx(file: Handle, overlapped: *mut Overlapped) -> i32;
    fn GetOverlappedResult(file: Handle, overlapped: *mut Overlapped, done: *mut u32, wait: i32) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

pub struct TasollerHid { file: Handle, event: Handle }

impl TasollerHid {
    pub fn open() -> Result<Option<Self>, String> {
        for path in hid_paths()? {
            let file = unsafe { CreateFileW(path.as_ptr(), 0xc0000000, 3, null_mut(), 3, 0x40000000, null_mut()) };
            if file as isize == -1 { return Err(std::io::Error::last_os_error().to_string()); }
            let event = unsafe { CreateEventW(null_mut(), 1, 0, std::ptr::null()) };
            if event.is_null() {
                let error = std::io::Error::last_os_error().to_string();
                unsafe { CloseHandle(file); }
                return Err(error);
            }
            let hid = Self { file, event };
            hid.stream(true)?;
            return Ok(Some(hid));
        }
        Ok(None)
    }

    fn transfer(&self, buffer: &mut [u8; 65], write: bool) -> Result<Option<usize>, String> {
        let mut overlapped = Overlapped { event: self.event, ..Default::default() };
        let mut done = 0;
        unsafe { ResetEvent(self.event); }
        let ok = unsafe {
            if write { WriteFile(self.file, buffer.as_ptr(), 65, &mut done, &mut overlapped) }
            else { ReadFile(self.file, buffer.as_mut_ptr(), 65, &mut done, &mut overlapped) }
        };
        if ok == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(997) { return Err(error.to_string()); }
            let wait = unsafe { WaitForSingleObject(self.event, 100) };
            if wait != 0 {
                // Drain cancellation before the overlapped/buffer storage leaves this stack.
                unsafe { CancelIoEx(self.file, &mut overlapped); GetOverlappedResult(self.file, &mut overlapped, &mut done, 1); }
                return if wait == 258 { Ok(None) } else { Err("Falló la espera HID".into()) };
            }
            if unsafe { GetOverlappedResult(self.file, &mut overlapped, &mut done, 0) } == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        Ok(Some(done as usize))
    }

    fn stream(&self, enabled: bool) -> Result<(), String> {
        let mut packet = [0; 65];
        packet[1..4].copy_from_slice(if enabled { &[0x68, 0xdd, 0xc8] } else { &[0x68, 0xbb, 0xf8] });
        if self.transfer(&mut packet, true)? != Some(65) { return Err("No se pudo activar la lectura FN del TASOLLER".into()); }
        Ok(())
    }

    pub fn read(&self) -> Result<Option<Vec<u8>>, String> {
        let mut packet = [0; 65];
        Ok(self.transfer(&mut packet, false)?.map(|size| packet[..size].to_vec()))
    }
}

impl Drop for TasollerHid {
    fn drop(&mut self) {
        let _ = self.stream(false);
        unsafe { CloseHandle(self.event); CloseHandle(self.file); }
    }
}
