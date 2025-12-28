//! macOS-specific AppKit handling.

use std::ffi::c_void;
use std::ffi::CStr;
use std::ptr::NonNull;

use dispatch2::run_on_main;
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
pub use objc2::MainThreadMarker;
pub use dispatch2::DispatchQueue;

use crate::error::WebViewError;

/// Runs a closure on the main thread using GCD.
pub fn run_on_main_thread<F, R>(f: F) -> Result<R, WebViewError>
where
    F: FnOnce() -> Result<R, WebViewError> + Send + 'static,
    R: Send + 'static,
{
    run_on_main(|_| f())
}

/// Converts a raw handle to an NSView pointer.
///
/// Handles both NSWindow (extracts contentView) and NSView objects.
pub fn appkit_ns_view_from_handle(parent_handle: u64) -> Result<NonNull<c_void>, WebViewError> {
    let ptr = NonNull::new(parent_handle as *mut c_void)
        .ok_or(WebViewError::InvalidWindowHandle)?;
    let obj = unsafe { &*(ptr.as_ptr() as *mut AnyObject) };
    let class_name = obj.class().name().to_string_lossy();
    eprintln!("[wrywebview] appkit handle class={}", class_name);

    let nswindow_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"NSWindow\0") };
    let nsview_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"NSView\0") };
    let nswindow_cls = AnyClass::get(nswindow_name).ok_or(WebViewError::InvalidWindowHandle)?;
    let nsview_cls = AnyClass::get(nsview_name).ok_or(WebViewError::InvalidWindowHandle)?;

    unsafe {
        if msg_send![obj, isKindOfClass: nswindow_cls] {
            let view: *mut AnyObject = msg_send![obj, contentView];
            let view = NonNull::new(view).ok_or(WebViewError::InvalidWindowHandle)?;
            eprintln!(
                "[wrywebview] appkit handle is NSWindow, contentView=0x{:x}",
                view.as_ptr() as usize
            );
            return Ok(view.cast());
        }
        if msg_send![obj, isKindOfClass: nsview_cls] {
            return Ok(ptr);
        }
    }

    Err(WebViewError::InvalidWindowHandle)
}
