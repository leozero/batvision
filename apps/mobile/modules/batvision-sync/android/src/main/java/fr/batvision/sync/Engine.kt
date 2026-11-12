package fr.batvision.sync

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.os.Environment
import android.util.Log
import java.io.File
import java.io.FileOutputStream
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONObject

private const val TAG = "batvision.sync"
private val IMAGE_SIZE = mapOf("poster" to "w342", "backdrop" to "w1280", "still" to "w500")

/** What a sync run leaves to do later, for the scheduler. */
data class Outcome(val waitingForWifi: Boolean, val waitingForCharging: Boolean, val serverBusy: Boolean)

/** The download in progress, for the state and the notification. */
data class Current(val key: String, val title: String, val received: Long, val total: Long?)

/**
 * Keeps the download folder in line with the server's expected set, like the desktop
 * engine (apps/desktop/src/main/engine.ts), plus Android's network rules (D68, D69).
 * One instance per process, shared by the sync worker and the module.
 */
class Engine private constructor(private val context: Context) {
  val store = Store(context)
  var library = Library(folderFor(currentStorage()))
    private set
  private val mutex = Mutex()
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
  private var plan: List<PlanEntry> = emptyList()
  private var decision: Decision? = null
  private var quota: Long? = null
  private var online = true
  private var syncing = false
  private var lastSyncAt: Long? = null
  private var lastError: String? = null
  private val waits = mutableMapOf<String, Wait>()
  private val failed = mutableMapOf<String, String>()
  @Volatile var current: Current? = null
    private set
  @Volatile private var cancelDownload = false
  private var flushJob: Job? = null
  private val listeners = mutableSetOf<(Map<String, Any?>) -> Unit>()
  private var lastEmit = 0L

  // ---- Session --------------------------------------------------------------------------

  val signedIn get() = store.server != null && store.token != null

  private fun api() = Api(store.server ?: throw IllegalStateException("no server"), store.token ?: throw IllegalStateException("signed out"))

  /** Signed out (or the token was revoked): stop and forget the plan; files stay. */
  fun reset() {
    cancelDownload = true
    store.clearSession()
    plan = emptyList()
    decision = null
    failed.clear()
    waits.clear()
    emit()
  }

  // ---- Storage ----------------------------------------------------------------------------

  private fun currentStorage() = context.getSharedPreferences("batvision.sync", Context.MODE_PRIVATE).getString("storageCurrent", "internal")!!

  /** The SD card's app folder, when a removable card is mounted. */
  fun sdRoot(): File? = context.getExternalFilesDirs(null).drop(1).firstOrNull { dir ->
    dir != null && Environment.getExternalStorageState(dir) == Environment.MEDIA_MOUNTED && Environment.isExternalStorageRemovable(dir)
  }

  private fun folderFor(storage: String): File =
    if (storage == "sd") File(sdRoot() ?: context.filesDir, "downloads") else File(context.filesDir, "downloads")

  /** The server says where files go: move them when it changes (and the card is there). */
  private fun followStorage(setting: String) {
    val target = if (setting == "sd" && sdRoot() == null) "internal" else setting
    if (target == currentStorage() && library.folder == folderFor(target)) return
    cancelDownload = true
    library.moveTo(folderFor(target))
    context.getSharedPreferences("batvision.sync", Context.MODE_PRIVATE).edit().putString("storageCurrent", target).apply()
  }

  // ---- Conditions -------------------------------------------------------------------------

  fun conditions(): Conditions {
    val cm = context.getSystemService(ConnectivityManager::class.java)
    val caps = cm.activeNetwork?.let(cm::getNetworkCapabilities)
    val connected = caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true
    val metered = caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) != true
    val battery = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
    val status = battery?.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
    val charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL
    return Conditions(connected, metered, charging)
  }

  private fun rules(): Rules {
    val s = store.settings
    return Rules(
      MobileData.parse(s.optString("mobile_data", "off")),
      if (s.has("mobile_cap_bytes") && !s.isNull("mobile_cap_bytes")) s.getLong("mobile_cap_bytes") else null,
      s.optBoolean("charging_only", false),
    )
  }

  private fun month() = NetworkRules.month(System.currentTimeMillis())

  // ---- Sync -------------------------------------------------------------------------------

  /** One full run: progress, plan, deletions, requests, then downloads while allowed. */
  suspend fun sync(onDownload: suspend (Current?) -> Unit = {}): Outcome = mutex.withLock {
    if (!signedIn) return Outcome(false, false, false)
    syncing = true
    cancelDownload = false
    emit()
    try {
      val api = api()
      flushProgress(api)
      var plan = api.plan(store.av1)
      online = true
      store.settings = plan.settings
      quota = if (plan.settings.isNull("quota_bytes")) null else plan.settings.getLong("quota_bytes")
      followStorage(plan.settings.optString("storage", "internal"))
      this.plan = plan.items
      plan = apply(api, plan)
      refreshPositions(api)
      lastSyncAt = System.currentTimeMillis()
      lastError = null
      syncing = false
      emit()
      downloadAll(api, onDownload)
    } catch (e: Unreachable) {
      online = false
      lastError = null
    } catch (e: HttpError) {
      if (e.status == 401) reset() else lastError = e.message
      Log.w(TAG, "sync failed", e)
    } catch (e: Exception) {
      lastError = e.message
      Log.e(TAG, "sync failed", e)
    } finally {
      syncing = false
      current = null
      onDownload(null)
      emit()
    }
    Outcome(
      waitingForWifi = waits.values.any { it == Wait.WIFI || it == Wait.MOBILE_CAP },
      waitingForCharging = waits.values.any { it == Wait.CHARGING },
      serverBusy = this.plan.any { it.status == "queued" || it.status == "encoding" },
    )
  }

  private fun apply(api: Api, plan: Plan): Plan {
    val decision = Planner.decide(plan.items, library.records(), quota)
    this.decision = decision
    for (key in decision.remove) library.remove(key)
    for (action in decision.actions) {
      try {
        when (action) {
          is Action.Request -> api.request(action.kind, action.id)
          is Action.Confirm -> api.confirm(action.variantId)
          is Action.Download -> {}
        }
      } catch (e: HttpError) {
        // 409: no longer expected, or the file can't be read. The next plan says so.
        Log.w(TAG, "${action.key}: ${e.message}")
      }
    }
    // Requests change statuses: show them now rather than at the next sync.
    if (decision.actions.none { it is Action.Request }) return plan
    val fresh = api.plan(store.av1)
    this.plan = fresh.items
    this.decision = Planner.decide(fresh.items, library.records(), quota)
    return fresh
  }

  /** Watched elsewhere since: bring the server's positions to the local copies. */
  private fun refreshPositions(api: Api) {
    val pending = store.pendingProgress()
    for (item in library.all().filter { it.complete }) {
      if (pending.any { it.kind == item.kind && it.id == item.id }) continue
      try {
        val d = api.detail(item.kind, item.id)
        library.put(item.copy(positionMs = d.optLong("position_ms"), played = d.optBoolean("played"), positionAt = now()))
      } catch (e: HttpError) {
        // Gone from the server: the plan drops it.
      }
    }
  }

  private suspend fun downloadAll(api: Api, onDownload: suspend (Current?) -> Unit) {
    waits.clear()
    val downloads = decision?.actions?.filterIsInstance<Action.Download>() ?: return
    for (action in downloads) {
      if (store.paused || cancelDownload) return
      if (library.get(action.key)?.complete == true || action.key in failed) continue
      val wait = NetworkRules.check(action.entry.profile, rules(), conditions(), store.mobileBytes(month()))
      if (wait != null) {
        waits[action.key] = wait
        emit()
        continue
      }
      try {
        download(api, action.entry, onDownload)
      } catch (e: Unreachable) {
        throw e
      } catch (e: HttpError) {
        // Signed URL expired or variant gone: the next plan has fresh URLs.
        if (e.status == 404) return
        failed[action.key] = e.message ?: "download failed"
      } catch (e: StopDownload) {
        e.wait?.let { waits[action.key] = it }
        if (e.wait == null) return
      } catch (e: Exception) {
        failed[action.key] = e.message ?: "download failed"
        Log.e(TAG, "download ${action.key} failed", e)
      } finally {
        current = null
        emit()
      }
    }
  }

  /** Stops a download midway: paused, cancelled, or the network no longer allows it. */
  private class StopDownload(val wait: Wait?) : Exception()

  private suspend fun download(api: Api, entry: PlanEntry, onDownload: suspend (Current?) -> Unit) {
    val key = entry.key
    val dir = library.ensureDir(key)
    val partial = File(dir, PARTIAL)
    val total = entry.sizeBytes
    val stored = library.get(key) ?: StoredItem(
      key = key, kind = entry.kind, id = entry.id, complete = false, variantId = entry.variantId,
      profile = entry.profile, audioLanguage = entry.audioLanguage, sizeBytes = total ?: 0,
      subtitles = emptyList(), meta = metaFromPlan(entry), positionMs = 0, played = false, positionAt = 0, downloadedAt = null,
    )
    library.put(stored)

    var offset = if (partial.exists()) partial.length() else 0L
    current = Current(key, displayTitle(entry), offset, total)
    onDownload(current)
    emit()

    if (total == null || offset < total) {
      api.open(entry.url!!, offset).use { response ->
        // The server ignored the range: start over.
        if (offset > 0 && response.code != 206) offset = 0
        val body = response.body ?: throw HttpError(response.code, "empty body")
        FileOutputStream(partial, offset > 0).use { out ->
          val input = body.byteStream()
          val buffer = ByteArray(256 * 1024)
          var received = offset
          var lastCheck = System.currentTimeMillis()
          var meteredBytes = 0L
          while (true) {
            val n = input.read(buffer)
            if (n < 0) break
            out.write(buffer, 0, n)
            received += n
            meteredBytes += n
            val t = System.currentTimeMillis()
            if (t - lastCheck >= 1000) {
              lastCheck = t
              val now = conditions()
              if (now.metered) store.addMobileBytes(month(), meteredBytes)
              meteredBytes = 0
              if (store.paused || cancelDownload) throw StopDownload(null)
              NetworkRules.check(entry.profile, rules(), now, store.mobileBytes(month()))?.let { throw StopDownload(it) }
              current = Current(key, displayTitle(entry), received, total)
              onDownload(current)
              emit(force = false)
            }
          }
          if (conditions().metered) store.addMobileBytes(month(), meteredBytes)
        }
      }
    }

    // Subtitles are small: always downloaded whole.
    val subtitles = entry.subtitles.mapIndexed { i, sub ->
      val file = "sub-$i.vtt"
      val bytes = api.bytes(sub.url) ?: throw HttpError(404, "subtitle gone")
      File(dir, file).writeBytes(bytes)
      StoredSubtitle(file, sub.language, sub.kind)
    }

    val detail = runCatching { api.detail(entry.kind, entry.id) }.getOrNull()
    val meta = detail?.let { fetchMeta(api, entry, it, dir) } ?: stored.meta

    val video = File(dir, VIDEO)
    partial.renameTo(video)
    library.put(
      stored.copy(
        complete = true, sizeBytes = video.length(), subtitles = subtitles, meta = meta,
        positionMs = detail?.optLong("position_ms") ?: 0, played = detail?.optBoolean("played") ?: false,
        positionAt = now(), downloadedAt = now(),
      ),
    )
    failed.remove(key)
    entry.variantId?.let { runCatching { api.confirm(it) } }
  }

  private fun displayTitle(entry: PlanEntry) =
    if (entry.kind == "episode" && entry.seasonNumber != null && entry.episodeNumber != null)
      "${entry.title} · S%02dE%02d".format(entry.seasonNumber, entry.episodeNumber)
    else entry.title

  private fun metaFromPlan(entry: PlanEntry) = StoredMeta(
    title = entry.title, episodeTitle = entry.episodeTitle, seriesId = entry.seriesId,
    seasonNumber = entry.seasonNumber, episodeNumber = entry.episodeNumber, durationMs = entry.durationMs,
  )

  private fun fetchMeta(api: Api, entry: PlanEntry, d: JSONObject, dir: File): StoredMeta {
    fun str(name: String) = if (d.has(name) && !d.isNull(name)) d.getString(name) else null
    fun image(path: String?, name: String): String? {
      if (path == null) return null
      val bytes = api.bytes("/api/images/${IMAGE_SIZE[name]}$path") ?: return null
      val file = name + path.substring(path.lastIndexOf('.').coerceAtLeast(0))
      File(dir, file).writeBytes(bytes)
      return file
    }
    val episode = entry.kind == "episode"
    val runtime = if (d.has("runtime_minutes") && !d.isNull("runtime_minutes")) d.getLong("runtime_minutes") * 60_000 else null
    return StoredMeta(
      title = (if (episode) str("series_title") else str("title")) ?: entry.title,
      episodeTitle = if (episode) str("title") else null,
      seriesId = if (d.has("series_id") && !d.isNull("series_id")) d.getLong("series_id") else entry.seriesId,
      seasonNumber = if (d.has("season_number") && !d.isNull("season_number")) d.getInt("season_number") else entry.seasonNumber,
      episodeNumber = if (d.has("episode_number") && !d.isNull("episode_number")) d.getInt("episode_number") else entry.episodeNumber,
      year = if (d.has("year") && !d.isNull("year")) d.getInt("year") else null,
      overview = str("overview"),
      poster = image(if (episode) str("series_poster_path") else str("poster_path"), "poster"),
      backdrop = image(if (episode) str("series_backdrop_path") else str("backdrop_path"), "backdrop"),
      still = if (episode) image(str("still_path"), "still") else null,
      durationMs = entry.durationMs ?: (if (d.has("duration_ms") && !d.isNull("duration_ms")) d.getLong("duration_ms") else runtime),
    )
  }

  // ---- Controls ---------------------------------------------------------------------------

  fun setPaused(paused: Boolean) {
    store.paused = paused
    if (paused) cancelDownload = true
    emit()
  }

  // ---- Offline progress -------------------------------------------------------------------

  fun reportProgress(kind: String, id: Long, positionMs: Long, played: Boolean) {
    library.get(itemKey(kind, id))?.let {
      library.put(it.copy(positionMs = positionMs, played = it.played || played, positionAt = now()))
    }
    store.addProgress(PendingProgress(kind, id, positionMs, played, now()))
    Log.i(TAG, "progress $kind $id at ${positionMs / 1000} s kept until sent")
    if (flushJob?.isActive == true) return
    flushJob = scope.launch {
      delay(5_000)
      if (!signedIn) return@launch
      runCatching { flushProgress(api()) }.onFailure { if (it is Unreachable) online = false }
      emit()
    }
  }

  /** Sends offline progress; kept for later when the server can't be reached. */
  private fun flushProgress(api: Api) {
    val pending = store.pendingProgress()
    if (pending.isEmpty()) return
    val batch = pending.take(1000)
    api.progress(batch)
    store.removeProgress(batch)
    Log.i(TAG, "sent ${batch.size} offline progress record(s)")
    online = true
  }

  // ---- Offline playback -------------------------------------------------------------------

  fun localPlayback(kind: String, id: Long): Map<String, Any?>? {
    val item = library.get(itemKey(kind, id))?.takeIf { it.complete } ?: return null
    val dir = library.dir(item.key)
    return mapOf(
      "url" to File(dir, VIDEO).toURI().toString(),
      "subtitles" to item.subtitles.map {
        mapOf("url" to File(dir, it.file).toURI().toString(), "language" to it.language, "kind" to it.kind)
      },
      "durationMs" to item.meta.durationMs,
      "positionMs" to if (item.played) 0L else item.positionMs,
      "audioLanguage" to item.audioLanguage,
      "nextEpisodeId" to nextLocalEpisode(item),
      "meta" to metaMap(item),
    )
  }

  private fun nextLocalEpisode(item: StoredItem): Long? {
    val seriesId = item.meta.seriesId ?: return null
    if (item.kind != "episode") return null
    // Specials (season 0) come after the regular seasons.
    fun order(s: Int?) = if (s == 0) 1_000_000L else (s ?: 0).toLong()
    fun rank(i: StoredItem) = order(i.meta.seasonNumber) * 10_000 + (i.meta.episodeNumber ?: 0)
    val now = rank(item)
    return library.all()
      .filter { it.complete && it.kind == "episode" && it.meta.seriesId == seriesId && rank(it) > now }
      .minByOrNull(::rank)?.id
  }

  // ---- State for the interface ------------------------------------------------------------

  fun addListener(listener: (Map<String, Any?>) -> Unit) = synchronized(listeners) { listeners += listener }

  fun removeListener(listener: (Map<String, Any?>) -> Unit) = synchronized(listeners) { listeners -= listener }

  private fun emit(force: Boolean = true) {
    val t = System.currentTimeMillis()
    if (!force && t - lastEmit < 500) return
    lastEmit = t
    val state = state()
    synchronized(listeners) { listeners.toList() }.forEach { it(state) }
  }

  private fun metaMap(item: StoredItem): Map<String, Any?> {
    val dir = library.dir(item.key)
    fun uri(name: String?) = name?.let { File(dir, it).toURI().toString() }
    return metaMap(item.meta) + mapOf("poster" to uri(item.meta.poster), "backdrop" to uri(item.meta.backdrop), "still" to uri(item.meta.still))
  }

  private fun metaMap(m: StoredMeta) = mapOf(
    "title" to m.title, "episodeTitle" to m.episodeTitle, "seriesId" to m.seriesId,
    "seasonNumber" to m.seasonNumber, "episodeNumber" to m.episodeNumber, "year" to m.year,
    "overview" to m.overview, "poster" to null, "backdrop" to null, "still" to null, "durationMs" to m.durationMs,
  )

  fun state(): Map<String, Any?> {
    val records = library.records()
    val items = mutableListOf<Map<String, Any?>>()
    val seen = mutableSetOf<String>()
    for (entry in plan) {
      seen += entry.key
      items += view(entry.key, entry, records[entry.key]?.sizeBytes ?: 0)
    }
    // Downloaded items the last plan didn't mention (offline start, plan not fetched yet).
    for (stored in library.all()) if (stored.key !in seen) items += view(stored.key, null, stored.sizeBytes)
    val rules = rules()
    return mapOf(
      "signedIn" to signedIn,
      "online" to online,
      "syncing" to syncing,
      "paused" to store.paused,
      "lastSyncAt" to lastSyncAt,
      "lastError" to lastError,
      "storage" to currentStorage(),
      "sdAvailable" to (sdRoot() != null),
      "usedBytes" to records.values.sumOf { it.sizeBytes },
      "quotaBytes" to quota,
      "mobileBytesThisMonth" to store.mobileBytes(month()),
      "mobileCapBytes" to rules.capBytes,
      "items" to items,
    )
  }

  private fun view(key: String, entry: PlanEntry?, onDisk: Long): Map<String, Any?> {
    val stored = library.get(key)
    val now = current?.takeIf { it.key == key }
    val status = when {
      stored?.complete == true -> "done"
      now != null -> "downloading"
      key in failed || entry?.status == "failed" -> "failed"
      else -> "waiting"
    }
    val posterPath = entry?.posterPath
    val server = store.server
    return mapOf(
      "key" to key,
      "kind" to (entry?.kind ?: stored?.kind ?: "movie"),
      "id" to (entry?.id ?: stored?.id ?: 0L),
      "status" to status,
      "serverStatus" to (entry?.status ?: "downloaded"),
      "encodeProgress" to (entry?.progress ?: 0.0),
      "receivedBytes" to (now?.received ?: onDisk),
      "totalBytes" to (if (stored?.complete == true) stored.sizeBytes else now?.total ?: entry?.sizeBytes),
      "profile" to (entry?.profile ?: stored?.profile ?: "max"),
      "audioLanguage" to (entry?.audioLanguage ?: stored?.audioLanguage),
      "watched" to (entry?.watched ?: stored?.played ?: false),
      "overQuota" to (decision?.overQuota?.contains(key) ?: false),
      "wait" to waits[key]?.name?.lowercase(),
      "error" to (failed[key] ?: entry?.error),
      "meta" to if (stored != null) metaMap(stored) else metaMap(StoredMeta(
        title = entry?.title.orEmpty(), episodeTitle = entry?.episodeTitle, seriesId = entry?.seriesId,
        seasonNumber = entry?.seasonNumber, episodeNumber = entry?.episodeNumber, durationMs = entry?.durationMs,
      )) + mapOf("poster" to if (posterPath != null && server != null) "$server/api/images/w342$posterPath" else null),
    )
  }

  companion object {
    @Volatile private var instance: Engine? = null

    fun get(context: Context): Engine =
      instance ?: synchronized(this) { instance ?: Engine(context.applicationContext).also { instance = it } }

    private fun now() = System.currentTimeMillis() / 1000
  }
}
