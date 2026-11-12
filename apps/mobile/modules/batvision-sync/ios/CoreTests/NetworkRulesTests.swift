import XCTest

@testable import BatvisionSyncCore

/// Same cases as NetworkRulesTest.kt.
final class NetworkRulesTests: XCTestCase {
  private let wifi = Conditions(connected: true, metered: false, charging: false)
  private let mobile = Conditions(connected: true, metered: true, charging: false)

  private func rules(_ mode: MobileData, cap: Int64? = nil, charging: Bool = false) -> Rules {
    Rules(mobileData: mode, capBytes: cap, chargingOnly: charging)
  }

  private func check(_ profile: String, _ rules: Rules, _ now: Conditions, _ used: Int64) -> Wait? {
    NetworkRules.check(profile: profile, rules: rules, now: now, mobileBytesThisMonth: used)
  }

  func testWifiDownloadsAnythingOfflineNothing() {
    XCTAssertNil(check("max", rules(.off), wifi, 0))
    var offline = wifi
    offline.connected = false
    XCTAssertEqual(check("saver", rules(.all), offline, 0), .offline)
  }

  func testMobileDataFollowsTheDevicesMode() {
    XCTAssertEqual(check("saver", rules(.off), mobile, 0), .wifi)
    XCTAssertNil(check("mobile", rules(.light), mobile, 0))
    XCTAssertNil(check("saver", rules(.light), mobile, 0))
    XCTAssertEqual(check("max", rules(.light), mobile, 0), .wifi)
    XCTAssertNil(check("max", rules(.all), mobile, 0))
  }

  func testTheMonthlyCapStopsMobileDataNotWifi() {
    let capped = rules(.all, cap: 2_000)
    XCTAssertNil(check("max", capped, mobile, 1_999))
    XCTAssertEqual(check("max", capped, mobile, 2_000), .mobileCap)
    XCTAssertNil(check("max", capped, wifi, 5_000))
  }

  func testChargingOnlyWaitsForTheChargerOnAnyNetwork() {
    let r = rules(.all, charging: true)
    XCTAssertEqual(check("max", r, wifi, 0), .charging)
    var charging = wifi
    charging.charging = true
    XCTAssertNil(check("max", r, charging, 0))
  }

  func testMonthsFollowTheDevicesCalendar() {
    // 2026-10-31T23:30Z is already November in Paris.
    let date = Date(timeIntervalSince1970: 1_793_489_400)
    XCTAssertEqual(NetworkRules.month(date, timeZone: TimeZone(identifier: "Europe/Paris")!), "2026-11")
    XCTAssertEqual(NetworkRules.month(date, timeZone: TimeZone(identifier: "UTC")!), "2026-10")
  }

  func testTheWaitNamesMatchTheJavaScriptSide() {
    XCTAssertEqual(Wait.mobileCap.rawValue, "mobile_cap")
    XCTAssertEqual(Wait.wifi.rawValue, "wifi")
  }
}
