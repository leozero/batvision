import Foundation

/// A subtitle of a plan item (WebVTT URL).
struct PlanSubtitle: Equatable {
  let url: String
  let language: String?
  let kind: String
}

/// The fields of the server's `PlanItem` the device uses.
struct PlanEntry: Equatable {
  var kind: String
  var id: Int64
  var profile: String
  var audioLanguage: String?
  var watched: Bool
  var status: String
  var variantId: Int64?
  var progress: Double
  var sizeBytes: Int64?
  var url: String?
  var subtitles: [PlanSubtitle]
  var title: String
  var episodeTitle: String? = nil
  var seriesId: Int64? = nil
  var seasonNumber: Int? = nil
  var episodeNumber: Int? = nil
  var posterPath: String? = nil
  var durationMs: Int64? = nil
  var error: String? = nil
  /// `manual`, `rolling`, `season` or `watchlist`: what cancelling removes (D74).
  var source: String? = nil

  var key: String { itemKey(kind, id) }
}

/// What is on disk for an item.
struct LocalRecord: Equatable {
  let key: String
  let complete: Bool
  /// The variant the file (or partial download) comes from.
  let variantId: Int64?
  let profile: String
  let audioLanguage: String?
  /// Bytes on disk.
  let sizeBytes: Int64
}

enum Action: Equatable {
  case request(key: String, kind: String, id: Int64)
  case download(key: String, entry: PlanEntry)
  case confirm(key: String, variantId: Int64)

  var key: String {
    switch self {
    case let .request(key, _, _), let .download(key, _), let .confirm(key, _): return key
    }
  }
}

struct Decision {
  /// Requests and confirmations to send, then downloads in priority order.
  let actions: [Action]
  /// Local items to delete (no longer expected, over quota, or replaced).
  let remove: [String]
  /// Expected items that don't fit in the quota.
  let overQuota: Set<String>
  /// Space the kept items take (or will take once downloaded).
  let usedBytes: Int64
}

func itemKey(_ kind: String, _ id: Int64) -> String { "\(kind)-\(id)" }

/// What the device does with the server's expected set: the same rules as the Kotlin
/// engine (Planner.kt) and the desktop app (apps/desktop/src/main/planner.ts). The server
/// lists items in priority order; keep the longest prefix that fits the quota, delete the
/// rest. An item not encoded yet has no known size: it's requested only while there is
/// room left.
enum Planner {
  private static func matches(_ local: LocalRecord, _ entry: PlanEntry) -> Bool {
    local.profile == entry.profile && local.audioLanguage == entry.audioLanguage
  }

  /// `local` in the library's order.
  static func decide(plan: [PlanEntry], local: [LocalRecord], quotaBytes: Int64?) -> Decision {
    let quota = quotaBytes ?? Int64.max
    let byKey = Dictionary(local.map { ($0.key, $0) }, uniquingKeysWith: { first, _ in first })
    var requests: [Action] = []
    var downloads: [Action] = []
    var kept = Set<String>()
    var overQuota = Set<String>()
    var used: Int64 = 0
    var full = false

    for entry in plan {
      let key = entry.key
      let have = byKey[key]
      let complete = (have?.complete == true && have.map { matches($0, entry) } == true) ? have : nil
      let size = complete?.sizeBytes ?? entry.sizeBytes
      if full || (size.map { used + $0 > quota } ?? false) {
        full = true
        overQuota.insert(key)
        continue
      }

      if let complete {
        kept.insert(key)
        used += complete.sizeBytes
        // The server forgot we have it (or the confirmation got lost).
        if entry.status == "ready", let variant = complete.variantId, entry.variantId == variant {
          requests.append(.confirm(key: key, variantId: variant))
        }
        continue
      }

      switch entry.status {
      case "ready":
        if entry.url != nil, let size {
          kept.insert(key)
          used += size
          downloads.append(.download(key: key, entry: entry))
        }
      case "queued", "encoding":
        if used < quota { kept.insert(key) }
      // Confirmed earlier but missing here (deleted by hand, or another profile).
      case "not_requested", "downloaded":
        if used < quota {
          requests.append(.request(key: key, kind: entry.kind, id: entry.id))
          kept.insert(key)
        }
      // failed or unavailable: nothing to do until the server changes its mind.
      default:
        break
      }
    }

    let remove = local.filter { l in
      guard kept.contains(l.key), let entry = plan.first(where: { $0.key == l.key }) else { return true }
      // A stale copy (other profile or audio) or a partial download of another variant.
      return l.complete ? !matches(l, entry) : l.variantId != entry.variantId
    }.map(\.key)

    return Decision(actions: requests + downloads, remove: remove, overQuota: overQuota, usedBytes: used)
  }
}
