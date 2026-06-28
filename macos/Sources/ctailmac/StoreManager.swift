import StoreKit

/// One-time "ctail Pro" unlock via StoreKit 2 (a non-consumable in-app purchase).
///
/// Strategy: the app is free and fully usable; Pro unlocks the premium features
/// (currently the AI assistant). A non-consumable is a permanent, restorable
/// purchase — no subscription — which fits a developer utility and keeps reviews
/// friendly.
///
/// Concurrency: this is a plain class used from the main thread. Async StoreKit
/// work runs in Tasks; every mutation of `isPro`/`product` hops back to the main
/// actor, so AppKit can read `isPro` synchronously without a data race.
/// Central Pro-gating policy. One place to decide what's free vs. Pro so the
/// gates stay consistent across the app.
enum Pro {
    /// The premium features, used to tailor the paywall headline.
    enum Feature {
        case ai, tabs, themes
        var title: String {
            switch self {
            case .ai:     return "Unlock the AI assistant"
            case .tabs:   return "Open more files at once"
            case .themes: return "Unlock all themes"
            }
        }
    }

    static var isUnlocked: Bool {
        #if DEBUG
        if devUnlocked { return true }
        #endif
        return StoreManager.shared.isPro
    }

    #if DEBUG
    /// Dev-only Pro override (never compiled into a release build). Enable via the
    /// env var `CTAIL_PRO=1`, or persistently with the "Unlock Pro (dev)" menu item
    /// (which toggles this UserDefaults flag).
    static let devUnlockKey = "ctail.dev.unlockPro"
    static var devUnlocked: Bool {
        get { ProcessInfo.processInfo.environment["CTAIL_PRO"] == "1"
            || UserDefaults.standard.bool(forKey: devUnlockKey) }
        set { UserDefaults.standard.set(newValue, forKey: devUnlockKey) }
    }
    #endif

    /// Free users can keep this many files open at once; Pro is unlimited.
    static let freeTabLimit = 2

    /// Themes available without Pro (the default look). Everything else is Pro.
    static let freeThemes: Set<String> = ["catppuccin"]

    static func themeAllowed(_ name: String) -> Bool { isUnlocked || freeThemes.contains(name) }

    /// The theme a non-Pro user falls back to if a Pro theme was somehow set.
    static let fallbackTheme = "catppuccin"
}

final class StoreManager {
    static let shared = StoreManager()
    static let proProductID = "net.biseth.ctail.pro"

    private(set) var isPro = false        // main-thread only
    private(set) var product: Product?    // main-thread only
    /// Called on the main thread whenever entitlement changes.
    var onChange: ((Bool) -> Void)?

    private init() {}

    /// Begin listening for transactions and load current entitlement + product.
    func start() {
        Task { await listenForTransactions() }
        Task { await refresh() }
    }

    /// Localised price (e.g. "$9.99"), or nil until the product loads.
    var displayPrice: String? { product?.displayPrice }

    func refresh() async {
        let loaded = try? await Product.products(for: [Self.proProductID]).first
        let owned = await currentlyOwned()
        await MainActor.run {
            if let loaded { self.product = loaded }
            self.setPro(owned)
        }
    }

    enum PurchaseOutcome { case success, cancelled, pending, unavailable, failed(String) }

    func purchase() async -> PurchaseOutcome {
        let prod = await MainActor.run { self.product }
        guard let prod else { return .unavailable }
        do {
            switch try await prod.purchase() {
            case .success(let verification):
                guard case .verified(let transaction) = verification else {
                    return .failed("Purchase could not be verified.")
                }
                await transaction.finish()
                await updateEntitlement()
                return await MainActor.run { self.isPro } ? .success : .failed("Could not confirm the purchase.")
            case .userCancelled: return .cancelled
            case .pending: return .pending
            @unknown default: return .failed("Unknown purchase result.")
            }
        } catch {
            return .failed(error.localizedDescription)
        }
    }

    /// Restore Purchases: force a sync with the App Store, then re-check.
    func restore() async {
        try? await AppStore.sync()
        await updateEntitlement()
    }

    // MARK: - Internals

    private func updateEntitlement() async {
        let owned = await currentlyOwned()
        await MainActor.run { self.setPro(owned) }
    }

    /// Iterates the user's current entitlements for a valid, unrevoked Pro unlock.
    private func currentlyOwned() async -> Bool {
        for await result in Transaction.currentEntitlements {
            if case .verified(let t) = result, t.productID == Self.proProductID, t.revocationDate == nil {
                return true
            }
        }
        return false
    }

    /// Main-thread mutation + change notification.
    private func setPro(_ value: Bool) {
        guard value != isPro else { return }
        isPro = value
        onChange?(value)
    }

    /// Long-lived listener so purchases made elsewhere (or family sharing /
    /// refunds) update entitlement live.
    private func listenForTransactions() async {
        for await result in Transaction.updates {
            if case .verified(let t) = result {
                await t.finish()
                await updateEntitlement()
            }
        }
    }
}
