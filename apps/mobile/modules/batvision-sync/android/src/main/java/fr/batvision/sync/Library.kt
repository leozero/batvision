package fr.batvision.sync

import java.io.File
import org.json.JSONArray
import org.json.JSONObject

const val VIDEO = "video.mp4"
const val PARTIAL = "video.mp4.part"
private const val MANIFEST = "batvision.json"
private val KEY = Regex("^(movie|episode)-\\d+$")

data class StoredSubtitle(val file: String, val language: String?, val kind: String)

/** What the offline library shows; images are file names next to the video. */
data class StoredMeta(
  val title: String,
  val episodeTitle: String? = null,
  val seriesId: Long? = null,
  val seasonNumber: Int? = null,
  val episodeNumber: Int? = null,
  val year: Int? = null,
  val overview: String? = null,
  val poster: String? = null,
  val backdrop: String? = null,
  val still: String? = null,
  val durationMs: Long? = null,
)

data class StoredItem(
  val key: String,
  val kind: String,
  val id: Long,
  val complete: Boolean,
  val variantId: Long?,
  val profile: String,
  val audioLanguage: String?,
  /** Final size (complete) or expected size (partial). */
  val sizeBytes: Long,
  val subtitles: List<StoredSubtitle>,
  val meta: StoredMeta,
  val positionMs: Long,
  val played: Boolean,
  /** Unix seconds of the last local or server position update. */
  val positionAt: Long,
  val downloadedAt: Long?,
)

/** What's downloaded, in `<folder>/batvision.json`, one sub-folder per item (as on desktop). */
class Library(folder: File) {
  var folder: File = folder
    private set
  private val items = LinkedHashMap<String, StoredItem>()

  init {
    load()
  }

  @Synchronized
  private fun load() {
    items.clear()
    val file = File(folder, MANIFEST)
    if (file.exists()) {
      runCatching {
        val array = JSONObject(file.readText()).getJSONArray("items")
        for (i in 0 until array.length()) {
          val item = fromJson(array.getJSONObject(i))
          items[item.key] = item
        }
      }
    }
    // Forget items whose files are gone.
    items.values.removeAll { !File(dir(it.key), if (it.complete) VIDEO else PARTIAL).exists() }
  }

  @Synchronized
  private fun save() {
    folder.mkdirs()
    val tmp = File(folder, "$MANIFEST.tmp")
    tmp.writeText(JSONObject().put("items", JSONArray(items.values.map(::toJson))).toString())
    tmp.renameTo(File(folder, MANIFEST))
  }

  @Synchronized fun all(): List<StoredItem> = items.values.toList()

  @Synchronized fun get(key: String): StoredItem? = items[key]

  @Synchronized
  fun put(item: StoredItem) {
    items[item.key] = item
    save()
  }

  /** Folder of an item; keys come from the server, so they're checked. */
  fun dir(key: String): File {
    require(KEY.matches(key)) { "bad item key: $key" }
    return File(folder, key)
  }

  fun ensureDir(key: String) = dir(key).also { it.mkdirs() }

  @Synchronized
  fun remove(key: String) {
    items.remove(key)
    dir(key).deleteRecursively()
    save()
  }

  /** Bytes on disk for each item, as the planner wants them. */
  @Synchronized
  fun records(): Map<String, LocalRecord> = items.values.associate { item ->
    val size = if (item.complete) item.sizeBytes else File(dir(item.key), PARTIAL).let { if (it.exists()) it.length() else 0L }
    item.key to LocalRecord(item.key, item.complete, item.variantId, item.profile, item.audioLanguage, size)
  }

  fun usedBytes() = records().values.sumOf { it.sizeBytes }

  /** Moves every item and the manifest to another folder (phone ↔ SD card). */
  @Synchronized
  fun moveTo(target: File) {
    if (target.absoluteFile == folder.absoluteFile) return
    target.mkdirs()
    folder.listFiles()?.filter { KEY.matches(it.name) || it.name == MANIFEST }?.forEach { from ->
      val to = File(target, from.name)
      if (!from.renameTo(to)) {
        // Another volume: copy, then delete.
        from.copyRecursively(to, overwrite = true)
        from.deleteRecursively()
      }
    }
    folder = target
    load()
  }

  companion object {
    private fun JSONObject.str(name: String): String? = if (isNull(name) || !has(name)) null else getString(name)
    private fun JSONObject.long(name: String): Long? = if (isNull(name) || !has(name)) null else getLong(name)
    private fun JSONObject.int(name: String): Int? = if (isNull(name) || !has(name)) null else getInt(name)

    fun toJson(i: StoredItem): JSONObject = JSONObject()
      .put("key", i.key).put("kind", i.kind).put("id", i.id).put("complete", i.complete)
      .put("variantId", i.variantId ?: JSONObject.NULL).put("profile", i.profile)
      .put("audioLanguage", i.audioLanguage ?: JSONObject.NULL).put("sizeBytes", i.sizeBytes)
      .put("subtitles", JSONArray(i.subtitles.map {
        JSONObject().put("file", it.file).put("language", it.language ?: JSONObject.NULL).put("kind", it.kind)
      }))
      .put("meta", JSONObject()
        .put("title", i.meta.title)
        .put("episodeTitle", i.meta.episodeTitle ?: JSONObject.NULL)
        .put("seriesId", i.meta.seriesId ?: JSONObject.NULL)
        .put("seasonNumber", i.meta.seasonNumber ?: JSONObject.NULL)
        .put("episodeNumber", i.meta.episodeNumber ?: JSONObject.NULL)
        .put("year", i.meta.year ?: JSONObject.NULL)
        .put("overview", i.meta.overview ?: JSONObject.NULL)
        .put("poster", i.meta.poster ?: JSONObject.NULL)
        .put("backdrop", i.meta.backdrop ?: JSONObject.NULL)
        .put("still", i.meta.still ?: JSONObject.NULL)
        .put("durationMs", i.meta.durationMs ?: JSONObject.NULL))
      .put("positionMs", i.positionMs).put("played", i.played).put("positionAt", i.positionAt)
      .put("downloadedAt", i.downloadedAt ?: JSONObject.NULL)

    fun fromJson(o: JSONObject): StoredItem {
      val subs = o.optJSONArray("subtitles") ?: JSONArray()
      val m = o.getJSONObject("meta")
      return StoredItem(
        key = o.getString("key"),
        kind = o.getString("kind"),
        id = o.getLong("id"),
        complete = o.getBoolean("complete"),
        variantId = o.long("variantId"),
        profile = o.getString("profile"),
        audioLanguage = o.str("audioLanguage"),
        sizeBytes = o.getLong("sizeBytes"),
        subtitles = (0 until subs.length()).map { subs.getJSONObject(it) }
          .map { StoredSubtitle(it.getString("file"), it.str("language"), it.getString("kind")) },
        meta = StoredMeta(
          title = m.getString("title"),
          episodeTitle = m.str("episodeTitle"),
          seriesId = m.long("seriesId"),
          seasonNumber = m.int("seasonNumber"),
          episodeNumber = m.int("episodeNumber"),
          year = m.int("year"),
          overview = m.str("overview"),
          poster = m.str("poster"),
          backdrop = m.str("backdrop"),
          still = m.str("still"),
          durationMs = m.long("durationMs"),
        ),
        positionMs = o.optLong("positionMs"),
        played = o.optBoolean("played"),
        positionAt = o.optLong("positionAt"),
        downloadedAt = o.long("downloadedAt"),
      )
    }
  }
}
