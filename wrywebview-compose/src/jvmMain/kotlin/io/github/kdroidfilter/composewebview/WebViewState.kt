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
    private var lastRequestedUrl: String = initialUrl
    private var hasLoadedOnce: Boolean = false

    /**
     * The target URL to navigate to. Changing this will trigger navigation.
     */
    var url: String by mutableStateOf(initialUrl)

    /**
     * The current URL displayed in the WebView.
     * Updated after navigation completes.
     */
    var currentUrl: String by mutableStateOf("")
        internal set

    /**
     * Whether the WebView is currently loading content.
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
        isLoading = true
        panel?.goBack()
    }

    /**
     * Navigate forward in the browsing history.
     */
    fun goForward() {
        isLoading = true
        panel?.goForward()
    }

    /**
     * Reload the current page.
     */
    fun reload() {
        isLoading = true
        panel?.reload()
    }

    /**
     * Load a new URL.
     */
    fun loadUrl(newUrl: String) {
        url = newUrl
        lastRequestedUrl = newUrl
        isLoading = true
        panel?.loadUrl(newUrl)
    }

    /**
     * Refresh the state by querying the WebView.
     */
    internal fun refreshState() {
        panel?.let { p ->
            if (p.isReady()) {
                // Update current URL
                p.getCurrentUrl()?.let { newUrl ->
                    if (newUrl.isNotEmpty() && newUrl != "about:blank") {
                        val urlChanged = newUrl != currentUrl
                        currentUrl = newUrl

                        // If URL changed or we got a valid URL for the first time, we're done loading
                        if (urlChanged || !hasLoadedOnce) {
                            hasLoadedOnce = true
                            isLoading = false
                        }
                    }
                }

                // Also check native loading state
                if (isLoading && hasLoadedOnce) {
                    val nativeLoading = p.isLoading()
                    if (!nativeLoading) {
                        isLoading = false
                    }
                }
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
