import XCTest

@testable import BatvisionSyncCore

final class LibraryTests: XCTestCase {
  private var folder: URL!

  override func setUpWithError() throws {
    folder = FileManager.default.temporaryDirectory.appendingPathComponent("batvision-tests-\(UUID().uuidString)")
  }

  override func tearDownWithError() throws {
    try? FileManager.default.removeItem(at: folder)
  }

  private func item(_ key: String, complete: Bool) -> StoredItem {
    StoredItem(
      key: key, kind: "movie", id: Int64(key.split(separator: "-")[1])!, complete: complete, variantId: 7,
      profile: "mobile", audioLanguage: "fra", sizeBytes: 1_000, subtitles: [], meta: StoredMeta(title: "Sintel"),
      positionMs: 0, played: false, positionAt: 0, downloadedAt: nil
    )
  }

  func testKeepsItemsAcrossLaunchesAndForgetsThoseWhoseFilesAreGone() throws {
    let library = Library(folder: folder)
    let done = item("movie-1", complete: true)
    try Data("video".utf8).write(to: library.ensureDir(done.key).appendingPathComponent(videoFile))
    library.put(done)
    library.ensureDir("movie-2")
    library.put(item("movie-2", complete: false))
    library.put(item("movie-3", complete: true))  // no video file

    let reopened = Library(folder: folder)
    XCTAssertEqual(reopened.all().map(\.key), ["movie-1", "movie-2"])
    XCTAssertEqual(reopened.get("movie-1"), done)
  }

  func testCountsADownloadInProgressAsNothingOnDisk() {
    let library = Library(folder: folder)
    library.ensureDir("movie-2")
    library.put(item("movie-2", complete: false))
    XCTAssertEqual(library.records().map(\.sizeBytes), [0])
  }

  func testRemovingDeletesTheFolder() {
    let library = Library(folder: folder)
    let dir = library.ensureDir("movie-1")
    library.put(item("movie-1", complete: false))
    library.remove("movie-1")
    XCTAssertFalse(FileManager.default.fileExists(atPath: dir.path))
    XCTAssertTrue(library.all().isEmpty)
  }

  func testOnlyAcceptsKeysFromTheServer() {
    XCTAssertTrue(Library.validKey("episode-42"))
    XCTAssertFalse(Library.validKey("../movie-1"))
    XCTAssertFalse(Library.validKey("movie-1/.."))
  }

  func testParsesAPlanItem() {
    let entry = PlanParser.entry([
      "kind": "episode", "id": 12, "profile": "mobile", "status": "ready", "variant_id": 99,
      "size_bytes": 1_234, "url": "/api/downloads/file/x", "title": "Show", "season_number": 1,
      "episode_number": 2, "watched": true, "progress": 1.0, "audio_language": NSNull(),
      "subtitles": [["url": "/s.vtt", "language": "fre", "kind": "forced"]],
    ])
    XCTAssertEqual(entry?.key, "episode-12")
    XCTAssertEqual(entry?.variantId, 99)
    XCTAssertNil(entry?.audioLanguage)
    XCTAssertEqual(entry?.subtitles, [PlanSubtitle(url: "/s.vtt", language: "fre", kind: "forced")])
    XCTAssertEqual(entry?.watched, true)
  }
}
