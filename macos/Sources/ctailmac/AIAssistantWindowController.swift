import AppKit

/// AI assistant window (issue #10), mirroring AIDialog.svelte. Ask questions
/// about the active log, or auto-generate a highlighting profile from it.
/// Handles GitHub Copilot device-flow sign-in on demand.
final class AIAssistantWindowController: NSWindowController {
    private let settings: AppSettings
    private let config: ConfigStore
    private let logProvider: () -> String      // returns the active tab's log context
    private let onProfileGenerated: (String) -> Void

    private let answerView = NSTextView()
    private let questionField = NSTextField()
    private let status = NSTextField(labelWithString: "")
    private let askButton = NSButton()
    private let genButton = NSButton()

    init(settings: AppSettings, config: ConfigStore,
         logProvider: @escaping () -> String, onProfileGenerated: @escaping (String) -> Void) {
        self.settings = settings
        self.config = config
        self.logProvider = logProvider
        self.onProfileGenerated = onProfileGenerated
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 560, height: 480),
                              styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.title = "AI Assistant"
        super.init(window: window)
        window.contentView = build()
        window.center()
        if settings.aiProvider.isEmpty { status.stringValue = "Configure an AI provider in Settings first." }
    }
    required init?(coder: NSCoder) { fatalError() }

    private func build() -> NSView {
        answerView.isEditable = false
        answerView.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        answerView.textContainerInset = NSSize(width: 8, height: 8)
        let answerScroll = NSScrollView()
        answerScroll.documentView = answerView
        answerScroll.hasVerticalScroller = true
        answerScroll.borderType = .bezelBorder
        answerScroll.translatesAutoresizingMaskIntoConstraints = false

        questionField.placeholderString = "Ask about the current log…"
        questionField.target = self
        questionField.action = #selector(ask)

        askButton.title = "Ask"; askButton.target = self; askButton.action = #selector(ask)
        askButton.bezelStyle = .rounded; askButton.keyEquivalent = "\r"
        genButton.title = "Generate Rules Profile"; genButton.target = self; genButton.action = #selector(generateRules)
        genButton.bezelStyle = .rounded

        status.font = .systemFont(ofSize: 11)
        status.textColor = .secondaryLabelColor

        let controls = NSStackView(views: [questionField, askButton])
        controls.orientation = .horizontal; controls.spacing = 8
        questionField.translatesAutoresizingMaskIntoConstraints = false
        questionField.widthAnchor.constraint(greaterThanOrEqualToConstant: 360).isActive = true

        let bottom = NSStackView(views: [genButton, status])
        bottom.orientation = .horizontal; bottom.spacing = 12

        let root = NSStackView(views: [answerScroll, controls, bottom])
        root.orientation = .vertical; root.spacing = 10
        root.edgeInsets = NSEdgeInsets(top: 14, left: 14, bottom: 14, right: 14)
        root.translatesAutoresizingMaskIntoConstraints = false

        let container = NSView()
        container.addSubview(root)
        NSLayoutConstraint.activate([
            root.topAnchor.constraint(equalTo: container.topAnchor),
            root.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            root.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            root.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            answerScroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 320),
        ])
        return container
    }

    // MARK: - Actions

    @objc private func ask() {
        let q = questionField.stringValue.trimmingCharacters(in: .whitespaces)
        guard !q.isEmpty else { return }
        run(messages: AIPrompts.logMessages(logContent: logProvider(), question: q), busy: "Asking…") { [weak self] text in
            self?.answerView.string = text
        }
    }

    @objc private func generateRules() {
        run(messages: AIPrompts.ruleGenMessages(logContent: logProvider()), busy: "Generating rules…") { [weak self] text in
            self?.applyGeneratedRules(text)
        }
    }

    private func applyGeneratedRules(_ text: String) {
        guard let rules = parseRules(text), !rules.isEmpty else {
            answerView.string = "Could not parse rules from the AI response:\n\n\(text)"
            return
        }
        let name = "AI Generated \(rules.count) rules"
        config.saveProfile(Profile(name: name, rules: rules))
        onProfileGenerated(name)
        answerView.string = "Created profile “\(name)” with \(rules.count) rules and set it active."
    }

    /// Extracts the JSON rule array from a model response (tolerating prose/fences).
    private func parseRules(_ text: String) -> [Rule]? {
        guard let start = text.firstIndex(of: "["), let end = text.lastIndex(of: "]") else { return nil }
        let json = String(text[start...end])
        return try? JSONDecoder().decode([Rule].self, from: Data(json.utf8))
    }

    private func run(messages: [AIMessage], busy: String, onText: @escaping (String) -> Void) {
        setBusy(true, busy)
        ensureClient { [weak self] result in
            guard let self else { return }
            switch result {
            case .failure(let e): self.fail(e)
            case .success(let client):
                client.chat(messages) { [weak self] r in
                    guard let self else { return }
                    self.setBusy(false, "")
                    switch r {
                    case .success(let text): onText(text)
                    case .failure(let e): self.fail(e)
                    }
                }
            }
        }
    }

    /// Resolves a chat backend, kicking off Copilot sign-in if needed.
    private func ensureClient(_ completion: @escaping (Result<any ChatBackend, Error>) -> Void) {
        AIService.makeClient(settings: settings) { [weak self] result in
            if case .failure(let e) = result, case AIError.needsCopilotAuth = e {
                self?.startCopilotSignIn(then: completion)
            } else {
                completion(result)
            }
        }
    }

    private func startCopilotSignIn(then completion: @escaping (Result<any ChatBackend, Error>) -> Void) {
        setBusy(true, "Requesting Copilot device code…")
        CopilotAuth.requestDeviceCode { [weak self] result in
            guard let self else { return }
            switch result {
            case .failure(let e): self.fail(e)
            case .success(let dc):
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(dc.userCode, forType: .string)
                self.answerView.string = "To use Copilot:\n\n1. Your code (copied to clipboard): \(dc.userCode)\n2. A browser is opening \(dc.verificationURI)\n3. Enter the code, then return here.\n\nWaiting for authorization…"
                if let url = URL(string: dc.verificationURI) { NSWorkspace.shared.open(url) }
                CopilotAuth.pollForToken(deviceCode: dc.deviceCode, interval: dc.interval) { [weak self] tokenResult in
                    switch tokenResult {
                    case .failure(let e): self?.fail(e)
                    case .success: AIService.makeClient(settings: self?.settings ?? AppSettings(), completion: completion)
                    }
                }
            }
        }
    }

    private func setBusy(_ busy: Bool, _ msg: String) {
        askButton.isEnabled = !busy; genButton.isEnabled = !busy
        status.stringValue = msg
    }

    private func fail(_ error: Error) {
        setBusy(false, "")
        answerView.string = "Error: \(error.localizedDescription)"
    }
}
