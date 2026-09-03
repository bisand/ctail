import Foundation
import CtailCore

// The data model (Rule, Profile, TabState, WindowState, AppSettings, Theme,
// ThemeColors) lives in the Rust engine (core/src/models.rs) and reaches Swift
// as generated records with the same field names and defaults. JSON keys are
// unchanged from the Go app, so existing config files round-trip. This file
// only adds the Swift-side conveniences the UI relies on.

// Module-wide aliases so UI files see the records without importing CtailCore.
typealias Rule = CtailCore.Rule
typealias Profile = CtailCore.Profile
typealias TabState = CtailCore.TabState
typealias WindowState = CtailCore.WindowState
typealias AppSettings = CtailCore.AppSettings
typealias Theme = CtailCore.Theme
typealias ThemeColors = CtailCore.ThemeColors
typealias FileSearchQuery = CtailCore.FileSearchQuery
typealias FileSearchStatus = CtailCore.FileSearchStatus

extension AppSettings {
    /// All defaults (matches `AppSettings::default()` in the engine).
    init() { self = defaultSettings() }

    /// Lenient parse: unknown keys ignored, missing keys defaulted, garbage -> defaults.
    static func fromJSON(_ json: String) -> AppSettings { settingsFromJson(json: json) }
}

enum Defaults {
    /// The built-in "Common Logs" profile.
    static func commonLogsProfile() -> Profile { defaultProfile() }
}
