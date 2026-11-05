// Utter's input method: shows the live draft as marked text in the focused field and
// commits the final text (spec 2026-11-04 §5.3). Talks to Utter over a Unix socket.
import Cocoa
import InputMethodKit

let connectionName = Bundle.main.infoDictionary?["InputMethodConnectionName"] as? String
    ?? "app.utter.inputmethod.live_Connection"
let server = IMKServer(name: connectionName, bundleIdentifier: Bundle.main.bundleIdentifier)
_ = server
IMLog.write("started, version \(Channel.version)")
Channel.shared.start()
NSApplication.shared.run()
