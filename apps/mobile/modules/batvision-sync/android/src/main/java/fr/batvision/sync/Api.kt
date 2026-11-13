package fr.batvision.sync

import java.io.IOException
import java.util.concurrent.TimeUnit
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import org.json.JSONArray
import org.json.JSONObject

/** The server can't be reached (network down, server stopped). */
class Unreachable(cause: Throwable) : IOException(cause.message, cause)

/** The server answered with an error status. */
class HttpError(val status: Int, message: String) : IOException("HTTP $status: $message")

/** Reads an answer's body: a connection lost midway is the server out of reach, not a failure. */
inline fun <T> reading(block: () -> T): T =
  try {
    block()
  } catch (e: IOException) {
    throw Unreachable(e)
  }

class Plan(val settings: JSONObject, val items: List<PlanEntry>)

/** Calls to the BatVision server with the device token. */
class Api(private val server: String, private val token: String) {
  private val json = "application/json".toMediaType()

  private fun url(path: String) = if (path.startsWith("http")) path else server.trimEnd('/') + path

  private fun call(request: Request, client: OkHttpClient = CLIENT): Response {
    val response =
      try {
        client.newCall(request.newBuilder().header("Authorization", "Bearer $token").build()).execute()
      } catch (e: IOException) {
        throw Unreachable(e)
      }
    // Behind a reverse proxy, a stopped server answers 502/503/504.
    if (response.code in 502..504) {
      response.close()
      throw Unreachable(IOException("server answered ${response.code}"))
    }
    return response
  }

  private fun send(method: String, path: String, body: Any?): String {
    val payload = body?.toString()?.toRequestBody(json)
    val request = Request.Builder().url(url(path)).method(method, payload).build()
    call(request).use { response ->
      val text = reading { response.body?.string() }.orEmpty()
      if (!response.isSuccessful) throw HttpError(response.code, text.take(200))
      return text
    }
  }

  fun plan(av1: Boolean): Plan {
    val o = JSONObject(send("POST", "/api/downloads/plan", JSONObject().put("av1", av1)))
    val items = o.getJSONArray("items")
    return Plan(o.getJSONObject("settings"), (0 until items.length()).map { parseEntry(items.getJSONObject(it)) })
  }

  fun request(kind: String, id: Long) {
    send("POST", "/api/downloads/request", JSONObject().put("kind", kind).put("id", id))
  }

  /** This device drops an item: the server removes what asks for it (D74). */
  fun cancel(kind: String, id: Long) {
    send("POST", "/api/downloads/cancel", JSONObject().put("kind", kind).put("id", id))
  }

  fun confirm(variantId: Long) {
    send("POST", "/api/downloads/confirm", JSONObject().put("variant_id", variantId))
  }

  fun progress(batch: List<PendingProgress>) {
    send("POST", "/api/progress/batch", JSONArray(batch.map(Store::progressJson)))
  }

  /** `/api/movies/{id}` or `/api/episodes/{id}`. */
  fun detail(kind: String, id: Long) = JSONObject(send("GET", "/api/${kind}s/$id", null))

  /** A file, from `offset` when resuming. The caller closes the response. */
  fun open(path: String, offset: Long = 0): Response {
    val builder = Request.Builder().url(url(path))
    if (offset > 0) builder.header("Range", "bytes=$offset-")
    val response = call(builder.build(), DOWNLOADS)
    if (!response.isSuccessful) {
      response.close()
      throw HttpError(response.code, "download failed")
    }
    return response
  }

  fun bytes(path: String): ByteArray? =
    try {
      open(path).use { response -> reading { response.body?.bytes() } }
    } catch (e: HttpError) {
      null
    }

  companion object {
    private val CLIENT =
      OkHttpClient.Builder().connectTimeout(15, TimeUnit.SECONDS).readTimeout(30, TimeUnit.SECONDS).build()

    /** Big files: no overall timeout, only between bytes. */
    private val DOWNLOADS = CLIENT.newBuilder().readTimeout(60, TimeUnit.SECONDS).build()

    private fun JSONObject.str(name: String): String? = if (!has(name) || isNull(name)) null else getString(name)
    private fun JSONObject.long(name: String): Long? = if (!has(name) || isNull(name)) null else getLong(name)
    private fun JSONObject.int(name: String): Int? = if (!has(name) || isNull(name)) null else getInt(name)

    fun parseEntry(o: JSONObject): PlanEntry {
      val subs = o.optJSONArray("subtitles") ?: JSONArray()
      return PlanEntry(
        kind = o.getString("kind"),
        id = o.getLong("id"),
        profile = o.getString("profile"),
        audioLanguage = o.str("audio_language"),
        watched = o.optBoolean("watched"),
        status = o.getString("status"),
        variantId = o.long("variant_id"),
        progress = o.optDouble("progress", 0.0),
        sizeBytes = o.long("size_bytes"),
        url = o.str("url"),
        subtitles = (0 until subs.length()).map { subs.getJSONObject(it) }
          .map { PlanSubtitle(it.getString("url"), it.str("language"), it.optString("kind", "normal")) },
        title = o.optString("title"),
        episodeTitle = o.str("episode_title"),
        seriesId = o.long("series_id"),
        seasonNumber = o.int("season_number"),
        episodeNumber = o.int("episode_number"),
        posterPath = o.str("poster_path"),
        durationMs = o.long("duration_ms"),
        error = o.str("error"),
        source = o.str("source"),
      )
    }
  }
}
