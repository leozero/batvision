import Foundation
import Network
import UIKit
import os

private let log = Logger(subsystem: "fr.batvision.ios", category: "sync")
private let imageSize = ["poster": "w342", "backdrop": "w1280", "still": "w500"]
let backgroundSessionId = "fr.batvision.ios.downloads"

/// The download in progress, for the state.
struct Current {
  let key: String
  let title: String
  var received: Int64
  var total: Int64?
}

private func now() -> Int64 { Int64(Date().timeIntervalSince1970) }

private func nn(_ value: Any?) -> Any { value ?? NSNull() }

/// Keeps the download folder in line with the server's expected set, like the Kotlin
/// engine (Engine.kt) and the desktop one, with the same network rules (D68, D69).
///
/// iOS differences (D86, D89): transfers run in a background `URLSession`, so they go on
/// with the app suspended or closed and iOS relaunches the app when one ends; planning
/// runs when the app opens, when the network changes, and when iOS grants background
/// time (BackgroundSync). Over cellular, iOS itself holds a transfer the rules don't allow.
final class Engine: NSObject {
  static let shared = Engine()

  let store = Store()
  let library: Library

  private let lock = NSRecursiveLock()
  private var plan: [PlanEntry] = []
  private var decision: Decision?
  private var quota: Int64?
  private var online = true
  private var syncing = false
  private var lastSyncAt: Int64?
  private var lastError: String?
  private var waits: [String: Wait] = [:]
  private var failed: [String: String] = [:]
  private var current: Current?
  private var activeTask: URLSessionDownloadTask?
  /// Items whose downloaded file is being completed (subtitles, pictures).
  private var finishing = Set<String>()
  /// Items dropped by hand: a download of one stops at once (D74).
  private var cancelled = Set<String>()
  /// Items whose URL was refused and the plan fetched again for: once each, until a success.
  private var refreshed = Set<String>()
  /// Items whose file is here but couldn't be completed, the server being unreachable:
  /// left alone until the next sync.
  private var stalled = Set<String>()
  private var retryPending = false
  private var retries = 0
  private var listeners: [UUID: ([String: Any]) -> Void] = [:]
  private var lastEmit = Date.distantPast
  private var lastCheck = Date.distantPast
  private var syncTask: Task<Void, Never>?
  private var syncAgain = false
  private var busyChecks = 0
  private var flushScheduled = false
  private var backgroundCompletion: (() -> Void)?
  private var backgroundEventsDone = false
  private let monitor = NWPathMonitor()
  private var path: NWPath?
  private var charging = false

  private lazy var downloads: URLSession = {
    let config = URLSessionConfiguration.background(withIdentifier: backgroundSessionId)
    config.sessionSendsLaunchEvents = true
    config.isDiscretionary = false
    let queue = OperationQueue()
    queue.maxConcurrentOperationCount = 1
    return URLSession(configuration: config, delegate: self, delegateQueue: queue)
  }()

  private override init() {
    let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    var folder = support.appendingPathComponent("downloads", isDirectory: true)
    try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    // Downloads come back from the server: keep them out of iCloud backups.
    var values = URLResourceValues()
    values.isExcludedFromBackup = true
    try? folder.setResourceValues(values)
    library = Library(folder: folder)
    super.init()
    monitor.pathUpdateHandler = { [weak self] path in self?.pathChanged(path) }
    monitor.start(queue: DispatchQueue(label: "fr.batvision.sync.path"))
    DispatchQueue.main.async { [weak self] in
      UIDevice.current.isBatteryMonitoringEnabled = true
      self?.updateBattery()
      NotificationCenter.default.addObserver(
        forName: UIDevice.batteryStateDidChangeNotification, object: nil, queue: .main
      ) { [weak self] _ in self?.updateBattery() }
    }
    // Reattach to a transfer iOS kept going while the app wasn't running.
    downloads.getAllTasks { [weak self] tasks in self?.adopt(tasks) }
  }

  private func locked<T>(_ body: () throws -> T) rethrows -> T {
    lock.lock()
    defer { lock.unlock() }
    return try body()
  }

  // ---- Session --------------------------------------------------------------------------

  var signedIn: Bool { store.server != nil && store.token != nil }

  private func api() throws -> Api {
    guard let server = store.server, let token = store.token else { throw ApiError.http(401, "signed out") }
    return Api(server: server, token: token)
  }

  /// Signed out (or the token was revoked): stop and forget the plan; files stay.
  func reset() {
    let task = locked { () -> URLSessionDownloadTask? in
      plan = []
      decision = nil
      failed = [:]
      waits = [:]
      refreshed = []
      stalled = []
      retries = 0
      return activeTask
    }
    task?.cancel()
    store.clearSession()
    emit()
  }

  // ---- Conditions -------------------------------------------------------------------------

  private func pathChanged(_ newPath: NWPath) {
    let retry = locked { () -> Bool in
      path = newPath
      return !waits.isEmpty || !online
    }
    if retry && newPath.status == .satisfied { requestSync() }
  }

  private func updateBattery() {
    let state = UIDevice.current.batteryState
    let retry = locked { () -> Bool in
      let was = charging
      charging = state == .charging || state == .full
      return charging && !was && waits.values.contains(.charging)
    }
    if retry { requestSync() }
  }

  func conditions() -> Conditions {
    locked {
      let p = path ?? monitor.currentPath
      return Conditions(connected: p.status == .satisfied, metered: p.isExpensive, charging: charging)
    }
  }

  private func rules() -> Rules {
    let s = store.settings
    return Rules(
      mobileData: MobileData.parse(s["mobile_data"] as? String),
      capBytes: JSONValue.int64(s, "mobile_cap_bytes"),
      chargingOnly: JSONValue.bool(s, "charging_only")
    )
  }

  private func month() -> String { NetworkRules.month(Date()) }

  // ---- Sync -------------------------------------------------------------------------------

  /// A sync soon: one at a time; asked during a sync, another follows it.
  func requestSync() {
    locked {
      guard signedIn else { return }
      if syncTask != nil {
        syncAgain = true
        return
      }
      syncTask = Task.detached { [weak self] in
        guard let self else { return }
        repeat {
          self.locked { self.syncAgain = false }
          await self.sync()
        } while self.locked({ self.syncAgain })
        self.locked { self.syncTask = nil }
      }
    }
  }

  /// A sync, waited for (background refresh).
  func runSync() async {
    requestSync()
    await locked({ syncTask })?.value
  }

  /// One full run: progress, plan, deletions, requests, then the next download.
  private func sync() async {
    guard signedIn, let api = try? api() else { return }
    locked {
      syncing = true
      stalled = []
    }
    emit()
    do {
      try await flushProgress(api)
      let fetched = try await api.plan(av1: store.av1)
      store.settings = fetched.settings
      locked {
        online = true
        quota = JSONValue.int64(fetched.settings, "quota_bytes")
        plan = fetched.items
        // Each sync gives failed downloads another try: a dropped connection doesn't
        // last, and nothing else would retry them before a restart. `refreshed` stays,
        // or the sync a refused URL asks for would let it ask again without end.
        failed = [:]
      }
      try await apply(api)
      await refreshPositions(api)
      locked {
        lastSyncAt = Int64(Date().timeIntervalSince1970 * 1000)
        lastError = nil
      }
    } catch let ApiError.unreachable(message) {
      log.info("server unreachable: \(message, privacy: .public)")
      locked {
        online = false
        lastError = nil
      }
    } catch let ApiError.http(status, message) {
      log.warning("sync failed: \(status) \(message, privacy: .public)")
      if status == 401 { reset() } else { locked { lastError = "HTTP \(status): \(message)" } }
    } catch {
      locked { lastError = error.localizedDescription }
    }
    locked { syncing = false }
    emit()
    startNextDownload()
    scheduleBusyCheck()
  }

  private func apply(_ api: Api) async throws {
    var decision = locked { Planner.decide(plan: plan, local: library.records(), quotaBytes: quota) }
    locked { self.decision = decision }
    for key in decision.remove { removeLocal(key) }
    for action in decision.actions {
      do {
        switch action {
        case let .request(_, kind, id): try await api.request(kind: kind, id: id)
        case let .confirm(_, variantId): try await api.confirm(variantId: variantId)
        case .download: break
        }
      } catch let ApiError.http(status, message) {
        // 409: no longer expected, or the file can't be read. The next plan says so.
        log.warning("\(action.key, privacy: .public): \(status) \(message, privacy: .public)")
      }
    }
    // Requests change statuses: show them now rather than at the next sync.
    guard decision.actions.contains(where: { if case .request = $0 { return true } else { return false } }) else { return }
    let fresh = try await api.plan(av1: store.av1)
    decision = locked {
      plan = fresh.items
      return Planner.decide(plan: fresh.items, local: library.records(), quotaBytes: quota)
    }
    locked { self.decision = decision }
  }

  /// Deletes an item, stopping its transfer first.
  private func removeLocal(_ key: String) {
    let task = locked { activeTask?.taskDescription == key ? activeTask : nil }
    if let task {
      locked { cancelled.insert(key) }
      task.cancel()
    }
    library.remove(key)
  }

  /// A transfer stopped on an error: one sync later, further apart each time the error
  /// repeats, so a server that stays down isn't asked in a loop.
  private func scheduleRetry() {
    let delay = locked { () -> TimeInterval? in
      if retryPending { return nil }
      retryPending = true
      retries += 1
      return TransferRules.retryDelay(attempt: retries - 1)
    }
    guard let delay else { return }
    DispatchQueue.global().asyncAfter(deadline: .now() + delay) { [weak self] in
      guard let self else { return }
      self.locked { self.retryPending = false }
      self.requestSync()
    }
  }

  /// Watched elsewhere since: bring the server's positions to the local copies.
  private func refreshPositions(_ api: Api) async {
    let pending = store.pendingProgress()
    for item in library.all() where item.complete {
      if pending.contains(where: { $0.kind == item.kind && $0.id == item.id }) { continue }
      guard let d = try? await api.detail(kind: item.kind, id: item.id) else { continue }
      var updated = item
      updated.positionMs = JSONValue.int64(d, "position_ms") ?? 0
      updated.played = JSONValue.bool(d, "played")
      updated.positionAt = now()
      library.put(updated)
    }
  }

  /// The server is encoding for us: look again shortly while the app is open, so the
  /// download starts as soon as the file is ready (as Android does in its worker).
  private func scheduleBusyCheck() {
    let busy = locked { plan.contains { $0.status == "queued" || $0.status == "encoding" } }
    let again = locked { () -> Bool in
      if !busy { busyChecks = 0 }
      busyChecks += busy ? 1 : 0
      return busy && busyChecks <= 30
    }
    guard again else { return }
    DispatchQueue.main.asyncAfter(deadline: .now() + 15) { [weak self] in
      if UIApplication.shared.applicationState == .active { self?.requestSync() }
    }
  }

  // ---- Downloads --------------------------------------------------------------------------

  /// Starts the first download allowed now, unless one is running.
  func startNextDownload() {
    lock.lock()
    defer { lock.unlock() }
    guard signedIn, !store.paused, activeTask == nil, let decision, let api = try? api() else { return }
    waits = [:]
    let conditions = conditions()
    let rules = rules()
    let mobileUsed = store.mobileBytes(month: month())
    for case let .download(key, entry) in decision.actions {
      if library.get(key)?.complete == true || failed[key] != nil || cancelled.contains(key) || finishing.contains(key)
        || stalled.contains(key)
      {
        continue
      }
      // Transferred while the app wasn't running, not completed yet.
      if FileManager.default.fileExists(atPath: library.dir(key).appendingPathComponent(partialFile).path) {
        finishing.insert(key)
        Task.detached { [weak self] in
          await self?.finish(key)
          self?.startNextDownload()
        }
        return
      }
      if let wait = NetworkRules.check(profile: entry.profile, rules: rules, now: conditions, mobileBytesThisMonth: mobileUsed) {
        waits[key] = wait
        continue
      }
      start(entry, api: api, mobileAllowed: NetworkRules.mobileWait(profile: entry.profile, rules: rules, mobileBytesThisMonth: mobileUsed) == nil)
      break
    }
    DispatchQueue.global().async { [weak self] in self?.emit() }
  }

  private func start(_ entry: PlanEntry, api: Api, mobileAllowed: Bool) {
    guard let url = entry.url else { return }
    let key = entry.key
    let dir = library.ensureDir(key)
    if library.get(key) == nil {
      library.put(StoredItem(
        key: key, kind: entry.kind, id: entry.id, complete: false, variantId: entry.variantId,
        profile: entry.profile, audioLanguage: entry.audioLanguage, sizeBytes: entry.sizeBytes ?? 0,
        subtitles: [], meta: metaFromPlan(entry), positionMs: 0, played: false, positionAt: 0, downloadedAt: nil
      ))
    }
    let resume = dir.appendingPathComponent(resumeFile)
    let task: URLSessionDownloadTask
    if let data = try? Data(contentsOf: resume) {
      try? FileManager.default.removeItem(at: resume)
      task = downloads.downloadTask(withResumeData: data)
    } else {
      var request = api.request(url)
      // Over cellular, iOS waits for Wi-Fi by itself when mobile data isn't allowed.
      request.allowsExpensiveNetworkAccess = mobileAllowed
      request.allowsConstrainedNetworkAccess = mobileAllowed
      task = downloads.downloadTask(with: request)
    }
    task.taskDescription = key
    activeTask = task
    current = Current(key: key, title: displayTitle(entry), received: 0, total: entry.sizeBytes)
    task.resume()
    log.info("downloading \(key, privacy: .public)")
  }

  /// After a relaunch: the transfer iOS went on with becomes the current one.
  private func adopt(_ tasks: [URLSessionTask]) {
    let running = tasks.compactMap { $0 as? URLSessionDownloadTask }.filter { $0.state == .running || $0.state == .suspended }
    locked {
      guard activeTask == nil, let task = running.first, let key = task.taskDescription, Library.validKey(key) else { return }
      activeTask = task
      let title = library.get(key)?.meta.title ?? key
      current = Current(key: key, title: title, received: task.countOfBytesReceived,
                        total: task.countOfBytesExpectedToReceive > 0 ? task.countOfBytesExpectedToReceive : nil)
    }
    for extra in running.dropFirst() { extra.cancel() }
    emit()
  }

  /// Stops the transfer, keeping what iOS needs to resume it.
  private func pauseTransfer(_ task: URLSessionDownloadTask) {
    guard let key = task.taskDescription, Library.validKey(key) else { return task.cancel() }
    let dir = library.ensureDir(key)
    task.cancel { data in
      if let data { try? data.write(to: dir.appendingPathComponent(resumeFile)) }
    }
  }

  private func entryFor(_ key: String) -> PlanEntry? { locked { plan.first { $0.key == key } } }

  /// The file is here: subtitles, pictures and positions, then the item is complete.
  private func finish(_ key: String) async {
    defer {
      locked { finishing.remove(key) }
      completeBackgroundEventsIfIdle()
    }
    guard let api = try? api() else { return }
    var entry = entryFor(key)
    if entry == nil, let fetched = try? await api.plan(av1: store.av1) {
      // Relaunched by iOS for a finished transfer: the plan is gone with the old process.
      locked { plan = fetched.items }
      entry = entryFor(key)
    }
    guard let entry, var stored = library.get(key) else { return }
    let dir = library.dir(key)
    let files = FileManager.default
    do {
      var subtitles: [StoredSubtitle] = []
      for (i, sub) in entry.subtitles.enumerated() {
        guard let data = try await api.bytes(sub.url) else { throw ApiError.http(404, "subtitle gone") }
        let file = "sub-\(i).vtt"
        try data.write(to: dir.appendingPathComponent(file))
        subtitles.append(StoredSubtitle(file: file, language: sub.language, kind: sub.kind))
      }
      let detail = try? await api.detail(kind: entry.kind, id: entry.id)
      let meta = detail != nil ? await fetchMeta(api, entry, detail!, dir) : stored.meta
      let video = dir.appendingPathComponent(videoFile)
      try? files.removeItem(at: video)
      try files.moveItem(at: dir.appendingPathComponent(partialFile), to: video)
      let size = (try? files.attributesOfItem(atPath: video.path)[.size] as? NSNumber)??.int64Value ?? stored.sizeBytes
      stored.complete = true
      stored.sizeBytes = size
      stored.subtitles = subtitles
      stored.meta = meta
      stored.positionMs = detail.flatMap { JSONValue.int64($0, "position_ms") } ?? 0
      stored.played = detail.map { JSONValue.bool($0, "played") } ?? false
      stored.positionAt = now()
      stored.downloadedAt = now()
      library.put(stored)
      try? files.removeItem(at: dir.appendingPathComponent(resumeFile))
      locked {
        failed.removeValue(forKey: key)
        refreshed.remove(key)
        retries = 0
      }
      log.info("downloaded \(key, privacy: .public)")
      if let variant = entry.variantId { try? await api.confirm(variantId: variant) }
    } catch ApiError.unreachable {
      // The file stays; completing it is tried again at the next sync, not before (the
      // caller goes on to the next download, which would come straight back here).
      locked {
        online = false
        stalled.insert(key)
      }
      scheduleRetry()
    } catch {
      locked { failed[key] = error.localizedDescription }
    }
    emit()
  }

  private func displayTitle(_ entry: PlanEntry) -> String {
    if entry.kind == "episode", let s = entry.seasonNumber, let e = entry.episodeNumber {
      return String(format: "%@ · S%02dE%02d", entry.title, s, e)
    }
    return entry.title
  }

  private func metaFromPlan(_ entry: PlanEntry) -> StoredMeta {
    StoredMeta(
      title: entry.title, episodeTitle: entry.episodeTitle, seriesId: entry.seriesId,
      seasonNumber: entry.seasonNumber, episodeNumber: entry.episodeNumber, durationMs: entry.durationMs
    )
  }

  private func fetchMeta(_ api: Api, _ entry: PlanEntry, _ d: [String: Any], _ dir: URL) async -> StoredMeta {
    func str(_ name: String) -> String? { JSONValue.string(d, name) }
    func image(_ path: String?, _ name: String) async -> String? {
      guard let path, let size = imageSize[name], let data = try? await api.bytes("/api/images/\(size)\(path)") else { return nil }
      let ext = path.range(of: ".", options: .backwards).map { String(path[$0.lowerBound...]) } ?? ""
      let file = name + ext
      try? data.write(to: dir.appendingPathComponent(file))
      return file
    }
    let episode = entry.kind == "episode"
    let runtime = JSONValue.int64(d, "runtime_minutes").map { $0 * 60_000 }
    return StoredMeta(
      title: (episode ? str("series_title") : str("title")) ?? entry.title,
      episodeTitle: episode ? str("title") : nil,
      seriesId: JSONValue.int64(d, "series_id") ?? entry.seriesId,
      seasonNumber: JSONValue.int(d, "season_number") ?? entry.seasonNumber,
      episodeNumber: JSONValue.int(d, "episode_number") ?? entry.episodeNumber,
      year: JSONValue.int(d, "year"),
      overview: str("overview"),
      poster: await image(episode ? str("series_poster_path") : str("poster_path"), "poster"),
      backdrop: await image(episode ? str("series_backdrop_path") : str("backdrop_path"), "backdrop"),
      still: episode ? await image(str("still_path"), "still") : nil,
      durationMs: entry.durationMs ?? JSONValue.int64(d, "duration_ms") ?? runtime
    )
  }

  // ---- Background events --------------------------------------------------------------------

  /// iOS relaunched the app for the transfer session: call back once the work is done.
  func setBackgroundCompletion(_ handler: @escaping () -> Void) {
    locked {
      backgroundCompletion = handler
      backgroundEventsDone = false
    }
    _ = downloads
  }

  private func completeBackgroundEventsIfIdle() {
    let handler = locked { () -> (() -> Void)? in
      guard backgroundEventsDone, finishing.isEmpty, let handler = backgroundCompletion else { return nil }
      backgroundCompletion = nil
      return handler
    }
    if let handler { DispatchQueue.main.async(execute: handler) }
  }

  // ---- Controls ---------------------------------------------------------------------------

  /// Drops an item, downloaded or not (D74). The server forgets what asks for it first, so
  /// the next plan doesn't bring it back; a transfer of it stops and the files go. Throws
  /// when the server can't be reached.
  func cancel(kind: String, id: Int64) async throws {
    try await api().cancel(kind: kind, id: id)
    let key = itemKey(kind, id)
    let task = locked { () -> URLSessionDownloadTask? in
      cancelled.insert(key)
      return activeTask?.taskDescription == key ? activeTask : nil
    }
    task?.cancel()
    // Wait for the session to let go of the transfer.
    while locked({ activeTask?.taskDescription == key }) { try await Task.sleep(nanoseconds: 100_000_000) }
    library.remove(key)
    locked {
      failed.removeValue(forKey: key)
      waits.removeValue(forKey: key)
      refreshed.remove(key)
      plan.removeAll { $0.key == key }
      cancelled.remove(key)
    }
    emit()
    requestSync()
  }

  func setPaused(_ paused: Bool) {
    store.paused = paused
    if paused, let task = locked({ activeTask }) { pauseTransfer(task) }
    emit()
    if !paused { requestSync() }
  }

  // ---- Offline progress -------------------------------------------------------------------

  func reportProgress(kind: String, id: Int64, positionMs: Int64, played: Bool) {
    let key = itemKey(kind, id)
    if var item = library.get(key) {
      item.positionMs = positionMs
      item.played = item.played || played
      item.positionAt = now()
      library.put(item)
    }
    store.addProgress(PendingProgress(kind: kind, id: id, positionMs: positionMs, played: played, at: now()))
    let schedule = locked { () -> Bool in
      if flushScheduled { return false }
      flushScheduled = true
      return true
    }
    guard schedule else { return }
    Task.detached { [weak self] in
      try? await Task.sleep(nanoseconds: 5_000_000_000)
      guard let self else { return }
      self.locked { self.flushScheduled = false }
      guard let api = try? self.api() else { return }
      do { try await self.flushProgress(api) } catch ApiError.unreachable { self.locked { self.online = false } } catch {}
      self.emit()
    }
  }

  /// Sends offline progress; kept for later when the server can't be reached.
  private func flushProgress(_ api: Api) async throws {
    let pending = store.pendingProgress()
    guard !pending.isEmpty else { return }
    let batch = Array(pending.prefix(1000))
    try await api.progress(batch)
    store.removeProgress(batch)
    locked { online = true }
  }

  // ---- Offline playback -------------------------------------------------------------------

  func localPlayback(kind: String, id: Int64) -> [String: Any]? {
    guard let item = library.get(itemKey(kind, id)), item.complete else { return nil }
    let dir = library.dir(item.key)
    return [
      "url": dir.appendingPathComponent(videoFile).absoluteString,
      "subtitles": item.subtitles.map {
        ["url": dir.appendingPathComponent($0.file).absoluteString, "language": nn($0.language), "kind": $0.kind]
      },
      "durationMs": nn(item.meta.durationMs),
      "positionMs": item.played ? 0 : item.positionMs,
      "audioLanguage": nn(item.audioLanguage),
      "nextEpisodeId": nn(nextLocalEpisode(item)),
      "meta": metaMap(item),
    ]
  }

  private func nextLocalEpisode(_ item: StoredItem) -> Int64? {
    guard item.kind == "episode", let seriesId = item.meta.seriesId else { return nil }
    // Specials (season 0) come after the regular seasons.
    func order(_ s: Int?) -> Int64 { s == 0 ? 1_000_000 : Int64(s ?? 0) }
    func rank(_ i: StoredItem) -> Int64 { order(i.meta.seasonNumber) * 10_000 + Int64(i.meta.episodeNumber ?? 0) }
    let here = rank(item)
    return library.all()
      .filter { $0.complete && $0.kind == "episode" && $0.meta.seriesId == seriesId && rank($0) > here }
      .min { rank($0) < rank($1) }?.id
  }

  // ---- State for the interface ------------------------------------------------------------

  func addListener(_ listener: @escaping ([String: Any]) -> Void) -> UUID {
    let id = UUID()
    locked { listeners[id] = listener }
    return id
  }

  func removeListener(_ id: UUID) { locked { _ = listeners.removeValue(forKey: id) } }

  private func emit(force: Bool = true) {
    let targets = locked { () -> [([String: Any]) -> Void] in
      let t = Date()
      if !force && t.timeIntervalSince(lastEmit) < 0.5 { return [] }
      lastEmit = t
      return Array(listeners.values)
    }
    guard !targets.isEmpty else { return }
    let s = state()
    for listener in targets { listener(s) }
  }

  private func metaMap(_ m: StoredMeta) -> [String: Any] {
    [
      "title": m.title, "episodeTitle": nn(m.episodeTitle), "seriesId": nn(m.seriesId),
      "seasonNumber": nn(m.seasonNumber), "episodeNumber": nn(m.episodeNumber), "year": nn(m.year),
      "overview": nn(m.overview), "poster": NSNull(), "backdrop": NSNull(), "still": NSNull(),
      "durationMs": nn(m.durationMs),
    ]
  }

  private func metaMap(_ item: StoredItem) -> [String: Any] {
    let dir = library.dir(item.key)
    func uri(_ name: String?) -> Any { nn(name.map { dir.appendingPathComponent($0).absoluteString }) }
    var m = metaMap(item.meta)
    m["poster"] = uri(item.meta.poster)
    m["backdrop"] = uri(item.meta.backdrop)
    m["still"] = uri(item.meta.still)
    return m
  }

  func state() -> [String: Any] {
    lock.lock()
    defer { lock.unlock() }
    let records = library.records()
    let sizes = Dictionary(records.map { ($0.key, $0.sizeBytes) }, uniquingKeysWith: { a, _ in a })
    var items: [[String: Any]] = []
    var seen = Set<String>()
    for entry in plan {
      seen.insert(entry.key)
      items.append(view(entry.key, entry, sizes[entry.key] ?? 0))
    }
    // Downloaded items the last plan didn't mention (offline start, plan not fetched yet).
    for stored in library.all() where !seen.contains(stored.key) {
      items.append(view(stored.key, nil, sizes[stored.key] ?? 0))
    }
    let rules = rules()
    return [
      "signedIn": signedIn,
      "online": online,
      "syncing": syncing,
      "paused": store.paused,
      "lastSyncAt": nn(lastSyncAt),
      "lastError": nn(lastError),
      "storage": "internal",
      "sdAvailable": false,
      "usedBytes": records.reduce(0) { $0 + $1.sizeBytes },
      "quotaBytes": nn(quota),
      "mobileBytesThisMonth": store.mobileBytes(month: month()),
      "mobileCapBytes": nn(rules.capBytes),
      "items": items,
    ]
  }

  private func view(_ key: String, _ entry: PlanEntry?, _ onDisk: Int64) -> [String: Any] {
    let stored = library.get(key)
    let now = current?.key == key ? current : nil
    let status: String
    if stored?.complete == true {
      status = "done"
    } else if now != nil || finishing.contains(key) {
      status = "downloading"
    } else if failed[key] != nil || entry?.status == "failed" {
      status = "failed"
    } else {
      status = "waiting"
    }
    var meta: [String: Any]
    if let stored {
      meta = metaMap(stored)
    } else {
      meta = metaMap(StoredMeta(
        title: entry?.title ?? "", episodeTitle: entry?.episodeTitle, seriesId: entry?.seriesId,
        seasonNumber: entry?.seasonNumber, episodeNumber: entry?.episodeNumber, durationMs: entry?.durationMs
      ))
    }
    // The local picture once downloaded, the server's until then (in progress too).
    if meta["poster"] is NSNull, let posterPath = entry?.posterPath, let server = store.server {
      meta["poster"] = "\(server)/api/images/w342\(posterPath)"
    }
    let total: Int64? = stored?.complete == true ? stored?.sizeBytes : (now?.total ?? entry?.sizeBytes)
    return [
      "key": key,
      "kind": entry?.kind ?? stored?.kind ?? "movie",
      "id": entry?.id ?? stored?.id ?? 0,
      "status": status,
      "serverStatus": entry?.status ?? "downloaded",
      "encodeProgress": entry?.progress ?? 0,
      "receivedBytes": now?.received ?? onDisk,
      "totalBytes": nn(total),
      "profile": entry?.profile ?? stored?.profile ?? "max",
      "audioLanguage": nn(entry?.audioLanguage ?? stored?.audioLanguage),
      "watched": entry?.watched ?? stored?.played ?? false,
      "overQuota": decision?.overQuota.contains(key) ?? false,
      "source": nn(entry?.source),
      "wait": nn(waits[key]?.rawValue),
      "error": nn(failed[key] ?? entry?.error),
      "meta": meta,
    ]
  }
}

// ---- Transfer session ------------------------------------------------------------------------

extension Engine: URLSessionDownloadDelegate {
  func urlSession(
    _ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64,
    totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64
  ) {
    guard let key = downloadTask.taskDescription else { return }
    let conditions = conditions()
    if conditions.metered { store.addMobileBytes(month: month(), bytes: bytesWritten) }
    let check = locked { () -> Bool in
      current = Current(
        key: key, title: current?.title ?? library.get(key)?.meta.title ?? key, received: totalBytesWritten,
        total: totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : current?.total
      )
      if Date().timeIntervalSince(lastCheck) < 1 { return false }
      lastCheck = Date()
      return true
    }
    if check {
      if store.paused { return pauseTransfer(downloadTask) }
      if locked({ cancelled.contains(key) }) { return downloadTask.cancel() }
      // Unplugged, or the monthly cap reached, midway: stop until the rules allow it again.
      // Bytes are arriving, so the connection is there whatever the path monitor says.
      var here = conditions
      here.connected = true
      if let entry = entryFor(key),
        let wait = NetworkRules.check(
          profile: entry.profile, rules: rules(), now: here, mobileBytesThisMonth: store.mobileBytes(month: month()))
      {
        locked { waits[key] = wait }
        return pauseTransfer(downloadTask)
      }
    }
    emit(force: false)
  }

  func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
    guard let key = downloadTask.taskDescription, Library.validKey(key) else { return }
    let status = (downloadTask.response as? HTTPURLResponse)?.statusCode ?? 0
    guard (200..<300).contains(status) else { return }
    // iOS deletes `location` when this returns: move it now.
    let part = library.ensureDir(key).appendingPathComponent(partialFile)
    try? FileManager.default.removeItem(at: part)
    do {
      try FileManager.default.moveItem(at: location, to: part)
    } catch {
      log.error("can't keep \(key, privacy: .public): \(error.localizedDescription, privacy: .public)")
    }
  }

  func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
    guard let key = task.taskDescription, Library.validKey(key) else { return }
    let status = (task.response as? HTTPURLResponse)?.statusCode ?? 0
    let wasCancelled = locked { () -> Bool in
      if activeTask?.taskIdentifier == task.taskIdentifier { activeTask = nil }
      if current?.key == key { current = nil }
      // The transfer has let go: the item may download again (a new variant after a
      // change of quality, say).
      return cancelled.remove(key) != nil
    }
    if wasCancelled {
      // It ended before the cancel reached it: what it left goes too.
      if error == nil { try? FileManager.default.removeItem(at: library.dir(key).appendingPathComponent(partialFile)) }
      emit()
      // A sync rather than the next download: the plan this was dropped under is stale,
      // and the sync that dropped it may still be deleting the files.
      requestSync()
      return
    }
    if let error = error as NSError? {
      if let data = error.userInfo[NSURLSessionDownloadTaskResumeData] as? Data {
        try? data.write(to: library.ensureDir(key).appendingPathComponent(resumeFile))
      }
      if error.code != NSURLErrorCancelled {
        log.warning("transfer \(key, privacy: .public) stopped: \(error.localizedDescription, privacy: .public)")
        // Connection lost or server gone: nothing else would start it again before the
        // app is next opened.
        if error.domain == NSURLErrorDomain { locked { online = false } }
        scheduleRetry()
      }
      emit()
      return
    }
    if !(200..<300).contains(status) {
      try? FileManager.default.removeItem(at: library.dir(key).appendingPathComponent(resumeFile))
      let end = locked { () -> TransferEnd in
        let end = TransferRules.end(status: status, refreshed: refreshed.contains(key))
        switch end {
        case .unreachable: online = false
        case .refresh: refreshed.insert(key)
        case .failed:
          failed[key] = "HTTP \(status)"
          refreshed.remove(key)
        }
        return end
      }
      switch end {
      case .unreachable:
        emit()
        scheduleRetry()
      case .refresh:
        // Signed URL expired or variant gone: the next plan has fresh URLs.
        requestSync()
      case .failed:
        emit()
        startNextDownload()
      }
      return
    }
    locked { _ = finishing.insert(key) }
    Task.detached { [weak self] in
      await self?.finish(key)
      self?.startNextDownload()
    }
  }

  func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
    locked { backgroundEventsDone = true }
    completeBackgroundEventsIfIdle()
  }
}
