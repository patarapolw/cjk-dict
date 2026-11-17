package cc.polv.cjk_dict

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/**
 * Keeps the app running while it syncs dictionaries (gigabytes of downloads, then
 * checksumming them), which the web view drives. Without it, Android freezes the app
 * as soon as it's in the background or the screen turns off, and the sync stalls.
 *
 * A foreground service of type `dataSync`, showing the sync's progress as an ongoing
 * notification, with a partial wake lock so the CPU keeps running with the screen off.
 * Started and updated with [update], ended with [stop]; see MainActivity.setSyncStatus.
 */
class SyncService : Service() {
  private var wakeLock: PowerManager.WakeLock? = null

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val text = intent?.getStringExtra(EXTRA_TEXT) ?: getString(R.string.sync_preparing)
    val percent = intent?.getIntExtra(EXTRA_PERCENT, -1) ?: -1

    // must be called within seconds of startForegroundService(), on every start
    ServiceCompat.startForeground(
      this,
      NOTIFICATION_ID,
      notification(text, percent),
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
      } else {
        0
      },
    )
    holdWakeLock()
    running = this
    // If the process dies, the web view that drives the sync goes with it: don't restart
    // the service on its own. The next launch resumes the downloads.
    return START_NOT_STICKY
  }

  /** Update the notification of the running service. */
  private fun refresh(text: String, percent: Int) {
    val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    manager.notify(NOTIFICATION_ID, notification(text, percent))
    holdWakeLock()
  }

  /** (Re)arm the wake lock. The timeout ends it should the web view stop updating us. */
  private fun holdWakeLock() {
    val lock = wakeLock ?: (getSystemService(Context.POWER_SERVICE) as PowerManager)
      .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "cjk-dict:sync")
      .apply { setReferenceCounted(false) }
      .also { wakeLock = it }
    lock.acquire(WAKE_LOCK_TIMEOUT_MS)
  }

  /** Android 15+: `dataSync` services may run for 6 hours a day. */
  override fun onTimeout(startId: Int, fgsType: Int) {
    Log.w(TAG, "foreground service time limit reached, stopping")
    stopSelf()
  }

  override fun onDestroy() {
    if (running === this) running = null
    (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).cancel(NOTIFICATION_ID)
    wakeLock?.let { if (it.isHeld) it.release() }
    wakeLock = null
    super.onDestroy()
  }

  private fun notification(text: String, percent: Int): Notification {
    val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      manager.createNotificationChannel(
        NotificationChannel(
          CHANNEL_ID,
          getString(R.string.sync_channel_name),
          NotificationManager.IMPORTANCE_LOW,
        ),
      )
    }
    val open = PendingIntent.getActivity(
      this,
      0,
      Intent(this, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    return NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.stat_sys_download)
      .setContentTitle(getString(R.string.sync_title))
      .setContentText(text)
      .setProgress(100, percent.coerceIn(0, 100), percent < 0)
      .setContentIntent(open)
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setSilent(true)
      .setCategory(NotificationCompat.CATEGORY_PROGRESS)
      .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
      .build()
  }

  companion object {
    private const val TAG = "SyncService"
    private const val CHANNEL_ID = "sync"
    private const val NOTIFICATION_ID = 1
    private const val EXTRA_TEXT = "text"
    private const val EXTRA_PERCENT = "percent"
    private const val WAKE_LOCK_TIMEOUT_MS = 10 * 60 * 1000L

    /** The started service. Only touched on the main thread, as are its callbacks. */
    private var running: SyncService? = null

    /**
     * Start the service, or update its notification. [percent] < 0 shows an
     * indeterminate progress bar. Call on the main thread.
     */
    fun update(context: Context, text: String, percent: Int) {
      // Not startForegroundService() again: from the background, Android 12+ refuses it.
      running?.let {
        it.refresh(text, percent)
        return
      }
      val intent = Intent(context, SyncService::class.java)
        .putExtra(EXTRA_TEXT, text)
        .putExtra(EXTRA_PERCENT, percent)
      try {
        ContextCompat.startForegroundService(context, intent)
      } catch (e: Exception) {
        // e.g. ForegroundServiceStartNotAllowedException: the app went to the
        // background before the sync started. It then runs while the app is open.
        Log.w(TAG, "could not start the foreground service", e)
      }
    }

    fun stop(context: Context) {
      // no more refresh(): a notification posted while it stops would outlive it
      running = null
      context.stopService(Intent(context, SyncService::class.java))
    }
  }
}
