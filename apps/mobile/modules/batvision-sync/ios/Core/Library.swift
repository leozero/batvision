import Foundation

let videoFile = "video.mp4"
/// The finished download, before subtitles and pictures are fetched.
let partialFile = "video.mp4.part"
/// What iOS needs to resume an interrupted download.
let resumeFile = "video.resume"
private let manifestFile = "batvision.json"

struct StoredSubtitle: Codable, Equatable {
  let file: String
  let language: String?
  let kind: String
}

/// What the offline library shows; images are file names next to the video.
struct StoredMeta: Codable, Equatable {
  var title: String
  var episodeTitle: String? = nil
  var seriesId: Int64? = nil
  var seasonNumber: Int? = nil
  var episodeNumber: Int? = nil
  var year: Int? = nil
  var overview: String? = nil
  var poster: String? = nil
  var backdrop: String? = nil
  var still: String? = nil
  var durationMs: Int64? = nil
}

struct StoredItem: Codable, Equatable {
  var key: String
  var kind: String
  var id: Int64
  var complete: Bool
  var variantId: Int64?
  var profile: String
  var audioLanguage: String?
  /// Final size (complete) or expected size (in progress).
  var sizeBytes: Int64
  var subtitles: [StoredSubtitle]
  var meta: StoredMeta
  var positionMs: Int64
  var played: Bool
  /// Unix seconds of the last local or server position update.
  var positionAt: Int64
  var downloadedAt: Int64?
}

private struct Manifest: Codable {
  var items: [StoredItem]
}

/// What's downloaded, in `<folder>/batvision.json`, one sub-folder per item (as on
/// Android and desktop).
final class Library {
  let folder: URL
  private var items: [StoredItem] = []
  private let lock = NSLock()
  private let files = FileManager.default

  init(folder: URL) {
    self.folder = folder
    load()
  }

  private func load() {
    lock.lock()
    defer { lock.unlock() }
    let url = folder.appendingPathComponent(manifestFile)
    items = (try? Data(contentsOf: url)).flatMap { try? JSONDecoder().decode(Manifest.self, from: $0) }?.items ?? []
    // Forget items whose files are gone. A download in progress lives in iOS's own
    // folder until it finishes: its item folder is enough.
    items.removeAll { item in
      let path = item.complete ? dirUnlocked(item.key).appendingPathComponent(videoFile) : dirUnlocked(item.key)
      return !files.fileExists(atPath: path.path)
    }
  }

  private func save() {
    try? files.createDirectory(at: folder, withIntermediateDirectories: true)
    guard let data = try? JSONEncoder().encode(Manifest(items: items)) else { return }
    try? data.write(to: folder.appendingPathComponent(manifestFile), options: .atomic)
  }

  func all() -> [StoredItem] {
    lock.lock()
    defer { lock.unlock() }
    return items
  }

  func get(_ key: String) -> StoredItem? {
    lock.lock()
    defer { lock.unlock() }
    return items.first { $0.key == key }
  }

  func put(_ item: StoredItem) {
    lock.lock()
    defer { lock.unlock() }
    if let i = items.firstIndex(where: { $0.key == item.key }) { items[i] = item } else { items.append(item) }
    save()
  }

  static func validKey(_ key: String) -> Bool {
    key.range(of: #"^(movie|episode)-\d+$"#, options: .regularExpression) != nil
  }

  private func dirUnlocked(_ key: String) -> URL {
    precondition(Library.validKey(key), "bad item key: \(key)")
    return folder.appendingPathComponent(key, isDirectory: true)
  }

  /// Folder of an item; keys come from the server, so they're checked.
  func dir(_ key: String) -> URL { dirUnlocked(key) }

  @discardableResult
  func ensureDir(_ key: String) -> URL {
    let url = dir(key)
    try? files.createDirectory(at: url, withIntermediateDirectories: true)
    return url
  }

  func remove(_ key: String) {
    lock.lock()
    defer { lock.unlock() }
    items.removeAll { $0.key == key }
    try? files.removeItem(at: dirUnlocked(key))
    save()
  }

  /// Bytes on disk for each item, as the planner wants them, in the library's order.
  /// A download in progress counts for nothing until iOS hands the file over.
  func records() -> [LocalRecord] {
    all().map { item in
      LocalRecord(
        key: item.key, complete: item.complete, variantId: item.variantId, profile: item.profile,
        audioLanguage: item.audioLanguage, sizeBytes: item.complete ? item.sizeBytes : 0
      )
    }
  }
}
