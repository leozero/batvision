import AVFoundation
import ExpoModulesCore
import UIKit
import VideoToolbox

/// `BatvisionSync` in JavaScript: the session, the engine's state and its controls. The
/// same interface as the Android module (BatvisionSyncModule.kt).
public class BatvisionSyncModule: Module {
  private var listener: UUID?

  public func definition() -> ModuleDefinition {
    Name("BatvisionSync")
    Events("onState")

    OnStartObserving {
      self.listener = Engine.shared.addListener { [weak self] state in self?.sendEvent("onState", state) }
    }

    OnStopObserving {
      if let id = self.listener { Engine.shared.removeListener(id) }
      self.listener = nil
    }

    /// Server and device token; the token lives in the iOS keychain.
    Function("getSession") { () -> [String: Any] in
      let store = Engine.shared.store
      return [
        "server": store.server ?? NSNull(),
        "token": store.token ?? NSNull(),
        "deviceId": store.deviceId ?? NSNull(),
      ]
    }

    Function("setServer") { (server: String?) in
      Engine.shared.store.server = server
    }

    /// Signed in: remember the token and start syncing.
    Function("signIn") { (token: String, deviceId: Double) in
      let engine = Engine.shared
      engine.store.token = token
      engine.store.deviceId = Int64(deviceId)
      engine.store.paused = false
      engine.requestSync()
      BackgroundSync.schedule()
    }

    /// Signed out or revoked: stop syncing. Downloaded files stay until the next account.
    Function("signOut") {
      BackgroundSync.cancel()
      Engine.shared.reset()
    }

    Function("getState") { () -> [String: Any] in Engine.shared.state() }

    Function("syncNow") { Engine.shared.requestSync() }

    Function("setPaused") { (paused: Bool) in Engine.shared.setPaused(paused) }

    /// Drops an item (D74); then a sync starts the next download.
    AsyncFunction("cancel") { (kind: String, id: Double) in
      try await Engine.shared.cancel(kind: kind, id: Int64(id))
    }

    Function("localPlayback") { (kind: String, id: Double) -> [String: Any]? in
      Engine.shared.localPlayback(kind: kind, id: Int64(id))
    }

    Function("reportProgress") { (kind: String, id: Double, positionMs: Double, played: Bool) in
      Engine.shared.reportProgress(kind: kind, id: Int64(id), positionMs: Int64(positionMs), played: played)
    }

    Function("capabilities") { () -> [String: Any] in Self.capabilities() }

    /// How this device presents itself (D72): model, hardware AV1.
    Function("deviceInfo") { () -> [String: Any] in
      let av1 = Self.hardwareAv1()
      Engine.shared.store.av1 = av1
      return ["model": Self.model(), "manufacturer": "Apple", "av1": av1]
    }
  }

  /// UIKit is read on the main thread; module functions run on the JavaScript thread.
  private static func onMain<T>(_ body: () -> T) -> T {
    Thread.isMainThread ? body() : DispatchQueue.main.sync(execute: body)
  }

  /// "iPad" or "iPhone" (iOS no longer gives apps the device's own name).
  private static func model() -> String {
    onMain { UIDevice.current.model }
  }

  private static func hardwareAv1() -> Bool {
    if #available(iOS 17.0, *) { return VTIsHardwareDecodeSupported(kCMVideoCodecType_AV1) }
    return false
  }

  /// What this device plays, in the server's terms (ffprobe names). AVPlayer reads MP4
  /// and MOV files; anything else comes remuxed as HLS.
  private static func capabilities() -> [String: Any] {
    var video = ["h264", "hevc"]
    if hardwareAv1() { video.append("av1") }
    let (longSide, hdr) = onMain { () -> (CGFloat, Bool) in
      let bounds = UIScreen.main.nativeBounds
      let edr: Bool
      if #available(iOS 16.0, *) { edr = UIScreen.main.potentialEDRHeadroom > 1 } else { edr = false }
      return (max(bounds.width, bounds.height), edr || AVPlayer.eligibleForHDRPlayback)
    }
    let height: Int = longSide > 2560 ? 2160 : longSide > 1920 ? 1440 : 1080
    return [
      "containers": ["mp4", "mov"],
      "video_codecs": video,
      "audio_codecs": ["aac", "mp3", "ac3", "eac3", "flac", "alac"],
      "max_height": height,
      "max_audio_channels": 6,
      "hdr": hdr,
      // AVPlayer ignores subtitles given beside an HLS stream (D88).
      "hls_subtitles": true,
    ]
  }
}
