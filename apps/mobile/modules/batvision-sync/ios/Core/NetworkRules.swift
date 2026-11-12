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
