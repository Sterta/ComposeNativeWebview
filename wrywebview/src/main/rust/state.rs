//! WebView state management and registry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::ThreadId;

use wry::WebView;

use crate::error::WebViewError;

/// Tracks the loading state and current URL of a WebView.
pub struct WebViewState {
    pub is_loading: AtomicBool,
    pub current_url: Mutex<String>,
}

impl WebViewState {
    /// Creates a new WebViewState with the given initial URL.
    pub fn new(url: String) -> Self {
        Self {
            is_loading: AtomicBool::new(true),
            current_url: Mutex::new(url),
        }
    }
}

/// Entry in the WebView registry containing the pointer and metadata.
pub struct WebViewEntry {
    pub ptr: *mut WebView,
    pub thread_id: ThreadId,
    pub state: Arc<WebViewState>,
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

/// Returns the global WebView registry.
pub fn webviews() -> &'static Mutex<HashMap<u64, WebViewEntry>> {
    WEBVIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Generates a new unique WebView ID.
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Executes a closure with access to the WebView, ensuring thread safety.
pub fn with_webview<F, R>(id: u64, f: F) -> Result<R, WebViewError>
where
    F: FnOnce(&WebView) -> Result<R, WebViewError>,
{
    let (ptr, thread_id) = {
        let map = webviews()
            .lock()
            .map_err(|_| WebViewError::Internal("webview registry lock poisoned".to_string()))?;
        let entry = map.get(&id).ok_or(WebViewError::WebViewNotFound(id))?;
        (entry.ptr, entry.thread_id)
    };

    if thread_id != std::thread::current().id() {
        return Err(WebViewError::WrongThread(id));
    }

    let webview = unsafe { &*ptr };
    f(webview)
}

/// Retrieves the state for a WebView by ID.
pub fn get_state(id: u64) -> Result<Arc<WebViewState>, WebViewError> {
    let map = webviews()
        .lock()
        .map_err(|_| WebViewError::Internal("webview registry lock poisoned".to_string()))?;
    let entry = map.get(&id).ok_or(WebViewError::WebViewNotFound(id))?;
    Ok(Arc::clone(&entry.state))
}

/// Registers a new WebView in the global registry.
pub fn register(webview: WebView, state: Arc<WebViewState>) -> Result<u64, WebViewError> {
    let id = next_id();
    let entry = WebViewEntry {
        ptr: Box::into_raw(Box::new(webview)),
        thread_id: std::thread::current().id(),
        state,
    };

    let mut map = webviews()
        .lock()
        .map_err(|_| WebViewError::Internal("webview registry lock poisoned".to_string()))?;
    map.insert(id, entry);
    Ok(id)
}

/// Removes and destroys a WebView from the registry.
pub fn unregister(id: u64) -> Result<(), WebViewError> {
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

    if let Some(entry) = entry {
        unsafe {
            drop(Box::from_raw(entry.ptr));
        }
    }

    Ok(())
}
