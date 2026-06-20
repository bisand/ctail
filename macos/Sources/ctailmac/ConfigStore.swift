import Foundation

/// Loads/saves settings and profiles, mirroring internal/config/config.go.
/// Storage lives in ~/Library/Application Support/ctail/ with:
///   settings.json, profiles/<name>.json, themes/<name>.json
/// Writes are atomic (temp file + rename) so a crash mid-write can't corrupt a
/// config. Parse failures fall back to defaults rather than throwing.
final class ConfigStore {
    let dir: URL
    private let profilesDir: URL
    let themesDir: URL
    private let enc: JSONEncoder = {
        let e = JSONEncoder(); e.outputFormatting = [.prettyPrinted, .sortedKeys]; return e
    }()

    /// `root` override is for tests; production uses Application Support.
    init(root: URL? = nil) {
        let base = root ?? FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ctail", isDirectory: true)
        dir = base
        profilesDir = base.appendingPathComponent("profiles", isDirectory: true)
        themesDir = base.appendingPathComponent("themes", isDirectory: true)
        try? FileManager.default.createDirectory(at: profilesDir, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: themesDir, withIntermediateDirectories: true)
    }

    // MARK: - Settings

    private var settingsURL: URL { dir.appendingPathComponent("settings.json") }

    func loadSettings() -> AppSettings {
        guard let data = try? Data(contentsOf: settingsURL),
              let s = try? JSONDecoder().decode(AppSettings.self, from: data)
        else { return AppSettings() }
        return s
    }

    @discardableResult
    func saveSettings(_ s: AppSettings) -> Bool {
        guard let data = try? enc.encode(s) else { return false }
        return atomicWrite(data, to: settingsURL)
    }

    // MARK: - Recent files (stored in settings, capped at 15, MRU order)

    func recentFiles() -> [String] { loadSettings().recentFiles }

    func addRecentFile(_ path: String, max: Int = 15) {
        var s = loadSettings()
        s.recentFiles.removeAll { $0 == path }
        s.recentFiles.insert(path, at: 0)
        if s.recentFiles.count > max { s.recentFiles = Array(s.recentFiles.prefix(max)) }
        saveSettings(s)
    }

    func clearRecentFiles() {
        var s = loadSettings(); s.recentFiles = []; saveSettings(s)
    }

    // MARK: - Profiles

    func listProfiles() -> [String] {
        let urls = (try? FileManager.default.contentsOfDirectory(at: profilesDir,
                    includingPropertiesForKeys: nil)) ?? []
        return urls.filter { $0.pathExtension == "json" }
                   .compactMap { try? JSONDecoder().decode(Profile.self, from: Data(contentsOf: $0)).name }
                   .sorted()
    }

    func loadProfile(_ name: String) -> Profile? {
        let url = profilesDir.appendingPathComponent(Self.sanitize(name) + ".json")
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(Profile.self, from: data)
    }

    @discardableResult
    func saveProfile(_ p: Profile) -> Bool {
        guard let data = try? enc.encode(p) else { return false }
        return atomicWrite(data, to: profilesDir.appendingPathComponent(Self.sanitize(p.name) + ".json"))
    }

    func deleteProfile(_ name: String) {
        try? FileManager.default.removeItem(at: profilesDir.appendingPathComponent(Self.sanitize(name) + ".json"))
    }

    @discardableResult
    func renameProfile(_ old: String, to new: String) -> Bool {
        guard var p = loadProfile(old) else { return false }
        deleteProfile(old)
        p.name = new
        return saveProfile(p)
    }

    /// Writes the built-in profile if no profiles exist yet.
    func ensureDefaultProfile() {
        if listProfiles().isEmpty { saveProfile(Defaults.commonLogsProfile()) }
    }

    // MARK: - Helpers

    private func atomicWrite(_ data: Data, to url: URL) -> Bool {
        let tmp = url.appendingPathExtension("tmp")
        do {
            try data.write(to: tmp, options: .atomic)
            _ = try FileManager.default.replaceItemAt(url, withItemAt: tmp)
            return true
        } catch {
            try? data.write(to: url, options: .atomic)   // best-effort fallback
            try? FileManager.default.removeItem(at: tmp)
            return false
        }
    }

    /// Mirrors sanitizeFilename in config.go — strips path-hostile characters.
    static func sanitize(_ name: String) -> String {
        let bad = CharacterSet(charactersIn: "/\\:*?\"<>|")
        let cleaned = name.components(separatedBy: bad).joined(separator: "_")
        return cleaned.isEmpty ? "profile" : cleaned
    }
}
