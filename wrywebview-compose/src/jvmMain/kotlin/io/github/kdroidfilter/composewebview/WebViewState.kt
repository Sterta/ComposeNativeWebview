package io.github.kdroidfilter.composewebview

import androidx.compose.runtime.*
import io.github.kdroidfilter.composewebview.wry.WryWebViewPanel

/**
 * State holder for WebView navigation and loading state.
 *
 * @param initialUrl The initial URL to load
 */
@Stable
class WebViewState(initialUrl: String) {
    internal var panel: WryWebViewPanel? = null

    /**
     * The target URL to navigate to. Changing this will trigger navigation.
     */
    var url: String by mutableStateOf(initialUrl)

    /**
     * The current URL displayed in the WebView.
     * Updated from native state.
     */
    var currentUrl: String by mutableStateOf("")
        internal set

    /**
     * Whether the WebView is currently loading content.
     * Updated from native state via page load handlers.
     */
    var isLoading: Boolean by mutableStateOf(true)
        internal set

    /**
     * Whether the WebView can navigate back in history.
     */
    var canGoBack: Boolean by mutableStateOf(false)
        internal set

    /**
     * Whether the WebView can navigate forward in history.
     */
    var canGoForward: Boolean by mutableStateOf(false)
        internal set

    /**
     * Navigate back in the browsing history.
     */
    fun goBack() {
        panel?.goBack()
    }

    /**
     * Navigate forward in the browsing history.
     */
    fun goForward() {
        panel?.goForward()
    }

    /**
     * Reload the current page.
     */
    fun reload() {
        panel?.reload()
    }

    /**
     * Load a new URL.
     */
    fun loadUrl(newUrl: String) {
        url = newUrl
        panel?.loadUrl(newUrl)
    }

    /**
     * Refresh the state by querying the native WebView state.
     */
    internal fun refreshState() {
        panel?.let { p ->
            if (p.isReady()) {
                // Get current URL from native state
                p.getCurrentUrl()?.let { newUrl ->
                    if (newUrl.isNotEmpty()) {
                        currentUrl = newUrl
                    }
                }
                // Get loading state from native state
                isLoading = p.isLoading()
            }
        }
    }
}

/**
 * Creates and remembers a [WebViewState] instance.
 *
 * @param initialUrl The initial URL to load in the WebView
 * @return A remembered [WebViewState] instance
 */
@Composable
fun rememberWebViewState(initialUrl: String = "about:blank"): WebViewState {
    return remember(initialUrl) { WebViewState(initialUrl) }
}
