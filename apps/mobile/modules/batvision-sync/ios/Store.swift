import Foundation
import Security

/// An offline progress record, as `POST /api/progress/batch` takes it.
struct PendingProgress: Codable, Equatable {
  let kind: String
  let id: Int64
  let positionMs: Int64
  let played: Bool
  let at: Int64

  var json: [String: Any] { ["kind": kind, "id": id, "position_ms": positionMs, "played": played, "at": at] }
}

/// What the engine keeps between launches: the server and its device token (in the
/// iOS keychain, readable in the background once the device was unlocked), pause,
/// offline progress and the monthly mobile data count.
final class Store {
  private let defaults = UserDefaults(suiteName: "batvision.sync") ?? .standard
  private let lock = NSLock()
  private let service = "fr.batvision.sync"

  init() {
    // The keychain outlives the app: a token from a previous install is stale.
    if !defaults.bool(forKey: "installed") {
      keychainWrite("token", nil)
      defaults.set(true, forKey: "installed")
    }
  }

  var server: String? {
    get { defaults.string(forKey: "server") }
    set { defaults.set(newValue, forKey: "server") }
  }

  var token: String? {
    get { keychainRead("token") }
    set { keychainWrite("token", newValue) }
  }

  /// This device's id on the server (from the sign-in).
  var deviceId: Int64? {
    get { defaults.object(forKey: "deviceId") as? Int64 ?? (defaults.object(forKey: "deviceId") as? NSNumber)?.int64Value }
    set { defaults.set(newValue, forKey: "deviceId") }
  }

  var paused: Bool {
    get { defaults.bool(forKey: "paused") }
    set { defaults.set(newValue, forKey: "paused") }
  }

  /// Hardware AV1 decoding, detected once.
  var av1: Bool {
    get { defaults.bool(forKey: "av1") }
    set { defaults.set(newValue, forKey: "av1") }
  }

  /// The last plan's settings, so rules apply even before the next plan.
  var settings: [String: Any] {
    get {
      guard let data = defaults.data(forKey: "settings") else { return [:] }
      return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
    }
    set { defaults.set(try? JSONSerialization.data(withJSONObject: newValue), forKey: "settings") }
  }

  func mobileBytes(month: String) -> Int64 {
    lock.lock()
    defer { lock.unlock() }
    return defaults.string(forKey: "mobileMonth") == month ? Int64(defaults.integer(forKey: "mobileBytes")) : 0
  }

  func addMobileBytes(month: String, bytes: Int64) {
    let total = mobileBytes(month: month) + bytes
    lock.lock()
    defer { lock.unlock() }
    defaults.set(month, forKey: "mobileMonth")
    defaults.set(Int(total), forKey: "mobileBytes")
  }

  func pendingProgress() -> [PendingProgress] {
    lock.lock()
    defer { lock.unlock() }
    return pendingUnlocked()
  }

  private func pendingUnlocked() -> [PendingProgress] {
    guard let data = defaults.data(forKey: "pendingProgress") else { return [] }
    return (try? JSONDecoder().decode([PendingProgress].self, from: data)) ?? []
  }

  private func savePending(_ list: [PendingProgress]) {
    defaults.set(try? JSONEncoder().encode(list), forKey: "pendingProgress")
  }

  /// Keeps only the latest record per item.
  func addProgress(_ p: PendingProgress) {
    lock.lock()
    defer { lock.unlock() }
    savePending(pendingUnlocked().filter { !($0.kind == p.kind && $0.id == p.id) } + [p])
  }

  func removeProgress(_ sent: [PendingProgress]) {
    lock.lock()
    defer { lock.unlock() }
    savePending(pendingUnlocked().filter { !sent.contains($0) })
  }

  func clearSession() {
    token = nil
    for key in ["deviceId", "settings", "pendingProgress"] { defaults.removeObject(forKey: key) }
  }

  // ---- Keychain ------------------------------------------------------------------------

  private func query(_ account: String) -> [String: Any] {
    [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: account]
  }

  private func keychainRead(_ account: String) -> String? {
    var q = query(account)
    q[kSecReturnData as String] = true
    q[kSecMatchLimit as String] = kSecMatchLimitOne
    var result: AnyObject?
    guard SecItemCopyMatching(q as CFDictionary, &result) == errSecSuccess, let data = result as? Data else { return nil }
    return String(data: data, encoding: .utf8)
  }

  private func keychainWrite(_ account: String, _ value: String?) {
    SecItemDelete(query(account) as CFDictionary)
    guard let value else { return }
    var q = query(account)
    q[kSecValueData as String] = Data(value.utf8)
    // Background downloads run while the device is locked.
    q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    SecItemAdd(q as CFDictionary, nil)
  }
}
