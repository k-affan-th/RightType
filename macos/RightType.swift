import AppKit
import Darwin

enum RepairError: LocalizedError {
    case tooLarge, unavailable, failed

    var errorDescription: String? {
        switch self {
        case .tooLarge: "Please use a shorter passage (up to 32 KiB)."
        case .unavailable: "RightType is incomplete. Rebuild or reinstall this preview."
        case .failed: "The text could not be fixed. Your original is unchanged."
        }
    }
}

enum TextEngine {
    static let maxBytes = 32 * 1024
    static var helper: URL {
        Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/righttype-text")
    }

    static func repair(_ text: String, helper: URL = helper) throws -> String {
        guard text.utf8.count <= maxBytes else { throw RepairError.tooLarge }
        guard FileManager.default.isExecutableFile(atPath: helper.path) else {
            throw RepairError.unavailable
        }
        let process = Process()
        let input = Pipe(), output = Pipe()
        process.executableURL = helper
        process.standardInput = input
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { throw RepairError.unavailable }
        // A hung engine must not keep the preview busy indefinitely.
        DispatchQueue.global().asyncAfter(deadline: .now() + 10) {
            if process.isRunning { process.terminate() }
        }
        defer {
            try? input.fileHandleForWriting.close()
            try? output.fileHandleForReading.close()
            if process.isRunning { process.terminate() }
        }
        do {
            try input.fileHandleForWriting.write(contentsOf: Data(text.utf8))
            try input.fileHandleForWriting.close()
            let data = try output.fileHandleForReading.readToEnd() ?? Data()
            process.waitUntilExit()
            guard process.terminationReason == .exit, process.terminationStatus == 0,
                  let result = String(data: data, encoding: .utf8) else {
                throw RepairError.failed
            }
            return result
        } catch {
            throw RepairError.failed
        }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate, NSTextViewDelegate {
    private var statusItem: NSStatusItem!
    private var window: NSWindow!
    private let source = NSTextView(), result = NSTextView()
    private let message = NSTextField(wrappingLabelWithString:
        "Paste Thai / English text, then preview the fix. Your original stays on the left.")
    private var previewButton: NSButton!
    private var copyButton: NSButton!
    private var revision = 0
    private var busy = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        let main = NSMenu()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Quit RightType", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let appItem = NSMenuItem()
        appItem.submenu = appMenu
        main.addItem(appItem)
        let edit = NSMenu(title: "Edit")
        // AppKit resolves undo: dynamically through the responder chain.
        for (title, action, key) in [
            ("Undo", Selector(("undo:")), "z"),
            ("Cut", #selector(NSText.cut(_:)), "x"),
            ("Copy", #selector(NSText.copy(_:)), "c"),
            ("Paste", #selector(NSText.paste(_:)), "v"),
            ("Select All", #selector(NSText.selectAll(_:)), "a")
        ] {
            edit.addItem(withTitle: title, action: action, keyEquivalent: key)
        }
        let editItem = NSMenuItem(title: "Edit", action: nil, keyEquivalent: "")
        editItem.submenu = edit
        main.addItem(editItem)
        NSApp.mainMenu = main

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.title = "Aก"
        statusItem.button?.setAccessibilityLabel("RightType")
        let menu = NSMenu()
        let openItem = menu.addItem(withTitle: "Fix Text…", action: #selector(showWindow), keyEquivalent: "")
        openItem.target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit RightType", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        statusItem.menu = menu
        buildWindow()
        showWindow()
    }

    private func editor(_ view: NSTextView, label: String, editable: Bool) -> NSView {
        view.isRichText = false
        view.isEditable = editable
        view.isSelectable = true
        view.allowsUndo = editable
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticSpellingCorrectionEnabled = false
        view.font = .systemFont(ofSize: 17)
        view.textColor = .textColor
        view.backgroundColor = .textBackgroundColor
        view.textContainerInset = NSSize(width: 10, height: 10)
        view.autoresizingMask = [.width]
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = false
        view.textContainer?.widthTracksTextView = true
        view.setAccessibilityLabel(label)
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        scroll.documentView = view
        let title = NSTextField(labelWithString: label)
        title.font = .boldSystemFont(ofSize: 13)
        let stack = NSStackView(views: [title, scroll])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        scroll.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        return stack
    }

    private func buildWindow() {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 760, height: 440),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "RightType — Fix Text (Mac Preview)"
        window.minSize = NSSize(width: 600, height: 380)
        window.isReleasedWhenClosed = false
        window.delegate = self
        source.delegate = self
        previewButton = NSButton(title: "Preview Fix", target: self, action: #selector(preview))
        previewButton.bezelStyle = .rounded
        copyButton = NSButton(title: "Copy Result", target: self, action: #selector(copyResult))
        copyButton.bezelStyle = .rounded
        copyButton.isEnabled = false
        let paste = NSButton(title: "Paste Text", target: self, action: #selector(pasteText))
        paste.bezelStyle = .rounded
        let clear = NSButton(title: "Clear", target: self, action: #selector(clearText))
        clear.bezelStyle = .rounded
        let buttons = NSStackView(views: [paste, previewButton, copyButton, clear])
        buttons.spacing = 10
        let editors = NSStackView(views: [editor(source, label: "Original", editable: true),
                                          editor(result, label: "Preview", editable: false)])
        editors.distribution = .fillEqually
        editors.spacing = 16
        message.textColor = .secondaryLabelColor
        let stack = NSStackView(views: [message, editors, buttons])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 16
        stack.translatesAutoresizingMaskIntoConstraints = false
        let content = window.contentView!
        content.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 20),
            stack.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
            stack.topAnchor.constraint(equalTo: content.topAnchor, constant: 20),
            stack.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -20),
            editors.widthAnchor.constraint(equalTo: stack.widthAnchor),
            message.widthAnchor.constraint(equalTo: stack.widthAnchor)
        ])
        window.center()
    }

    @objc private func showWindow() {
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        window.makeFirstResponder(source)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        showWindow()
        return true
    }

    func textDidChange(_ notification: Notification) {
        revision += 1
        result.string = ""
        copyButton.isEnabled = false
        message.stringValue = "Choose Preview Fix to review this text."
    }

    @objc private func pasteText() {
        guard let text = NSPasteboard.general.string(forType: .string) else {
            message.stringValue = "The clipboard has no plain text."
            return
        }
        source.string = text
        textDidChange(Notification(name: NSText.didChangeNotification))
    }

    @objc private func preview() {
        guard !busy else { return }
        let text = source.string
        guard !text.isEmpty else {
            message.stringValue = "Add some text first."
            return
        }
        guard text.utf8.count <= TextEngine.maxBytes else {
            message.stringValue = RepairError.tooLarge.localizedDescription
            return
        }
        let request = revision
        busy = true
        previewButton.isEnabled = false
        copyButton.isEnabled = false
        message.stringValue = "Preparing the preview…"
        DispatchQueue.global(qos: .userInitiated).async {
            let outcome = Result { try TextEngine.repair(text) }
            DispatchQueue.main.async { [self] in
                busy = false
                previewButton.isEnabled = true
                // An edit or close invalidates an in-flight preview.
                guard request == revision else { return }
                switch outcome {
                case .success(let fixed):
                    result.string = fixed
                    copyButton.isEnabled = !fixed.isEmpty
                    message.stringValue = fixed == text ? "No confident fixes found. Your text is unchanged."
                        : "Review the preview, then copy it when you're ready."
                case .failure(let error):
                    result.string = ""
                    message.stringValue = error.localizedDescription
                }
            }
        }
    }

    @objc private func copyResult() {
        guard copyButton.isEnabled else { return }
        NSPasteboard.general.clearContents()
        message.stringValue = NSPasteboard.general.setString(result.string, forType: .string)
            ? "Result copied." : "The result could not be copied."
    }

    @objc private func clearText() {
        source.string = ""
        source.undoManager?.removeAllActions()
        textDidChange(Notification(name: NSText.didChangeNotification))
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        clearText()
        sender.orderOut(nil)
        return false
    }
}

func selfTest() throws {
    let sample = "l;ylfu\tแนพพำแะ\nhello สวัสดี 😀  "
    guard try TextEngine.repair(sample) == "สวัสดี\tcorrect\nhello สวัสดี 😀  ",
          try TextEngine.repair("") == "",
          try TextEngine.repair("bc1qexampleaddress") == "bc1qexampleaddress" else {
        throw RepairError.failed
    }
    do {
        _ = try TextEngine.repair(String(repeating: "a", count: TextEngine.maxBytes + 1))
        throw RepairError.failed
    } catch RepairError.tooLarge { }
    do {
        _ = try TextEngine.repair("hello", helper: URL(fileURLWithPath: "/nonexistent/righttype-test"))
        throw RepairError.failed
    } catch RepairError.unavailable { }
    print("Mac text bridge self-check passed.")
}

signal(SIGPIPE, SIG_IGN)
if CommandLine.arguments == [CommandLine.arguments[0], "--self-test"] {
    do { try selfTest() } catch {
        fputs("Mac text bridge self-check failed.\n", stderr)
        exit(1)
    }
} else {
    let app = NSApplication.shared
    let delegate = AppDelegate()
    app.setActivationPolicy(.accessory)
    app.delegate = delegate
    app.run()
}
