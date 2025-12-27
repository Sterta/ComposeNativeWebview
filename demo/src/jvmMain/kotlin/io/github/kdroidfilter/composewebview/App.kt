package io.github.kdroidfilter.composewebview

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview

@Composable
fun App() {
    val webViewState = rememberWebViewState("https://google.com")

    Column {
        Text("URL: ${webViewState.currentUrl}")

        if (webViewState.isLoading) {
            CircularProgressIndicator()
        }

        Row(modifier = Modifier.fillMaxWidth()) {
            Button(onClick = { webViewState.goBack() }) { Text("Back") }
            Button(onClick = { webViewState.goForward() }) { Text("Forward") }
            Button(onClick = { webViewState.reload() }) { Text("Reload") }
            Button(onClick = { webViewState.loadUrl("https://github.com") }) {
                Text("Go to GitHub")
            }
        }



        WryWebView(state = webViewState, modifier = Modifier.fillMaxSize())
    }
}