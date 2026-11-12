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
    let work = Task {
      await Engine.shared.runSync()
      task.setTaskCompleted(success: true)
    }
    task.expirationHandler = { work.cancel() }
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
