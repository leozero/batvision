import XCTest

@testable import BatvisionSyncCore

/// Same cases as PlannerTest.kt and the desktop planner (apps/desktop/src/main/planner.test.ts).
final class PlannerTests: XCTestCase {
  private let gb: Int64 = 1_000_000_000

  private func entry(_ id: Int64, _ status: String, profile: String = "max", sizeBytes: Int64? = -1) -> PlanEntry {
    PlanEntry(
      kind: "episode", id: id, profile: profile, audioLanguage: nil, watched: false, status: status,
      variantId: status == "not_requested" ? nil : id * 10, progress: 0,
      sizeBytes: sizeBytes == -1 ? (status == "ready" ? gb : nil) : sizeBytes,
      url: status == "ready" ? "/api/downloads/file/t\(id)" : nil, subtitles: [], title: "Show"
    )
  }

  private func local(_ id: Int64, complete: Bool = true, variantId: Int64? = -1, profile: String = "max", sizeBytes: Int64? = nil) -> LocalRecord {
    LocalRecord(
      key: "episode-\(id)", complete: complete, variantId: variantId == -1 ? id * 10 : variantId,
      profile: profile, audioLanguage: nil, sizeBytes: sizeBytes ?? gb
    )
  }

  private func steps(_ d: Decision) -> [String] {
    d.actions.map {
      switch $0 {
      case let .request(key, _, _): return "request:\(key)"
      case let .download(key, _): return "download:\(key)"
      case let .confirm(key, _): return "confirm:\(key)"
      }
    }
  }

  func testRequestsWaitsForAndDownloadsExpectedItemsInOrder() {
    let d = Planner.decide(plan: [entry(1, "ready"), entry(2, "encoding"), entry(3, "not_requested")], local: [], quotaBytes: nil)
    XCTAssertEqual(steps(d), ["request:episode-3", "download:episode-1"])
    XCTAssertEqual(d.remove, [])
  }

  func testKeepsWhatItHasAndDeletesWhatIsNoLongerExpected() {
    let d = Planner.decide(plan: [entry(1, "downloaded")], local: [local(1), local(2)], quotaBytes: nil)
    XCTAssertEqual(steps(d), [])
    XCTAssertEqual(d.remove, ["episode-2"])
    XCTAssertEqual(d.usedBytes, gb)
  }

  func testKeepsTheLongestPrefixThatFitsTheQuota() {
    let d = Planner.decide(
      plan: [entry(1, "downloaded"), entry(2, "ready"), entry(3, "downloaded")],
      local: [local(1), local(3)],
      quotaBytes: gb * 5 / 2
    )
    // 1 and 2 fit (2 GB); 3 would make 3 GB: deleted, even though it's on disk.
    XCTAssertEqual(steps(d), ["download:episode-2"])
    XCTAssertEqual(d.remove, ["episode-3"])
    XCTAssertEqual(d.overQuota, ["episode-3"])
  }

  func testStopsAtTheFirstItemThatDoesntFitEvenIfALaterOneWould() {
    let d = Planner.decide(plan: [entry(1, "ready", sizeBytes: 3 * gb), entry(2, "ready")], local: [], quotaBytes: 2 * gb)
    XCTAssertEqual(steps(d), [])
    XCTAssertEqual(d.overQuota, ["episode-1", "episode-2"])
  }

  func testRequestsItemsOfUnknownSizeOnlyWhileThereIsRoom() {
    let d = Planner.decide(plan: [entry(1, "downloaded"), entry(2, "not_requested")], local: [local(1)], quotaBytes: gb)
    XCTAssertEqual(steps(d), [])
  }

  func testRequestsAgainAnItemConfirmedEarlierButMissingOnDisk() {
    let d = Planner.decide(plan: [entry(1, "downloaded")], local: [], quotaBytes: nil)
    XCTAssertEqual(d.actions, [.request(key: "episode-1", kind: "episode", id: 1)])
  }

  func testConfirmsACompleteCopyTheServerStillOffers() {
    let d = Planner.decide(plan: [entry(1, "ready")], local: [local(1)], quotaBytes: nil)
    XCTAssertEqual(d.actions, [.confirm(key: "episode-1", variantId: 10)])
  }

  func testReplacesACopyMadeWithAnotherProfile() {
    let d = Planner.decide(plan: [entry(1, "ready", profile: "mobile")], local: [local(1, profile: "max")], quotaBytes: nil)
    XCTAssertEqual(d.remove, ["episode-1"])
    XCTAssertEqual(steps(d), ["download:episode-1"])
  }

  func testKeepsACopyWhenOnlyTheAudioLanguageChanged() {
    var wanted = entry(1, "downloaded")
    wanted.audioLanguage = "fre"
    let copy = LocalRecord(
      key: "episode-1", complete: true, variantId: 10, profile: "max", audioLanguage: "eng", sizeBytes: gb
    )
    let d = Planner.decide(plan: [wanted], local: [copy], quotaBytes: nil)
    XCTAssertEqual(d.remove, [])
    XCTAssertEqual(steps(d), [])
  }

  func testDropsAPartialDownloadFromAnotherVariantKeepsOneFromTheSame() {
    let same = Planner.decide(plan: [entry(1, "ready")], local: [local(1, complete: false, sizeBytes: 100)], quotaBytes: nil)
    XCTAssertEqual(same.remove, [])
    let other = Planner.decide(
      plan: [entry(1, "ready")], local: [local(1, complete: false, variantId: 99, sizeBytes: 100)], quotaBytes: nil
    )
    XCTAssertEqual(other.remove, ["episode-1"])
  }

  func testIgnoresFailedAndUnavailableItems() {
    let d = Planner.decide(plan: [entry(1, "failed"), entry(2, "unavailable")], local: [], quotaBytes: nil)
    XCTAssertEqual(steps(d), [])
  }
}
