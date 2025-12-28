//! Native WebView bindings using wry.
//!
//! This library provides a cross-platform WebView implementation
//! exposed through UniFFI for use from Kotlin/Swift.

mod error;
mod handle;
mod platform;
mod state;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use wry::WebViewBuilder;

pub use error::WebViewError;

use handle::{make_bounds, raw_window_handle_from, RawWindow};
use platform::run_on_main_thread;
use state::{get_state, register, unregister, with_webview, WebViewState};

#[cfg(target_os = "linux")]
use platform::linux::{ensure_gtk_initialized, run_on_gtk_thread};

#[cfg(target_os = "macos")]
use platform::macos::{DispatchQueue, MainThreadMarker};

// ============================================================================
// WebView Creation
// ============================================================================

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

    let state = Arc::new(WebViewState::new(url.clone()));
    let state_for_nav = Arc::clone(&state);
    let state_for_load = Arc::clone(&state);

    let webview = WebViewBuilder::new()
        .with_url(&url)
        .with_bounds(make_bounds(0, 0, width, height))
        .with_navigation_handler(move |new_url| {
            eprintln!("[wrywebview] navigation_handler url={}", new_url);
            state_for_nav.is_loading.store(true, Ordering::SeqCst);
            if let Ok(mut current) = state_for_nav.current_url.lock() {
                *current = new_url.clone();
            }
            true
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

    let id = register(webview, state)?;
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

    #[cfg(not(target_os = "linux"))]
    run_on_main_thread(move || create_webview_inner(parent_handle, width, height, url))
}

// ============================================================================
// Bounds Management
// ============================================================================

fn set_bounds_inner(id: u64, x: i32, y: i32, width: i32, height: i32) -> Result<(), WebViewError> {
    eprintln!(
        "[wrywebview] set_bounds id={} pos=({}, {}) size={}x{}",
        id, x, y, width, height
    );
    let bounds = make_bounds(x, y, width, height);
    with_webview(id, |webview| webview.set_bounds(bounds).map_err(WebViewError::from))
}

#[uniffi::export]
pub fn set_bounds(id: u64, x: i32, y: i32, width: i32, height: i32) -> Result<(), WebViewError> {
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

// ============================================================================
// Navigation
// ============================================================================

fn load_url_inner(id: u64, url: String) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] load_url id={} url={}", id, url);
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

    #[cfg(not(target_os = "linux"))]
    run_on_main_thread(move || load_url_inner(id, url))
}

fn go_back_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] go_back id={}", id);
    if let Ok(state) = get_state(id) {
        state.is_loading.store(true, Ordering::SeqCst);
    }
    with_webview(id, |webview| {
        webview
            .evaluate_script("window.history.back()")
            .map_err(WebViewError::from)
    })
}

#[uniffi::export]
pub fn go_back(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || go_back_inner(id));
    }

    #[cfg(not(target_os = "linux"))]
    run_on_main_thread(move || go_back_inner(id))
}

fn go_forward_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] go_forward id={}", id);
    if let Ok(state) = get_state(id) {
        state.is_loading.store(true, Ordering::SeqCst);
    }
    with_webview(id, |webview| {
        webview
            .evaluate_script("window.history.forward()")
            .map_err(WebViewError::from)
    })
}

#[uniffi::export]
pub fn go_forward(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || go_forward_inner(id));
    }

    #[cfg(not(target_os = "linux"))]
    run_on_main_thread(move || go_forward_inner(id))
}

fn reload_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] reload id={}", id);
    if let Ok(state) = get_state(id) {
        state.is_loading.store(true, Ordering::SeqCst);
    }
    with_webview(id, |webview| {
        webview
            .evaluate_script("window.location.reload()")
            .map_err(WebViewError::from)
    })
}

#[uniffi::export]
pub fn reload(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || reload_inner(id));
    }

    #[cfg(not(target_os = "linux"))]
    run_on_main_thread(move || reload_inner(id))
}

// ============================================================================
// Focus
// ============================================================================

fn focus_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] focus id={}", id);
    with_webview(id, |webview| {
        webview
            .evaluate_script("document.documentElement.focus(); window.focus();")
            .map_err(WebViewError::from)
    })
}

#[uniffi::export]
pub fn focus(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || focus_inner(id));
    }

    #[cfg(not(target_os = "linux"))]
    run_on_main_thread(move || focus_inner(id))
}

// ============================================================================
// State Queries
// ============================================================================

#[uniffi::export]
pub fn get_url(id: u64) -> Result<String, WebViewError> {
    let state = get_state(id)?;
    let url = state
        .current_url
        .lock()
        .map_err(|_| WebViewError::Internal("url lock poisoned".to_string()))?;
    Ok(url.clone())
}

#[uniffi::export]
pub fn is_loading(id: u64) -> Result<bool, WebViewError> {
    let state = get_state(id)?;
    Ok(state.is_loading.load(Ordering::SeqCst))
}

// ============================================================================
// Destruction
// ============================================================================

fn destroy_webview_inner(id: u64) -> Result<(), WebViewError> {
    eprintln!("[wrywebview] destroy_webview id={}", id);
    unregister(id)
}

#[uniffi::export]
pub fn destroy_webview(id: u64) -> Result<(), WebViewError> {
    #[cfg(target_os = "linux")]
    {
        return run_on_gtk_thread(move || destroy_webview_inner(id));
    }

    #[cfg(not(target_os = "linux"))]
    run_on_main_thread(move || destroy_webview_inner(id))
}

// ============================================================================
// Event Pumps
// ============================================================================

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
        platform::windows::pump_events();
    }
}

uniffi::setup_scaffolding!();
