use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, Arc};
use std::thread::ThreadId;

use wry::dpi::{LogicalPosition, LogicalSize};
use wry::raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
use wry::{Rect, WebView, WebViewBuilder};

#[cfg(target_os = "linux")]
use std::sync::mpsc;
#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use std::os::raw::c_ulong;
#[cfg(target_os = "linux")]
use wry::raw_window_handle::XlibWindowHandle;

#[cfg(target_os = "macos")]
use std::ffi::c_void;
#[cfg(target_os = "macos")]
use std::ffi::CStr;
#[cfg(target_os = "macos")]
use std::ptr::NonNull;
#[cfg(target_os = "macos")]
use wry::raw_window_handle::AppKitWindowHandle;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyClass, AnyObject};
#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2::msg_send;

#[cfg(target_os = "windows")]
use std::num::NonZeroIsize;
#[cfg(target_os = "windows")]
use wry::raw_window_handle::Win32WindowHandle;

#[cfg(target_os = "macos")]
use dispatch2::{DispatchQueue, run_on_main};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum WebViewError {
    #[error("unsupported platform for native webview")]
    UnsupportedPlatform,
    #[error("invalid parent window handle")]
    InvalidWindowHandle,
    #[error("webview {0} not found")]
    WebViewNotFound(u64),
    #[error("webview {0} must be accessed from the creating thread")]
    WrongThread(u64),
    #[error("wry error: {0}")]
    WryError(String),
    #[error("gtk initialization failed: {0}")]
    GtkInit(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<wry::Error> for WebViewError {
    fn from(error: wry::Error) -> Self {
        WebViewError::WryError(error.to_string())
    }
}

struct RawWindow {
    raw: RawWindowHandle,
}

impl HasWindowHandle for RawWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        unsafe { Ok(WindowHandle::borrow_raw(self.raw)) }
    }
}

struct WebViewState {
    is_loading: AtomicBool,
    current_url: Mutex<String>,
}

struct WebViewEntry {
    ptr: *mut WebView,
    thread_id: ThreadId,
    state: Arc<WebViewState>,
}

impl Clone for WebViewEntry {
    fn clone(&self) -> Self {
        WebViewEntry {
            ptr: self.ptr,
            thread_id: self.thread_id,
            state: Arc::clone(&self.state),
        }
    }
}

// The raw pointer is only dereferenced on the creating thread (checked at runtime).
unsafe impl Send for WebViewEntry {}
unsafe impl Sync for WebViewEntry {}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static WEBVIEWS: OnceLock<Mutex<HashMap<u64, WebViewEntry>>> = OnceLock::new();

fn webviews() -> &'static Mutex<HashMap<u64, WebViewEntry>> {
    WEBVIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(target_os = "linux")]
type GtkTask = Box<dyn FnOnce() + Send + 'static>;

#[cfg(target_os = "linux")]
struct GtkRunner {
    sender: mpsc::Sender<GtkTask>,
    init_error: Option<String>,
}

#[cfg(target_os = "linux")]
static GTK_RUNNER: OnceLock<GtkRunner> = OnceLock::new();

#[cfg(target_os = "linux")]
fn gtk_runner() -> Result<&'static GtkRunner, WebViewError> {
    let runner = GTK_RUNNER.get_or_init(|| {
        let (task_tx, task_rx) = mpsc::channel::<GtkTask>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        std::thread::spawn(move || {
            let init_result = gtk::init().map_err(|err| err.to_string());
            let _ = init_tx.send(init_result.clone());
            if init_result.is_err() {
                return;
            }
            loop {
                while let Ok(task) = task_rx.try_recv() {
                    task();
                }
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                }
                std::thread::sleep(Duration::from_millis(8));
            }
        });
        let init_result = init_rx
            .recv()
            .unwrap_or_else(|_| Err("gtk init thread failed".to_string()));
        GtkRunner {
            sender: task_tx,
            init_error: init_result.err(),
        }
    });
    if let Some(err) = runner.init_error.as_ref() {
        return Err(WebViewError::GtkInit(err.clone()));
    }
    Ok(runner)
}

#[cfg(target_os = "linux")]
fn run_on_gtk_thread<F, R>(f: F) -> Result<R, WebViewError>
where
    F: FnOnce() -> Result<R, WebViewError> + Send + 'static,
    R: Send + 'static,
{
    let runner = gtk_runner()?;
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    runner
        .sender
        .send(Box::new(move || {
            let result = f();
            let _ = result_tx.send(result);
        }))
        .map_err(|_| WebViewError::Internal("gtk runner stopped".to_string()))?;
    result_rx
        .recv()
        .map_err(|_| WebViewError::Internal("gtk runner stopped".to_string()))?
}

fn with_webview<F, R>(id: u64, f: F) -> Result<R, WebViewError>
where
    F: FnOnce(&WebView) -> Result<R, WebViewError>,
{
    let (ptr, thread_id) = {
        let map = webviews()
            .lock()
            .map_err(|_| WebViewError::Internal("webview registry lock poisoned".to_string()))?;
        let entry = map
            .get(&id)
            .ok_or(WebViewError::WebViewNotFound(id))?;
        (entry.ptr, entry.thread_id)
    };

    if thread_id != std::thread::current().id() {
        return Err(WebViewError::WrongThread(id));
    }

    let webview = unsafe { &*ptr };
    f(webview)
}

fn make_bounds(x: i32, y: i32, width: i32, height: i32) -> Rect {
    let width = width.max(1);
    let height = height.max(1);
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(width, height).into(),
    }
}

#[cfg(target_os = "macos")]
fn run_on_main_thread<F, R>(f: F) -> Result<R, WebViewError>
where
    F: FnOnce() -> Result<R, WebViewError> + Send + 'static,
    R: Send + 'static,
{
    run_on_main(|_| f())
}

#[cfg(not(target_os = "macos"))]
fn run_on_main_thread<F, R>(f: F) -> Result<R, WebViewError>
where
    F: FnOnce() -> Result<R, WebViewError>,
{
    f()
}

fn raw_window_handle_from(parent_handle: u64) -> Result<RawWindowHandle, WebViewError> {
    if parent_handle == 0 {
        return Err(WebViewError::InvalidWindowHandle);
    }

    #[cfg(target_os = "windows")]
    {
        let hwnd =
            NonZeroIsize::new(parent_handle as isize).ok_or(WebViewError::InvalidWindowHandle)?;
        let handle = RawWindowHandle::Win32(Win32WindowHandle::new(hwnd));
        eprintln!("[wrywebview] raw_window_handle Win32=0x{:x}", parent_handle);
        return Ok(handle);
    }

    #[cfg(target_os = "macos")]
    {
        let ns_view = appkit_ns_view_from_handle(parent_handle)?;
        let handle = RawWindowHandle::AppKit(AppKitWindowHandle::new(ns_view));
        eprintln!(
            "[wrywebview] raw_window_handle AppKit=0x{:x} ns_view=0x{:x}",
            parent_handle,
            ns_view.as_ptr() as usize
        );
        return Ok(handle);
    }

    #[cfg(target_os = "linux")]
    {
        let handle =
            RawWindowHandle::Xlib(XlibWindowHandle::new(parent_handle as c_ulong));
        eprintln!("[wrywebview] raw_window_handle Xlib=0x{:x}", parent_handle);
        return Ok(handle);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(WebViewError::UnsupportedPlatform)
    }
}

#[cfg(target_os = "macos")]
fn appkit_ns_view_from_handle(parent_handle: u64) -> Result<NonNull<c_void>, WebViewError> {
    let ptr = NonNull::new(parent_handle as *mut c_void)
        .ok_or(WebViewError::InvalidWindowHandle)?;
    let obj = unsafe { &*(ptr.as_ptr() as *mut AnyObject) };
    let class_name = obj.class().name().to_string_lossy();
    eprintln!("[wrywebview] appkit handle class={}", class_name);

    let nswindow_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"NSWindow\0") };
    let nsview_name = unsafe { CStr::from_bytes_with_nul_unchecked(b"NSView\0") };
    let nswindow_cls =
        AnyClass::get(nswindow_name).ok_or(WebViewError::InvalidWindowHandle)?;
    let nsview_cls =
        AnyClass::get(nsview_name).ok_or(WebViewError::InvalidWindowHandle)?;

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

#[cfg(target_os = "linux")]
fn ensure_gtk_initialized() -> Result<(), WebViewError> {
    gtk::init().map_err(|err| WebViewError::GtkInit(err.to_string()))
}

fn create_webview_inner(
    parent_handle: u64,
    width: i32,
    height: i32,
    url: String,
) -> Result<u64, WebViewError> {
    eprintln!(
        "[wrywebview] create_webview handle=0x{:x} size={}x{} url={}",
        parent_handle, width, height, url
    );
    let raw = raw_window_handle_from(parent_handle)?;
    let window = RawWindow { raw };

    #[cfg(target_os = "linux")]
    ensure_gtk_initialized()?;

    // Create shared state for tracking loading and URL
    let state = Arc::new(WebViewState {
        is_loading: AtomicBool::new(true),
        current_url: Mutex::new(url.clone()),
    });

    let state_for_nav = Arc::clone(&state);
    let state_for_load = Arc::clone(&state);

    let webview = WebViewBuilder::new()
        .with_url(&url)
        .with_bounds(make_bounds(0, 0, width, height))
        .with_navigation_handler(move |new_url| {
            eprintln!("[wrywebview] navigation_handler url={}", new_url);
            // Navigation started
            state_for_nav.is_loading.store(true, Ordering::SeqCst);
            if let Ok(mut current) = state_for_nav.current_url.lock() {
                *current = new_url.clone();
            }
            true // Allow navigation
        })
        .with_on_page_load_handler(move |event, url| {
            match event {
                wry::PageLoadEvent::Started => {
                    eprintln!("[wrywebview] page_load_handler event=Started url={}", url);
                    state_for_load.is_loading.store(true, Ordering::SeqCst);
                }
                wry::PageLoadEvent::Finished => {
                    eprintln!("[wrywebview] page_load_handler event=Finished url={}", url);
                    state_for_load.is_loading.store(false, Ordering::SeqCst);
                    if let Ok(mut current) = state_for_load.current_url.lock() {
                        *current = url.clone();
                    }
                }
            }
        })
        .build_as_child(&window)?;

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let entry = WebViewEntry {
        ptr: Box::into_raw(Box::new(webview)),
        thread_id: std::thread::current().id(),
        state,
    };

    let mut map = webviews()
        .lock()
        .map_err(|_| WebViewError::Internal("webview registry lock poisoned".to_string()))?;
    map.insert(id, entry);
    eprintln!("[wrywebview] create_webview success id={}", id);
    Ok(id)
}

#[uniffi::export]
pub fn create_webview(
    parent_handle: u64,
    width: i32,
    height: i32,
    url: String,
) -> Result<u64, WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || create_webview_inner(parent_handle, width, height, url));
    }
    run_on_main_thread(move || create_webview_inner(parent_handle, width, height, url))
}

fn set_bounds_inner(
    id: u64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), WebViewError> {
    eprintln!(
        "[wrywebview] set_bounds id={} pos=({}, {}) size={}x{}",
        id, x, y, width, height
    );
    let bounds = make_bounds(x, y, width, height);
    with_webview(id, |webview| webview.set_bounds(bounds).map_err(WebViewError::from))
}

#[uniffi::export]
pub fn set_bounds(
    id: u64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), WebViewError> {
    #[cfg(target_os = "macos")]
    {
        if MainThreadMarker::new().is_some() {
            return set_bounds_inner(id, x, y, width, height);
        }
        DispatchQueue::main().exec_async(move || {
            let _ = set_bounds_inner(id, x, y, width, height);
        });
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || set_bounds_inner(id, x, y, width, height));
    }

    #[cfg(target_os = "windows")]
    {
        run_on_main_thread(move || set_bounds_inner(id, x, y, width, height))
    }
}

fn load_url_inner(id: u64, url: String) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] load_url id={} url={}", id, url);
    // Mark as loading before starting navigation
    if let Ok(state) = get_state(id) {
        state.is_loading.store(true, Ordering::SeqCst);
    }
    with_webview(id, |webview| webview.load_url(&url).map_err(WebViewError::from))
}

#[uniffi::export]
pub fn load_url(id: u64, url: String) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || load_url_inner(id, url));
    }
    run_on_main_thread(move || load_url_inner(id, url))
}

fn go_back_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] go_back id={}", id);
    if let Ok(state) = get_state(id) {
        state.is_loading.store(true, Ordering::SeqCst);
    }
    with_webview(id, |webview| {
        webview.evaluate_script("window.history.back()").map_err(WebViewError::from)
    })
}

#[uniffi::export]
pub fn go_back(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || go_back_inner(id));
    }
    run_on_main_thread(move || go_back_inner(id))
}

fn go_forward_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] go_forward id={}", id);
    if let Ok(state) = get_state(id) {
        state.is_loading.store(true, Ordering::SeqCst);
    }
    with_webview(id, |webview| {
        webview.evaluate_script("window.history.forward()").map_err(WebViewError::from)
    })
}

#[uniffi::export]
pub fn go_forward(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || go_forward_inner(id));
    }
    run_on_main_thread(move || go_forward_inner(id))
}

fn reload_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] reload id={}", id);
    if let Ok(state) = get_state(id) {
        state.is_loading.store(true, Ordering::SeqCst);
    }
    with_webview(id, |webview| {
        webview.evaluate_script("window.location.reload()").map_err(WebViewError::from)
    })
}

#[uniffi::export]
pub fn reload(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || reload_inner(id));
    }
    run_on_main_thread(move || reload_inner(id))
}

fn focus_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] focus id={}", id);
    with_webview(id, |webview| {
        // Focus the webview by focusing the document
        webview.evaluate_script("document.documentElement.focus(); window.focus();").map_err(WebViewError::from)
    })
}

#[uniffi::export]
pub fn focus(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || focus_inner(id));
    }
    run_on_main_thread(move || focus_inner(id))
}

fn get_state(id: u64) -> Result<Arc<WebViewState>, WebViewError> {
    let map = webviews()
        .lock()
        .map_err(|_| WebViewError::Internal("webview registry lock poisoned".to_string()))?;
    let entry = map
        .get(&id)
        .ok_or(WebViewError::WebViewNotFound(id))?;
    Ok(Arc::clone(&entry.state))
}

#[uniffi::export]
pub fn get_url(id: u64) -> Result<String, WebViewError> {
    let state = get_state(id)?;
    let url = state.current_url.lock()
        .map_err(|_| WebViewError::Internal("url lock poisoned".to_string()))?;
    Ok(url.clone())
}

#[uniffi::export]
pub fn is_loading(id: u64) -> Result<bool, WebViewError> {
    let state = get_state(id)?;
    Ok(state.is_loading.load(Ordering::SeqCst))
}

fn destroy_webview_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] destroy_webview id={}", id);
    let entry = {
        let mut map = webviews()
            .lock()
            .map_err(|_| WebViewError::Internal("webview registry lock poisoned".to_string()))?;
        let Some(entry) = map.get(&id) else {
            return Ok(());
        };
        if entry.thread_id != std::thread::current().id() {
            return Err(WebViewError::WrongThread(id));
        }
        map.remove(&id)
    };

    let Some(entry) = entry else {
        return Ok(());
    };

    unsafe {
        drop(Box::from_raw(entry.ptr));
    }
    Ok(())
}

#[uniffi::export]
pub fn destroy_webview(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || destroy_webview_inner(id));
    }
    run_on_main_thread(move || destroy_webview_inner(id))
}

#[uniffi::export]
pub fn pump_gtk_events() {
    #[cfg(target_os = "linux")]
    {
        // Events are pumped continuously on the dedicated GTK thread.
    }
}

#[uniffi::export]
pub fn pump_windows_events() {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
        };
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

uniffi::setup_scaffolding!();
