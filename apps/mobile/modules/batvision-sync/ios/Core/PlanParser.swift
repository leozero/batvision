import Foundation

/// Reads JSON values the way the server writes them (numbers may come as Int or Double).
enum JSONValue {
  static func string(_ o: [String: Any], _ name: String) -> String? { o[name] as? String }

  static func int64(_ o: [String: Any], _ name: String) -> Int64? {
    switch o[name] {
    case let n as NSNumber: return n.int64Value
    case let n as Int64: return n
    case let n as Int: return Int64(n)
    default: return nil
    }
  }

  static func int(_ o: [String: Any], _ name: String) -> Int? { int64(o, name).map(Int.init) }

  static func double(_ o: [String: Any], _ name: String) -> Double? { (o[name] as? NSNumber)?.doubleValue }

  static func bool(_ o: [String: Any], _ name: String) -> Bool { (o[name] as? NSNumber)?.boolValue ?? false }
}

enum PlanParser {
  /// One item of `POST /api/downloads/plan`.
  static func entry(_ o: [String: Any]) -> PlanEntry? {
    guard let kind = JSONValue.string(o, "kind"), let id = JSONValue.int64(o, "id"),
      let profile = JSONValue.string(o, "profile"), let status = JSONValue.string(o, "status")
    else { return nil }
    let subtitles = (o["subtitles"] as? [[String: Any]] ?? []).compactMap { s -> PlanSubtitle? in
      guard let url = JSONValue.string(s, "url") else { return nil }
      return PlanSubtitle(url: url, language: JSONValue.string(s, "language"), kind: JSONValue.string(s, "kind") ?? "normal")
    }
    return PlanEntry(
      kind: kind,
      id: id,
      profile: profile,
      audioLanguage: JSONValue.string(o, "audio_language"),
      watched: JSONValue.bool(o, "watched"),
      status: status,
      variantId: JSONValue.int64(o, "variant_id"),
      progress: JSONValue.double(o, "progress") ?? 0,
      sizeBytes: JSONValue.int64(o, "size_bytes"),
      url: JSONValue.string(o, "url"),
      subtitles: subtitles,
      title: JSONValue.string(o, "title") ?? "",
      episodeTitle: JSONValue.string(o, "episode_title"),
      seriesId: JSONValue.int64(o, "series_id"),
      seasonNumber: JSONValue.int(o, "season_number"),
      episodeNumber: JSONValue.int(o, "episode_number"),
      posterPath: JSONValue.string(o, "poster_path"),
      durationMs: JSONValue.int64(o, "duration_ms"),
      error: JSONValue.string(o, "error"),
      source: JSONValue.string(o, "source")
    )
  }
}
