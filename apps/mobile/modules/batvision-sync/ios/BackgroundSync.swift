import BackgroundTasks
import ExpoModulesCore
import UIKit

/// When iOS lets the app sync in the background (D86): about every 30 minutes at best,
/// as iOS decides. Transfers themselves don't need it: they run in the background
/// session and relaunch the app when they end.
enum BackgroundSync {
  static let identifier = "fr.batvision.ios.sync"

  static func register() {
    BGTaskScheduler.shared.register(forTaskWithIdentifier: identifier, using: nil) { task in
      guard let task = task as? BGAppRefreshTask else { return task.setTaskCompleted(success: false) }
      handle(task)
    }
  }

  static func schedule() {
    guard Engine.shared.signedIn else { return }
    let request = BGAppRefreshTaskRequest(identifier: identifier)
    request.earliestBeginDate = Date(timeIntervalSinceNow: 30 * 60)
    try? BGTaskScheduler.shared.submit(request)
  }

  static func cancel() {
    BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: identifier)
  }

  private static func handle(_ task: BGAppRefreshTask) {
    schedule()
    // Out of time: tell iOS at once. The sync can't be interrupted, and an app that is
    // late to answer gets killed or fewer refreshes. Whichever comes first answers, once.
    let answer = Answer(task)
    task.expirationHandler = { answer.complete(success: false) }
    Task {
      await Engine.shared.runSync()
      answer.complete(success: true)
    }
  }
}

/// Tells iOS a background task is over, once only: a second call is a crash.
private final class Answer: @unchecked Sendable {
  private let task: BGTask
  private let lock = NSLock()
  private var given = false

  init(_ task: BGTask) { self.task = task }

  func complete(success: Bool) {
    lock.lock()
    let first = !given
    given = true
    lock.unlock()
    if first { task.setTaskCompleted(success: success) }
  }
}

/// Hooks the engine into the app's lifecycle.
public class BatvisionSyncAppDelegate: ExpoAppDelegateSubscriber {
  public func application(
    _ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
  ) -> Bool {
    // Must happen before launch ends; the engine reattaches to running transfers.
    BackgroundSync.register()
    _ = Engine.shared
    return true
  }

  /// iOS relaunched the app because a background transfer ended.
  public func application(
    _ application: UIApplication, handleEventsForBackgroundURLSession identifier: String,
    completionHandler: @escaping () -> Void
  ) {
    guard identifier == backgroundSessionId else { return completionHandler() }
    Engine.shared.setBackgroundCompletion(completionHandler)
  }

  public func applicationDidEnterBackground(_ application: UIApplication) {
    BackgroundSync.schedule()
  }

  public func applicationDidBecomeActive(_ application: UIApplication) {
    Engine.shared.requestSync()
  }
}
