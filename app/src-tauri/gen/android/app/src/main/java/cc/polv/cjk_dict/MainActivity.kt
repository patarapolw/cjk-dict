package cc.polv.cjk_dict

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.View
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

private const val SYNC_STOP_DELAY_MS = 3000L

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

  /**
   * Called from Rust (src/sync_service.rs), on the main thread: show the dictionary
   * sync's progress and keep the app running for it, or end that with `text == null`.
   * [percent] < 0 shows an indeterminate progress bar.
   */
  @Suppress("unused")
  fun setSyncStatus(text: String?, percent: Int) {
    handler.removeCallbacks(stopSync)
    if (text == null) {
      // The sync is idle for a moment between steps, e.g. after checking for updates and
      // before its downloads are queued. Don't stop then: from the background, Android
      // wouldn't let the service start again. (Here, not in the page: timers in a page
      // in the background are throttled to as little as once a minute.)
      handler.postDelayed(stopSync, SYNC_STOP_DELAY_MS)
      return
    }
    keepRendererRunning(true)
    askForNotifications()
    SyncService.update(this, text, percent)
  }

  private val handler = Handler(Looper.getMainLooper())

  private val stopSync = Runnable {
    keepRendererRunning(false)
    SyncService.stop(this)
  }

  override fun onDestroy() {
    handler.removeCallbacks(stopSync)
    super.onDestroy()
  }

  private var webView: WebView? = null

  /**
   * The sync runs in the page's JavaScript. By default, the web view's renderer process
   * loses its priority once the app is no longer visible, and Android then freezes it
   * even while SyncService keeps the app itself running.
   */
  private fun keepRendererRunning(keep: Boolean) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    // (IMPORTANT, waived when not visible) is the default
    webView?.setRendererPriorityPolicy(WebView.RENDERER_PRIORITY_IMPORTANT, !keep)
  }

  private var askedForNotifications = false

  /** Android 13+: without the permission, the sync runs but its progress isn't shown. */
  private fun askForNotifications() {
    if (askedForNotifications || Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    askedForNotifications = true
    val permission = Manifest.permission.POST_NOTIFICATIONS
    if (ContextCompat.checkSelfPermission(this, permission) != PackageManager.PERMISSION_GRANTED) {
      ActivityCompat.requestPermissions(this, arrayOf(permission), 0)
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)

    webView.setBackgroundColor(0x00000000)
    webView.fitsSystemWindows = true
    this.webView = webView
  }
}
