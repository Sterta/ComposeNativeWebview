//! Error types for the WebView library.

/// Errors that can occur when working with WebViews.
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
