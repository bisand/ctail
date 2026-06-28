import AppKit

/// "Unlock ctail Pro" paywall — a small, native sheet-style window listing the
/// Pro features with Buy and Restore actions. Driven entirely by StoreManager.
final class PaywallWindowController: NSWindowController {
    private let store = StoreManager.shared
    private let feature: Pro.Feature?
    private let onUnlocked: () -> Void

    private let headline = NSTextField(labelWithString: "")
    private let buyButton = NSButton(title: "Unlock ctail Pro", target: nil, action: nil)
    private let restoreButton = NSButton(title: "Restore Purchases", target: nil, action: nil)
    private let statusLabel = NSTextField(labelWithString: "")

    init(feature: Pro.Feature?, onUnlocked: @escaping () -> Void) {
        self.feature = feature
        self.onUnlocked = onUnlocked
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 420, height: 400),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = "ctail Pro"
        super.init(window: window)
        window.contentView = build()
        window.center()
        Task { await store.refresh(); await MainActor.run { self.refreshPrice() } }
    }
    required init?(coder: NSCoder) { fatalError() }

    // MARK: - Layout

    private func build() -> NSView {
        let icon = NSImageView()
        icon.image = NSImage(systemSymbolName: "sparkles", accessibilityDescription: "Pro")
        icon.symbolConfiguration = .init(pointSize: 40, weight: .semibold)
        icon.contentTintColor = .controlAccentColor

        headline.stringValue = feature?.title ?? "Unlock ctail Pro"
        headline.font = .systemFont(ofSize: 18, weight: .bold)
        headline.alignment = .center
        headline.maximumNumberOfLines = 2

        let sub = NSTextField(labelWithString: "ctail is free to use. Pro unlocks the power features with a one-time purchase — no subscription.")
        sub.font = .systemFont(ofSize: 12)
        sub.textColor = .secondaryLabelColor
        sub.alignment = .center
        sub.maximumNumberOfLines = 3
        sub.lineBreakMode = .byWordWrapping
        sub.preferredMaxLayoutWidth = 360

        let features = NSStackView(views: [
            featureRow("AI log assistant — explain errors & generate rules"),
            featureRow("Open unlimited files at once"),
            featureRow("All color themes"),
            featureRow("Yours forever — one-time purchase"),
        ])
        features.orientation = .vertical
        features.alignment = .leading
        features.spacing = 8

        buyButton.target = self
        buyButton.action = #selector(buy)
        buyButton.bezelStyle = .rounded
        buyButton.keyEquivalent = "\r"
        buyButton.controlSize = .large

        restoreButton.target = self
        restoreButton.action = #selector(restore)
        restoreButton.bezelStyle = .rounded
        restoreButton.isBordered = false
        restoreButton.contentTintColor = .controlAccentColor

        statusLabel.font = .systemFont(ofSize: 11)
        statusLabel.textColor = .secondaryLabelColor
        statusLabel.alignment = .center
        statusLabel.maximumNumberOfLines = 2

        var views: [NSView] = [icon, headline, sub, features, buyButton, restoreButton, statusLabel]
        #if DEBUG
        let devButton = NSButton(title: "Dev: unlock without purchase",
                                 target: self, action: #selector(devUnlock))
        devButton.bezelStyle = .rounded
        devButton.contentTintColor = .systemOrange
        views.append(devButton)
        #endif

        let stack = NSStackView(views: views)
        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 14
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.edgeInsets = NSEdgeInsets(top: 24, left: 28, bottom: 20, right: 28)
        stack.setCustomSpacing(18, after: features)

        let root = NSView()
        root.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: root.topAnchor),
            stack.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            stack.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            buyButton.widthAnchor.constraint(greaterThanOrEqualToConstant: 240),
        ])
        return root
    }

    private func featureRow(_ text: String) -> NSView {
        let check = NSImageView()
        check.image = NSImage(systemSymbolName: "checkmark.circle.fill", accessibilityDescription: nil)
        check.contentTintColor = .systemGreen
        let label = NSTextField(labelWithString: text)
        label.font = .systemFont(ofSize: 12)
        let row = NSStackView(views: [check, label])
        row.orientation = .horizontal
        row.spacing = 8
        return row
    }

    private func refreshPrice() {
        if let price = store.displayPrice {
            buyButton.title = "Unlock ctail Pro — \(price)"
            buyButton.isEnabled = true
        } else {
            // Product not loaded (e.g. no StoreKit config in a dev build or
            // offline). Keep Restore available; Buy stays generic + disabled.
            buyButton.title = "Unlock ctail Pro"
            buyButton.isEnabled = false
            if statusLabel.stringValue.isEmpty {
                statusLabel.stringValue = "Store unavailable right now. If you already bought Pro, tap Restore."
            }
        }
    }

    // MARK: - Actions

    #if DEBUG
    /// Dev-only: unlock Pro without going through StoreKit (never in release).
    @objc private func devUnlock() {
        Pro.devUnlocked = true
        succeed()
    }
    #endif

    @objc private func buy() {
        setBusy(true)
        Task {
            let outcome = await store.purchase()
            await MainActor.run { self.handle(outcome) }
        }
    }

    @objc private func restore() {
        setBusy(true)
        statusLabel.stringValue = "Restoring…"
        Task {
            await store.restore()
            await MainActor.run {
                self.setBusy(false)
                if self.store.isPro { self.succeed() }
                else { self.statusLabel.stringValue = "No previous purchase found for this Apple ID." }
            }
        }
    }

    private func handle(_ outcome: StoreManager.PurchaseOutcome) {
        setBusy(false)
        switch outcome {
        case .success: succeed()
        case .cancelled: statusLabel.stringValue = ""
        case .pending: statusLabel.stringValue = "Purchase pending approval."
        case .unavailable: statusLabel.stringValue = "Store unavailable. Try Restore or check your connection."
        case .failed(let msg): statusLabel.stringValue = msg
        }
    }

    private func succeed() {
        statusLabel.textColor = .systemGreen
        statusLabel.stringValue = "Thanks! ctail Pro is unlocked."
        onUnlocked()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in self?.close() }
    }

    private func setBusy(_ busy: Bool) {
        buyButton.isEnabled = !busy && store.displayPrice != nil
        restoreButton.isEnabled = !busy
    }
}
