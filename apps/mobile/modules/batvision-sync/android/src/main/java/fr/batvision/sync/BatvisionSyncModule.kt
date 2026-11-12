package fr.batvision.sync

import android.media.MediaCodecList
import android.os.Build
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/** `BatvisionSync` in JavaScript: the session, the engine's state and its controls. */
class BatvisionSyncModule : Module() {
  private val context get() = requireNotNull(appContext.reactContext) { "no context" }
  private val engine get() = Engine.get(context)
  private val listener: (Map<String, Any?>) -> Unit = { state -> sendEvent("onState", state) }

  override fun definition() = ModuleDefinition {
    Name("BatvisionSync")
    Events("onState")

    OnStartObserving { engine.addListener(listener) }
    OnStopObserving { engine.removeListener(listener) }

    /** Server and device token; the token lives encrypted in the Android keystore. */
    Function("getSession") {
      mapOf("server" to engine.store.server, "token" to engine.store.token, "deviceId" to engine.store.deviceId)
    }

    Function("setServer") { server: String? -> engine.store.server = server }

    /** Signed in: remember the token and start syncing. */
    Function("signIn") { token: String, deviceId: Long ->
      engine.store.token = token
      engine.store.deviceId = deviceId
      engine.store.paused = false
      Scheduler.start(context)
      Scheduler.now(context)
    }

    /** Signed out or revoked: stop syncing. Downloaded files stay until the next account. */
    Function("signOut") {
      Scheduler.stop(context)
      engine.reset()
    }

    Function("getState") { engine.state() }

    Function("syncNow") {
      if (engine.signedIn) {
        Scheduler.start(context)
        Scheduler.now(context)
      }
    }

    Function("setPaused") { paused: Boolean ->
      engine.setPaused(paused)
      if (!paused) Scheduler.now(context)
    }

    Function("localPlayback") { kind: String, id: Long -> engine.localPlayback(kind, id) }

    Function("reportProgress") { kind: String, id: Long, positionMs: Double, played: Boolean ->
      engine.reportProgress(kind, id, positionMs.toLong(), played)
    }

    Function("capabilities") { capabilities() }

    /** How this phone presents itself (D72): model name, hardware AV1. */
    Function("deviceInfo") {
      val av1 = hardwareAv1()
      engine.store.av1 = av1
      mapOf("model" to Build.MODEL, "manufacturer" to Build.MANUFACTURER, "av1" to av1)
    }
  }

  /** What this phone decodes, in the server's terms (ffprobe codec names). */
  private fun capabilities(): Map<String, Any?> {
    val types = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos
      .filter { !it.isEncoder }.flatMap { it.supportedTypes.toList() }.map { it.lowercase() }.toSet()
    val video = mapOf("video/avc" to "h264", "video/hevc" to "hevc", "video/av01" to "av1", "video/x-vnd.on2.vp9" to "vp9")
    // Media3 decodes these in software when the phone has no decoder.
    val audio = mapOf("audio/ac3" to "ac3", "audio/eac3" to "eac3")
    val metrics = context.resources.displayMetrics
    val longSide = maxOf(metrics.widthPixels, metrics.heightPixels)
    val height = when {
      longSide > 2560 -> 2160L
      longSide > 1920 -> 1440L
      else -> 1080L
    }
    val display = context.getSystemService(android.hardware.display.DisplayManager::class.java)
      .getDisplay(android.view.Display.DEFAULT_DISPLAY)
    val hdr = Build.VERSION.SDK_INT >= Build.VERSION_CODES.N &&
      (display?.hdrCapabilities?.supportedHdrTypes?.isNotEmpty() == true)
    return mapOf(
      "containers" to listOf("mp4", "mkv", "webm", "ts"),
      "video_codecs" to video.filterKeys { it in types }.values.toList(),
      "audio_codecs" to listOf("aac", "mp3", "opus", "flac", "vorbis") + audio.filterKeys { it in types }.values,
      "max_height" to height,
      "max_audio_channels" to 6L,
      "hdr" to hdr,
    )
  }

  private fun hardwareAv1(): Boolean =
    MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.any { info ->
      !info.isEncoder &&
        info.supportedTypes.any { it.equals("video/av01", ignoreCase = true) } &&
        (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) info.isHardwareAccelerated
        else !info.name.startsWith("c2.android") && !info.name.startsWith("OMX.google"))
    }
}
