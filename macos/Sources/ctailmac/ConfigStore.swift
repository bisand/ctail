import Foundation
import CtailCore

/// Settings/profile persistence, backed by the engine's config store
/// (core/src/config.rs): ~/Library/Application Support/ctail/ with
/// settings.json, profiles/<name>.json, themes/<name>.json; atomic writes;
/// lenient parsing. Keeps the URL-flavoured API the UI uses.
final class ConfigStore {
    private let core: CoreConfigStore
    let dir: URL
    let themesDir: URL

    /// `root` override is for tests; `CTAIL_CONFIG_DIR` in the environment does
    /// the same for a dev run (so trying things out doesn't touch the real
    /// session); production uses Application Support.
    init(root: URL? = nil) {
        core = CoreConfigStore(root: root?.path)
        dir = URL(fileURLWithPath: core.dir(), isDirectory: true)
        themesDir = URL(fileURLWithPath: core.themesDir(), isDirectory: true)
    }

    // MARK: - Settings

    func loadSettings() -> AppSettings { core.loadSettings() }

    @discardableResult
    func saveSettings(_ s: AppSettings) -> Bool { core.saveSettings(settings: s) }

    // MARK: - Recent files (stored in settings, capped at 15, MRU order)

    func recentFiles() -> [String] { core.recentFiles() }
    func addRecentFile(_ path: String, max: Int = 15) { core.addRecentFile(path: path, max: UInt32(max)) }
    func clearRecentFiles() { core.clearRecentFiles() }

    // MARK: - Profiles

    func listProfiles() -> [String] { core.listProfiles() }
    func loadProfile(_ name: String) -> Profile? { core.loadProfile(name: name) }

    @discardableResult
    func saveProfile(_ p: Profile) -> Bool { core.saveProfile(profile: p) }

    func deleteProfile(_ name: String) { core.deleteProfile(name: name) }

    @discardableResult
    func renameProfile(_ old: String, to new: String) -> Bool { core.renameProfile(old: old, new: new) }

    /// Writes the built-in profile if no profiles exist yet.
    func ensureDefaultProfile() { core.ensureDefaultProfile() }

    /// Profile name -> file-name-safe stem (strips path-hostile characters).
    static func sanitize(_ name: String) -> String { sanitizeProfileName(name: name) }
}
