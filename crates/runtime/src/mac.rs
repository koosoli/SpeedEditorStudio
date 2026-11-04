//! macOS system interfaces the runtime uses: CoreFoundation strings and
//! arrays, CoreGraphics keyboard and scroll events, the Accessibility API
//! (menus, the frontmost app) and the keyboard layout (which key types which
//! character). Declared by hand, so no wrapper crate's API can drift.
//!
//! Only partly tested on a Mac (see README, "macOS").

#![allow(non_upper_case_globals, dead_code)]

use std::ffi::{CString, c_char, c_void};
use std::ptr;
use std::sync::{Arc, Mutex};

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFArrayRef = *const c_void;
pub type AXUIElementRef = *const c_void;

const kCFStringEncodingUTF8: u32 = 0x0800_0100;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFGetTypeID(cf: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFStringCreateWithCString(alloc: CFTypeRef, s: *const i8, encoding: u32) -> CFStringRef;
    fn CFStringGetLength(s: CFStringRef) -> isize;
    fn CFStringGetCString(s: CFStringRef, buf: *mut i8, size: isize, encoding: u32) -> bool;
    fn CFArrayGetCount(a: CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(a: CFArrayRef, i: isize) -> CFTypeRef;
    fn CFDataGetBytePtr(d: CFTypeRef) -> *const u8;
    fn CFDictionaryCreate(
        alloc: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    pub static kCFBooleanTrue: CFTypeRef;
}

/// An owned CoreFoundation object, released when dropped.
pub struct Owned(pub CFTypeRef);

impl Owned {
    /// Takes ownership of a "Create"/"Copy" result; None for null.
    pub fn new(p: CFTypeRef) -> Option<Owned> {
        (!p.is_null()).then_some(Owned(p))
    }
    pub fn get(&self) -> CFTypeRef {
        self.0
    }
}

impl Clone for Owned {
    fn clone(&self) -> Owned {
        Owned(unsafe { CFRetain(self.0) })
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

unsafe impl Send for Owned {}

pub fn cfstring(s: &str) -> Owned {
    let c = CString::new(s).unwrap_or_default();
    Owned(unsafe { CFStringCreateWithCString(ptr::null(), c.as_ptr(), kCFStringEncodingUTF8) })
}

/// The Rust string of a CFString (None if it isn't one).
pub fn string_of(cf: CFTypeRef) -> Option<String> {
    unsafe {
        if cf.is_null() || CFGetTypeID(cf) != CFStringGetTypeID() {
            return None;
        }
        let size = CFStringGetLength(cf) * 4 + 1;
        let mut buf = vec![0i8; size as usize];
        CFStringGetCString(cf, buf.as_mut_ptr(), size, kCFStringEncodingUTF8)
            .then(|| std::ffi::CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
    }
}

/// The elements of a CFArray, each retained.
pub fn array_of(cf: CFTypeRef) -> Vec<Owned> {
    unsafe {
        if cf.is_null() || CFGetTypeID(cf) != CFArrayGetTypeID() {
            return Vec::new();
        }
        (0..CFArrayGetCount(cf)).map(|i| Owned(CFRetain(CFArrayGetValueAtIndex(cf, i)))).collect()
    }
}

// ------------------------------------------------------------- CoreGraphics

pub const kCGEventSourceStateHIDSystemState: i32 = 1;
pub const kCGHIDEventTap: u32 = 0;
pub const kCGScrollEventUnitLine: u32 = 1;
pub const kCGEventFlagMaskShift: u64 = 0x0002_0000;
pub const kCGEventFlagMaskControl: u64 = 0x0004_0000;
pub const kCGEventFlagMaskAlternate: u64 = 0x0008_0000;
pub const kCGEventFlagMaskCommand: u64 = 0x0010_0000;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub fn CGEventSourceCreate(state: i32) -> CFTypeRef;
    pub fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CFTypeRef;
    pub fn CGEventSetFlags(event: CFTypeRef, flags: u64);
    pub fn CGEventPost(tap: u32, event: CFTypeRef);
    pub fn CGEventKeyboardSetUnicodeString(event: CFTypeRef, length: usize, chars: *const u16);
    pub fn CGEventCreateScrollWheelEvent(source: CFTypeRef, units: u32, wheel_count: u32, wheel1: i32, ...)
    -> CFTypeRef;
}

// --------------------------------------------------------- Input Monitoring

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOHIDCheckAccess(request_type: u32) -> u32;
    fn IOHIDRequestAccess(request_type: u32) -> bool;
}

const kIOHIDRequestTypeListenEvent: u32 = 1;
const kIOHIDAccessTypeGranted: u32 = 0;

/// Whether the app may read input devices (Input Monitoring permission), which
/// macOS requires for devices it counts as keyboards, such as a Bluetooth Speed
/// Editor. With `prompt`, macOS asks the user if it hasn't been asked yet.
pub fn input_monitoring_allowed(prompt: bool) -> bool {
    unsafe {
        if IOHIDCheckAccess(kIOHIDRequestTypeListenEvent) == kIOHIDAccessTypeGranted {
            return true;
        }
        prompt && IOHIDRequestAccess(kIOHIDRequestTypeListenEvent)
    }
}

// ----------------------------------------------------------------- media keys

// Media and volume keys aren't ordinary key events on a Mac: the system takes
// them as "aux control button" events, which only AppKit can build.
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> *const c_void;
    fn sel_registerName(name: *const c_char) -> *const c_void;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[repr(C)]
struct NSPoint {
    x: f64,
    y: f64,
}

const NS_EVENT_TYPE_SYSTEM_DEFINED: u64 = 14;
const NX_SUBTYPE_AUX_CONTROL_BUTTONS: i16 = 8;

/// Presses or releases a media key; `key` is an NX_KEYTYPE_* value (0 volume
/// up, 1 volume down, 7 mute, 16 play/pause, 17 next, 18 previous).
pub fn post_media_key(key: u32, down: bool) {
    type Make = unsafe extern "C" fn(
        *const c_void,
        *const c_void,
        u64,
        NSPoint,
        u64,
        f64,
        i64,
        *const c_void,
        i16,
        i64,
        i64,
    ) -> *const c_void;
    type Get = unsafe extern "C" fn(*const c_void, *const c_void) -> CFTypeRef;
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let state: i64 = if down { 0xa } else { 0xb };
        let msg = objc_msgSend as unsafe extern "C" fn();
        let make: Make = std::mem::transmute(msg);
        let get: Get = std::mem::transmute(msg);
        let event = make(
            objc_getClass(c"NSEvent".as_ptr()),
            sel_registerName(c"otherEventWithType:location:modifierFlags:timestamp:windowNumber:context:subtype:data1:data2:".as_ptr()),
            NS_EVENT_TYPE_SYSTEM_DEFINED,
            NSPoint { x: 0.0, y: 0.0 },
            (state << 8) as u64,
            0.0,
            0,
            ptr::null(),
            NX_SUBTYPE_AUX_CONTROL_BUTTONS,
            ((key as i64) << 16) | (state << 8),
            -1,
        );
        if !event.is_null() {
            // Owned by the NSEvent, which the pool releases.
            let cg_event = get(event, sel_registerName(c"CGEvent".as_ptr()));
            if !cg_event.is_null() {
                CGEventPost(kCGHIDEventTap, cg_event);
            }
        }
        objc_autoreleasePoolPop(pool);
    }
}

// ------------------------------------------------------------ Accessibility

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    pub fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementSetAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: CFTypeRef) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> i32;
}

/// Whether the app may send keys and use menus (Accessibility permission).
/// With `prompt`, macOS shows its dialog that leads to the setting.
pub fn accessibility_trusted(prompt: bool) -> bool {
    if !prompt {
        return unsafe { AXIsProcessTrusted() };
    }
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = Owned::new(CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        ));
        AXIsProcessTrustedWithOptions(options.as_ref().map_or(ptr::null(), |o| o.get()))
    }
}

/// An attribute of an Accessibility element (None if missing).
pub fn ax_attribute(element: &Owned, name: &str) -> Option<Owned> {
    let mut value: CFTypeRef = ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(element.get(), cfstring(name).get(), &mut value) };
    if err == 0 { Owned::new(value) } else { None }
}

pub fn ax_set_true(element: &Owned, name: &str) -> bool {
    unsafe { AXUIElementSetAttributeValue(element.get(), cfstring(name).get(), kCFBooleanTrue) == 0 }
}

pub fn ax_press(element: &Owned) -> Result<(), i32> {
    match unsafe { AXUIElementPerformAction(element.get(), cfstring("AXPress").get()) } {
        0 => Ok(()),
        err => Err(err),
    }
}

pub fn ax_pid(element: &Owned) -> Option<i32> {
    let mut pid = 0;
    (unsafe { AXUIElementGetPid(element.get(), &mut pid) } == 0).then_some(pid)
}

pub fn ax_title(element: &Owned) -> Option<String> {
    ax_attribute(element, "AXTitle").and_then(|t| string_of(t.get()))
}

pub fn ax_children(element: &Owned) -> Vec<Owned> {
    ax_attribute(element, "AXChildren").map(|c| array_of(c.get())).unwrap_or_default()
}

/// The app that has keyboard focus.
pub fn focused_application() -> Option<Owned> {
    let system = Owned::new(unsafe { AXUIElementCreateSystemWide() })?;
    ax_attribute(&system, "AXFocusedApplication")
}

// ----------------------------------------------------------- keyboard layout

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentKeyboardLayoutInputSource() -> CFTypeRef;
    fn TISGetInputSourceProperty(source: CFTypeRef, key: CFStringRef) -> CFTypeRef;
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const c_void,
        key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_length: usize,
        actual_length: *mut usize,
        chars: *mut u16,
    ) -> i32;
}

/// Shift and Option as UCKeyTranslate wants them ((modifiers >> 8) & 0xff).
pub const UC_SHIFT: u32 = 0x02;
pub const UC_OPTION: u32 = 0x08;

/// Which key (virtual key code) types which character on the current
/// keyboard layout, and with which of Shift / Option. Characters reachable
/// with fewer modifiers win. Read when `capture_layout` last ran; empty if it
/// never could.
pub fn layout_table() -> Arc<Vec<(char, u16, u32)>> {
    LAYOUT.lock().unwrap().clone().unwrap_or_default()
}

type LayoutTable = Arc<Vec<(char, u16, u32)>>;
static LAYOUT: Mutex<Option<LayoutTable>> = Mutex::new(None);

unsafe extern "C" {
    fn pthread_main_np() -> i32;
}

/// Reads the current keyboard layout into `layout_table`. macOS only lets the
/// main thread ask for it (anything else is killed by a dispatch assertion), so
/// on any other thread this does nothing and the previous table stays.
pub fn capture_layout() {
    if unsafe { pthread_main_np() } != 0 {
        *LAYOUT.lock().unwrap() = Some(Arc::new(build_layout_table()));
    }
}

fn build_layout_table() -> Vec<(char, u16, u32)> {
    let mut table: Vec<(char, u16, u32)> = Vec::new();
    unsafe {
        let Some(source) = Owned::new(TISCopyCurrentKeyboardLayoutInputSource()) else { return table };
        let data = TISGetInputSourceProperty(source.get(), kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return table;
        }
        let layout = CFDataGetBytePtr(data) as *const c_void;
        let kbd = LMGetKbdType() as u32;
        for modifiers in [0, UC_SHIFT, UC_OPTION, UC_OPTION | UC_SHIFT] {
            for code in 0u16..0x7f {
                let mut dead = 0u32;
                let mut len = 0usize;
                let mut chars = [0u16; 4];
                // Option bit 0: kUCKeyTranslateNoDeadKeysMask.
                let err = UCKeyTranslate(layout, code, 0, modifiers, kbd, 1, &mut dead, 4, &mut len, chars.as_mut_ptr());
                if err != 0 || len != 1 {
                    continue;
                }
                if let Some(c) = char::from_u32(chars[0] as u32) {
                    let c = c.to_lowercase().next().unwrap_or(c);
                    if !table.iter().any(|(t, ..)| *t == c) {
                        table.push((c, code, modifiers));
                    }
                }
            }
        }
    }
    table
}
