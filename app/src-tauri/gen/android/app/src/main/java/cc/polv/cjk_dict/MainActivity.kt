package cc.polv.cjk_dict

import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // With edge-to-edge (enforced from Android 15), the WebView would draw under the
    // status and navigation bars, and would not shrink when the keyboard opens.
    // Pad the content view by the system bars, or by the keyboard while it is shown.
    val rootView: View = findViewById(android.R.id.content)

    ViewCompat.setOnApplyWindowInsetsListener(rootView) { v, insets ->
      val systemBars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
      val imeVisible = insets.isVisible(WindowInsetsCompat.Type.ime())
      val imeHeight = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom

      val bottomPadding = if (imeVisible) imeHeight else systemBars.bottom

      v.setPadding(
        systemBars.left,
        systemBars.top,
        systemBars.right,
        bottomPadding
      )
      insets
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)

    webView.setBackgroundColor(0x00000000)
    webView.fitsSystemWindows = true
  }
}
