//! TASOLLER PLUS WinUSB input and volatile RGB output; no configuration writes.
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

type Handle = *mut c_void;

#[repr(C)]
struct Guid { a: u32, b: u16, c: u16, d: [u8; 8] }
#[repr(C)]
struct InterfaceData { size: u32, guid: Guid, flags: u32, reserved: usize }

// DeviceInterfaceGUIDs advertised by TASOLLER PLUS's WinUSB interface (MI_00).
const GUID: Guid = Guid { a: 0x1d4b2365, b: 0x4749, c: 0x48ea, d: [0xb3, 0x8a, 0x7c, 0x6f, 0xdd, 0xdd, 0x7e, 0x26] };

#[link(name = "setupapi")]
extern "system" {
    fn SetupDiGetClassDevsW(guid: *const Guid, enumerator: *const u16, window: Handle, flags: u32) -> Handle;
    fn SetupDiEnumDeviceInterfaces(set: Handle, device: Handle, guid: *const Guid, index: u32, data: *mut InterfaceData) -> i32;
    fn SetupDiGetDeviceInterfaceDetailW(set: Handle, data: *const InterfaceData, detail: *mut c_void, size: u32, required: *mut u32, device: Handle) -> i32;
    fn SetupDiDestroyDeviceInfoList(set: Handle) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(path: *const u16, access: u32, share: u32, security: Handle, disposition: u32, flags: u32, template: Handle) -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
}
#[link(name = "winusb")]
extern "system" {
    fn WinUsb_Initialize(file: Handle, interface: *mut Handle) -> i32;
    fn WinUsb_SetPipePolicy(interface: Handle, pipe: u8, policy: u32, length: u32, value: *mut c_void) -> i32;
    fn WinUsb_ReadPipe(interface: Handle, pipe: u8, buffer: *mut u8, length: u32, transferred: *mut u32, overlapped: Handle) -> i32;
    fn WinUsb_WritePipe(interface: Handle, pipe: u8, buffer: *const u8, length: u32, transferred: *mut u32, overlapped: Handle) -> i32;
    fn WinUsb_Free(interface: Handle) -> i32;
}

struct DeviceSet(Handle);
impl Drop for DeviceSet {
    fn drop(&mut self) { unsafe { SetupDiDestroyDeviceInfoList(self.0); } }
}

fn paths(guid: &Guid, identity: &str) -> Result<Vec<Vec<u16>>, String> {
    let set = unsafe { SetupDiGetClassDevsW(guid, null(), null_mut(), 0x12) };
    if set as isize == -1 { return Err(std::io::Error::last_os_error().to_string()); }
    let set = DeviceSet(set);
    let mut paths = Vec::new();
    for index in 0.. {
        let mut data: InterfaceData = unsafe { zeroed() };
        data.size = size_of::<InterfaceData>() as u32;
        if unsafe { SetupDiEnumDeviceInterfaces(set.0, null_mut(), guid, index, &mut data) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(259) { break; } // ERROR_NO_MORE_ITEMS
            return Err(error.to_string());
        }
        let mut required = 0;
        unsafe { SetupDiGetDeviceInterfaceDetailW(set.0, &data, null_mut(), 0, &mut required, null_mut()); }
        if required < 6 { continue; }
        let mut detail = vec![0u16; (required as usize).div_ceil(2)];
        unsafe { std::ptr::write_unaligned(detail.as_mut_ptr().cast::<u32>(), if size_of::<usize>() == 8 { 8 } else { 6 }); }
        if unsafe { SetupDiGetDeviceInterfaceDetailW(set.0, &data, detail.as_mut_ptr().cast(), required, null_mut(), null_mut()) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        // SP_DEVICE_INTERFACE_DETAIL_DATA_W.DevicePath begins at byte 4 on both architectures.
        let path = &detail[2..];
        let end = path.iter().position(|&c| c == 0).ok_or("Ruta USB inválida")?;
        if String::from_utf16_lossy(&path[..end]).to_ascii_lowercase().contains(identity) {
            paths.push(path[..=end].to_vec());
        }
    }
    Ok(paths)
}

pub fn hid_paths() -> Result<Vec<Vec<u16>>, String> {
    let guid = Guid { a: 0x4d1e55b2, b: 0xf16f, c: 0x11cf, d: [0x88, 0xcb, 0, 0x11, 0x11, 0, 0, 0x30] };
    paths(&guid, "vid_0e8f&pid_1231&mi_02")
}

pub struct TasollerUsb { file: Handle, interface: Handle, led_ready: bool }

impl Drop for TasollerUsb {
    fn drop(&mut self) {
        // Clear menu colors before releasing ownership to a game or exiting.
        if self.led_ready {
            let mut off = [0; 114];
            off[..3].copy_from_slice(b"DL\x02");
            let _ = self.write_lights(&off);
        }
        unsafe {
            if !self.interface.is_null() { WinUsb_Free(self.interface); }
            CloseHandle(self.file);
        }
    }
}

impl TasollerUsb {
    pub fn open() -> Result<Option<Self>, String> {
        let mut last_error = None;
        for path in paths(&GUID, "vid_0e8f&pid_1231&mi_00")? {
            // WinUSB requires FILE_FLAG_OVERLAPPED, even for synchronous pipe reads.
            let file = unsafe { CreateFileW(path.as_ptr(), 0xc0000000, 3, null_mut(), 3, 0x40000000, null_mut()) };
            if file as isize == -1 { last_error = Some(std::io::Error::last_os_error().to_string()); continue; }
            let mut usb = Self { file, interface: null_mut(), led_ready: false };
            if unsafe { WinUsb_Initialize(file, &mut usb.interface) } == 0 {
                last_error = Some(std::io::Error::last_os_error().to_string());
                continue;
            }
            let mut timeout = 100u32;
            if unsafe { WinUsb_SetPipePolicy(usb.interface, 0x84, 3, 4, (&mut timeout as *mut u32).cast()) } == 0 {
                last_error = Some(std::io::Error::last_os_error().to_string());
                continue;
            }
            // A lighting failure must not prevent navigation from connecting.
            usb.led_ready = unsafe { WinUsb_SetPipePolicy(usb.interface, 0x03, 3, 4, (&mut timeout as *mut u32).cast()) } != 0;
            return Ok(Some(usb));
        }
        match last_error { Some(e) => Err(e), None => Ok(None) }
    }

    pub fn write_lights(&self, packet: &[u8; 114]) -> Result<(), String> {
        if !self.led_ready { return Err("Salida LED no disponible".into()); }
        let mut length = 0;
        if unsafe { WinUsb_WritePipe(self.interface, 0x03, packet.as_ptr(), 114, &mut length, null_mut()) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if length != 114 { return Err("Escritura LED incompleta".into()); }
        Ok(())
    }

    /// None means a bounded timeout. A lost device is an error and must be reopened.
    pub fn read(&self) -> Result<Option<Vec<u8>>, String> {
        let mut buffer = [0u8; 64];
        let mut length = 0;
        if unsafe { WinUsb_ReadPipe(self.interface, 0x84, buffer.as_mut_ptr(), buffer.len() as u32, &mut length, null_mut()) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(121) { return Ok(None); }
            return Err(error.to_string());
        }
        if length as usize > buffer.len() { return Err("Longitud USB inválida".into()); }
        Ok(Some(buffer[..length as usize].to_vec()))
    }
}
