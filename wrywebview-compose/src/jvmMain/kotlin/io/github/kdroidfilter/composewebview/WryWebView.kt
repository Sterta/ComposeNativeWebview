package io.github.kdroidfilter.composewebview

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.awt.SwingPanel
import io.github.kdroidfilter.composewebview.wry.WryWebViewPanel
import kotlinx.coroutines.delay

/**
 * A composable that displays a WebView with the given URL.
 *
 * @param url The URL to load
 * @param modifier The modifier to apply to this layout
 */
@Composable
fun WryWebView(url: String, modifier: Modifier = Modifier) {
    SwingPanel(
        modifier = modifier,
        factory = { WryWebViewPanel(url) },
        update = { it.loadUrl(url) },
    )
}

/**
 * A composable that displays a WebView controlled by a [WebViewState].
 *
 * @param state The [WebViewState] that controls this WebView
 * @param modifier The modifier to apply to this layout
 */
@Composable
fun WryWebView(state: WebViewState, modifier: Modifier = Modifier) {
    // Periodically refresh the state to update currentUrl and isLoading
    LaunchedEffect(state) {
        while (true) {
            delay(250) // Poll frequently for better responsiveness
            state.refreshState()
        }
    }

    DisposableEffect(Unit) {
        onDispose {
            state.panel = null
        }
    }

    SwingPanel(
        modifier = modifier,
        factory = {
            WryWebViewPanel(state.url).also { panel ->
                state.panel = panel
            }
        },
        update = { panel ->
            if (state.panel != panel) {
                state.panel = panel
            }
            // Load new URL if it changed
            panel.loadUrl(state.url)
        },
    )
}
