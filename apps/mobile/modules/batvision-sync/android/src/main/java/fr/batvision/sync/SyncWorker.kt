package fr.batvision.sync

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay

private const val CHANNEL = "downloads"
private const val NOTIFICATION_ID = 4201

/** Runs one sync (D63): the whole cycle, even with the app closed. */
class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
  private var foregroundRefused = false

  override suspend fun doWork(): Result {
    val engine = Engine.get(applicationContext)
    if (!engine.signedIn) return Result.success()
    var outcome = engine.sync(::show)
    // The server is encoding for us: look again shortly rather than in a scheduled job,
    // so the download starts as soon as the file is ready (within WorkManager's 10 min).
    var checks = 0
    while (outcome.serverBusy && checks++ < BUSY_CHECKS && !isStopped) {
      delay(BUSY_EVERY_MS)
      outcome = engine.sync(::show)
    }
    Scheduler.after(applicationContext, outcome)
    return Result.success()
  }

  /**
   * The progress notification. Android 12+ may refuse a foreground service to a worker
   * started in the background: the download goes on without the notification.
   */
  private suspend fun show(current: Current?) {
    if (current == null || foregroundRefused) return
    try {
      setForeground(foregroundInfo(current))
    } catch (e: CancellationException) {
      throw e
    } catch (e: Exception) {
      foregroundRefused = true
      Log.w("batvision.sync", "no download notification: foreground refused", e)
    }
  }

  override suspend fun getForegroundInfo(): ForegroundInfo =
    foregroundInfo(Engine.get(applicationContext).current)

  private fun foregroundInfo(current: Current?): ForegroundInfo {
    val notification = notification(applicationContext, current)
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q)
      ForegroundInfo(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
    else ForegroundInfo(NOTIFICATION_ID, notification)
  }

  companion object {
    private const val BUSY_EVERY_MS = 15_000L
    private const val BUSY_CHECKS = 30

    /** The only notification (D72): what is downloading, with Pause. */
    fun notification(context: Context, current: Current?): Notification {
      val manager = context.getSystemService(NotificationManager::class.java)
      if (manager.getNotificationChannel(CHANNEL) == null) {
        manager.createNotificationChannel(
          NotificationChannel(CHANNEL, context.getString(R.string.batvision_channel), NotificationManager.IMPORTANCE_LOW),
        )
      }
      val percent = current?.total?.takeIf { it > 0 }?.let { (current.received * 100 / it).toInt() }
      val pause = PendingIntent.getBroadcast(
        context, 0, Intent(context, PauseReceiver::class.java), PendingIntent.FLAG_IMMUTABLE,
      )
      val open = context.packageManager.getLaunchIntentForPackage(context.packageName)?.let {
        PendingIntent.getActivity(context, 0, it, PendingIntent.FLAG_IMMUTABLE)
      }
      return NotificationCompat.Builder(context, CHANNEL)
        .setSmallIcon(android.R.drawable.stat_sys_download)
        .setContentTitle(context.getString(R.string.batvision_downloading, current?.title ?: "BatVision"))
        .setContentText(percent?.let { "$it %" })
        .setProgress(100, percent ?: 0, percent == null)
        .setOngoing(true)
        .setOnlyAlertOnce(true)
        .setContentIntent(open)
        .addAction(0, context.getString(R.string.batvision_pause), pause)
        .build()
    }
  }
}

/** When syncs run: on demand, every 30 minutes, and when what's waiting can go on. */
object Scheduler {
  private const val NOW = "batvision.sync.now"
  private const val PERIODIC = "batvision.sync.periodic"
  private const val WAITING = "batvision.sync.waiting"

  private fun connected() = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()

  /** The 30-minute sync (D71); Android may delay it. */
  fun start(context: Context) {
    WorkManager.getInstance(context).enqueueUniquePeriodicWork(
      PERIODIC,
      ExistingPeriodicWorkPolicy.KEEP,
      PeriodicWorkRequestBuilder<SyncWorker>(30, TimeUnit.MINUTES).setConstraints(connected()).build(),
    )
  }

  /**
   * A sync right away: app opened, network back, button. No network constraint: Android
   * only counts a network once it reached the internet, and a server on the home network
   * must work without it. The engine handles an unreachable server itself.
   */
  fun now(context: Context) {
    val request = OneTimeWorkRequestBuilder<SyncWorker>()
      .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
      .build()
    WorkManager.getInstance(context).enqueueUniqueWork(NOW, ExistingWorkPolicy.KEEP, request)
  }

  /** Items wait for Wi-Fi or the charger: run again as soon as that's true. */
  fun after(context: Context, outcome: Outcome) {
    val work = WorkManager.getInstance(context)
    if (!outcome.waitingForWifi && !outcome.waitingForCharging && !outcome.serverBusy) {
      work.cancelUniqueWork(WAITING)
      return
    }
    val constraints = Constraints.Builder()
      .setRequiredNetworkType(if (outcome.waitingForWifi) NetworkType.UNMETERED else NetworkType.NOT_REQUIRED)
      .setRequiresCharging(outcome.waitingForCharging)
      .build()
    val request = OneTimeWorkRequestBuilder<SyncWorker>()
      .setConstraints(constraints)
      // The server is encoding for us: look again in a minute, as the desktop app does.
      .setInitialDelay(if (outcome.serverBusy) 1L else 0L, TimeUnit.MINUTES)
      .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 1, TimeUnit.MINUTES)
      .build()
    work.enqueueUniqueWork(WAITING, ExistingWorkPolicy.REPLACE, request)
  }

  fun stop(context: Context) {
    val work = WorkManager.getInstance(context)
    listOf(NOW, PERIODIC, WAITING).forEach(work::cancelUniqueWork)
  }
}
