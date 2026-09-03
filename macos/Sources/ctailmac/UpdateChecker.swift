import Foundation
import CtailCore

/// Asks the engine (`core/src/update.rs`) whether a newer release exists. The
/// request blocks on the network, so it runs on a global queue and answers on
/// the main queue. App Store builds should leave the check disabled (the
/// Store handles updates) via the disableUpdateCheck setting.
enum UpdateChecker {
    typealias Result = UpdateCheck

    static func check(current: String, completion: @escaping (Result) -> Void) {
        DispatchQueue.global(qos: .utility).async {
            let result = checkForUpdate(current: current)
            DispatchQueue.main.async { completion(result) }
        }
    }

    /// Positive when `a` is newer than `b`, negative when older, zero when
    /// the same: dotted numeric components, build suffix ignored.
    static func compareVersions(_ a: String, _ b: String) -> Int {
        Int(CtailCore.compareVersions(a: a, b: b))
    }
}
