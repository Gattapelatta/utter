import Cocoa
import InputMethodKit

/// One controller per client field. Keys pass through to the app; F19 (the "wake" key Utter
/// posts so the field connects at once) is swallowed.
@objc(UtterController)
final class UtterController: IMKInputController {
    static let wakeKeyCode: UInt16 = 80  // F19

    /// The controller of the field that is active now.
    static weak var current: UtterController?

    private var textClient: IMKTextInput? { client() }

    override func handle(_ event: NSEvent!, client sender: Any!) -> Bool {
        guard let event, event.type == .keyDown || event.type == .keyUp else { return false }
        return event.keyCode == Self.wakeKeyCode
    }

    override func activateServer(_ sender: Any!) {
        super.activateServer(sender)
        Self.current = self
        let bundle = (sender as? IMKTextInput)?.bundleIdentifier() ?? ""
        Channel.shared.send(["t": "active", "bundle": bundle])
        if let draft = Channel.shared.pendingDraft {
            showDraft(draft)
        }
    }

    override func deactivateServer(_ sender: Any!) {
        if Self.current === self {
            Self.current = nil
            Channel.shared.send(["t": "inactive"])
        }
        super.deactivateServer(sender)
    }

    /// The app asks to finish (focus moves away): the draft is cleared, never committed.
    override func commitComposition(_ sender: Any!) {
        clearDraft()
    }

    func showDraft(_ text: String) {
        guard let c = textClient else { return }
        let marked = NSAttributedString(string: text, attributes: [.underlineStyle: NSUnderlineStyle.single.rawValue])
        c.setMarkedText(marked, selectionRange: NSRange(location: text.utf16.count, length: 0), replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    func commit(_ text: String) {
        guard let c = textClient else { return }
        c.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    func clearDraft() {
        guard let c = textClient else { return }
        c.setMarkedText("", selectionRange: NSRange(location: 0, length: 0), replacementRange: NSRange(location: NSNotFound, length: 0))
    }
}
