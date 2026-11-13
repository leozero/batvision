import Foundation

/// What may download over mobile data (D68).
enum MobileData {
  case off, light, all

  static func parse(_ value: String?) -> MobileData {
    switch value {
    case "light": return .light
    case "all": return .all
    default: return .off
    }
  }
}

/// The device's download rules, from the server's settings.
struct Rules: Equatable {
  let mobileData: MobileData
  let capBytes: Int64?
  let chargingOnly: Bool
}

/// The connection and power right now.
struct Conditions: Equatable {
  var connected: Bool
  /// Cellular or a personal hotspot: iOS calls it "expensive".
  var metered: Bool
  var charging: Bool
}

/// Why a download can't start now. `nil` = it can.
enum Wait: String {
  case offline
  case wifi
  case charging
  case mobileCap = "mobile_cap"
}

enum NetworkRules {
  /// Lighter profiles may use mobile data in `light` mode; `max` waits for Wi-Fi.
  private static let lightProfiles: Set<String> = ["mobile", "saver"]

  static func check(profile: String, rules: Rules, now: Conditions, mobileBytesThisMonth: Int64) -> Wait? {
    if !now.connected { return .offline }
    if rules.chargingOnly && !now.charging { return .charging }
    if !now.metered { return nil }
    return mobileWait(profile: profile, rules: rules, mobileBytesThisMonth: mobileBytesThisMonth)
  }

  /// Whether this download may go on over mobile data; iOS then pauses it by itself on
  /// cellular when it may not (`allowsExpensiveNetworkAccess`).
  static func mobileWait(profile: String, rules: Rules, mobileBytesThisMonth: Int64) -> Wait? {
    switch rules.mobileData {
    case .off: return .wifi
    case .light where !lightProfiles.contains(profile): return .wifi
    default:
      if let cap = rules.capBytes, mobileBytesThisMonth >= cap { return .mobileCap }
      return nil
    }
  }

  /// The calendar month mobile data is counted in, e.g. `2026-11`.
  static func month(_ date: Date, timeZone: TimeZone = .current) -> String {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = timeZone
    let parts = calendar.dateComponents([.year, .month], from: date)
    return String(format: "%04d-%02d", parts.year ?? 0, parts.month ?? 0)
  }
}

/// What to do when a transfer ends without the file.
enum TransferEnd: Equatable {
  /// The server is down or restarting: nothing wrong with the item, try again later.
  case unreachable
  /// The URL is stale: fetch a fresh plan and try once more.
  case refresh
  case failed
}

enum TransferRules {
  /// `refreshed`: a fresh plan was already fetched for this item, without a success since.
  static func end(status: Int, refreshed: Bool) -> TransferEnd {
    // Behind a reverse proxy, a stopped server answers 502/503/504.
    if (502...504).contains(status) { return .unreachable }
    // Signed URL expired or variant gone. Once only: a file the server lost for good
    // would otherwise refetch the plan and retry without end.
    if [403, 404, 410].contains(status) { return refreshed ? .failed : .refresh }
    return .failed
  }

  /// Seconds before retrying after the `attempt`-th error in a row (from 0): further apart
  /// each time, so a server that stays down isn't asked in a loop.
  static func retryDelay(attempt: Int) -> TimeInterval {
    min(15 * pow(2, Double(max(0, min(attempt, 10)))), 300)
  }
}
