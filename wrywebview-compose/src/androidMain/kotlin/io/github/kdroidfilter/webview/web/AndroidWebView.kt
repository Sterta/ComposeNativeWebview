package io.github.kdroidfilter.webview.web

import android.webkit.JavascriptInterface
import android.webkit.WebView
import io.github.kdroidfilter.webview.jsbridge.WebViewJsBridge
import io.github.kdroidfilter.webview.jsbridge.parseJsMessage
import io.github.kdroidfilter.webview.util.KLogger
import kotlinx.coroutines.CoroutineScope

internal class AndroidWebView(
    override val webView: WebView,
    override val scope: CoroutineScope,
    override val webViewJsBridge: WebViewJsBridge?,
) : IWebView {
    init {
        initWebView()
    }

    override fun canGoBack(): Boolean = webView.canGoBack()

    override fun canGoForward(): Boolean = webView.canGoForward()

    override fun loadUrl(url: String, additionalHttpHeaders: Map<String, String>) {
        webView.loadUrl(url, additionalHttpHeaders)
    }

    override suspend fun loadHtml(
        html: String?,
        baseUrl: String?,
        mimeType: String?,
        encoding: String?,
        historyUrl: String?,
    ) {
        if (html == null) return
        webView.loadDataWithBaseURL(baseUrl, html, mimeType, encoding, historyUrl)
    }

    override suspend fun loadHtmlFile(fileName: String, readType: WebViewFileReadType) {
        KLogger.d(tag = "AndroidWebView") { "loadHtmlFile fileName=$fileName readType=$readType" }
        val normalized = fileName.removePrefix("/")
        when (readType) {
            WebViewFileReadType.ASSET_RESOURCES -> {
                // Prefer Compose Multiplatform resources location, fall back to regular android_asset.
                val composeFiles = "file:///android_asset/compose-resources/files/$normalized"
                val composeAssets = "file:///android_asset/compose-resources/assets/$normalized"
                val legacyAssets = "file:///android_asset/$normalized"
                webView.loadUrl(composeFiles)
                // If the asset doesn't exist, Android will show an error page; callers can opt to use COMPOSE_RESOURCE_FILES.
                // Keeping behavior simple and aligned with upstream.
                KLogger.d(tag = "AndroidWebView") { "loadUrl $composeFiles (fallbacks: $composeAssets, $legacyAssets)" }
            }
            WebViewFileReadType.COMPOSE_RESOURCE_FILES -> webView.loadUrl(fileName)
        }
    }

    override fun goBack() = webView.goBack()

    override fun goForward() = webView.goForward()

    override fun reload() = webView.reload()

    override fun stopLoading() = webView.stopLoading()

    override fun evaluateJavaScript(script: String) {
        webView.evaluateJavascript(script, null)
    }

    override fun injectJsBridge() {
        val bridge = webViewJsBridge ?: return
        super.injectJsBridge()
        val js =
            """
            if (window.${bridge.jsBridgeName} && window.androidJsBridge && window.androidJsBridge.call) {
              window.${bridge.jsBridgeName}.postMessage = function (message) {
                window.androidJsBridge.call(message);
              };
            }
            """.trimIndent()
        evaluateJavaScript(js)
    }

    override fun initJsBridge(webViewJsBridge: WebViewJsBridge) {
        webView.addJavascriptInterface(this, "androidJsBridge")
    }

    @JavascriptInterface
    fun call(raw: String) {
        parseJsMessage(raw)?.let { message ->
            webViewJsBridge?.dispatch(message)
        } ?: run {
            KLogger.w(tag = "AndroidWebView") { "Invalid JS message: $raw" }
        }
    }
}
