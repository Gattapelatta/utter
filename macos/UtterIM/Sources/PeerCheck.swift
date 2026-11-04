import CryptoKit
import Foundation
import Security

/// Only Utter itself may drive the input method: the socket's server must be signed with
/// identifier `app.utter.desktop` and the same leaf certificate as this input method.
enum PeerCheck {
    static func ownLeafSHA1() -> String? {
        var me: SecCode?
        guard SecCodeCopySelf([], &me) == errSecSuccess, let me else { return nil }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(me, [], &staticCode) == errSecSuccess, let staticCode else { return nil }
        var info: CFDictionary?
        guard SecCodeCopySigningInformation(staticCode, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
              let dict = info as? [String: Any],
              let certs = dict[kSecCodeInfoCertificates as String] as? [SecCertificate],
              let leaf = certs.first else { return nil }
        let der = SecCertificateCopyData(leaf) as Data
        return Insecure.SHA1.hash(data: der).map { String(format: "%02X", $0) }.joined()
    }

    static func serverIsUtter(fd: Int32) -> Bool {
        guard let leaf = ownLeafSHA1() else {
            IMLog.write("peer check: own signature unreadable")
            return false
        }
        var token = audit_token_t()
        var len = socklen_t(MemoryLayout<audit_token_t>.size)
        // SOL_LOCAL = 0, LOCAL_PEERTOKEN = 6 (sys/un.h).
        guard getsockopt(fd, 0, 6, &token, &len) == 0 else { return false }
        let tokenData = withUnsafeBytes(of: &token) { Data($0) }
        let attrs = [kSecGuestAttributeAudit: tokenData] as CFDictionary
        var guest: SecCode?
        guard SecCodeCopyGuestWithAttributes(nil, attrs, [], &guest) == errSecSuccess, let guest else { return false }
        var req: SecRequirement?
        let text = "identifier \"app.utter.desktop\" and certificate leaf = H\"\(leaf)\""
        guard SecRequirementCreateWithString(text as CFString, [], &req) == errSecSuccess, let req else { return false }
        return SecCodeCheckValidity(guest, [], req) == errSecSuccess
    }
}
