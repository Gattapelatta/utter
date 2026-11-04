import Foundation

/// Appends to ~/Library/Logs/app.utter.desktop/im.log (never message text).
enum IMLog {
    static let url: URL = {
        let dir = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Logs/app.utter.desktop")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("im.log")
    }()

    static func write(_ line: String) {
        let stamp = ISO8601DateFormatter().string(from: Date())
        guard let data = "\(stamp) \(line)\n".data(using: .utf8) else { return }
        if let h = try? FileHandle(forWritingTo: url) {
            h.seekToEndOfFile()
            h.write(data)
            try? h.close()
        } else {
            try? data.write(to: url)
        }
    }
}

/// Client of Utter's socket: one JSON object per line. Reconnects every second while Utter
/// is not running; before the first message the server's signature is checked.
final class Channel {
    static let shared = Channel()
    static let version = 1

    private let path = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/app.utter.desktop/im.sock").path
    /// Written on `writes` only; the reader thread hands it over there.
    private var fd: Int32 = -1
    private let writes = DispatchQueue(label: "app.utter.inputmethod.writes")

    /// The last draft received; shown when a field activates after it arrived.
    private(set) var pendingDraft: String?

    func start() {
        let reader = Thread { self.loop() }
        reader.name = "app.utter.inputmethod.reader"
        reader.start()
    }

    private func loop() {
        while true {
            if let s = connect() {
                if PeerCheck.serverIsUtter(fd: s) {
                    IMLog.write("connected")
                    writes.sync { fd = s }
                    send(["t": "hello", "version": Channel.version])
                    if let bundle = UtterControllerBridge.activeBundle() {
                        send(["t": "active", "bundle": bundle])
                    }
                    read(s)
                    IMLog.write("disconnected")
                    writes.sync { fd = -1 }
                } else {
                    IMLog.write("peer rejected")
                }
                close(s)
            }
            Thread.sleep(forTimeInterval: 1)
        }
    }

    private func connect() -> Int32? {
        let s = socket(AF_UNIX, SOCK_STREAM, 0)
        guard s >= 0 else { return nil }
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8)
        guard bytes.count < MemoryLayout.size(ofValue: addr.sun_path) else { close(s); return nil }
        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            raw.copyBytes(from: bytes)
            raw[bytes.count] = 0
        }
        let ok = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(s, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        if ok != 0 {
            close(s)
            return nil
        }
        var one: Int32 = 1
        setsockopt(s, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))
        return s
    }

    private func read(_ s: Int32) {
        var buffer = Data()
        var chunk = [UInt8](repeating: 0, count: 64 * 1024)
        while true {
            let n = Darwin.read(s, &chunk, chunk.count)
            if n <= 0 { return }
            buffer.append(chunk, count: n)
            while let nl = buffer.firstIndex(of: 0x0A) {
                let line = buffer.subdata(in: buffer.startIndex..<nl)
                buffer.removeSubrange(buffer.startIndex...nl)
                handle(line)
            }
        }
    }

    private func handle(_ line: Data) {
        guard let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
              let t = obj["t"] as? String else { return }
        let text = obj["text"] as? String ?? ""
        DispatchQueue.main.async {
            let c = UtterController.current
            switch t {
            case "draft":
                self.pendingDraft = text
                c?.showDraft(text)
            case "commit":
                self.pendingDraft = nil
                c?.commit(text)
            case "clear":
                self.pendingDraft = nil
                c?.clearDraft()
            default:
                break
            }
        }
    }

    func send(_ msg: [String: Any]) {
        writes.async {
            guard self.fd >= 0,
                  var data = try? JSONSerialization.data(withJSONObject: msg) else { return }
            data.append(0x0A)
            _ = data.withUnsafeBytes { Darwin.write(self.fd, $0.baseAddress, data.count) }
        }
    }
}

/// Reads the active field's bundle on the main thread (AppKit objects live there).
enum UtterControllerBridge {
    static func activeBundle() -> String? {
        DispatchQueue.main.sync {
            UtterController.current?.client()?.bundleIdentifier()
        }
    }
}
