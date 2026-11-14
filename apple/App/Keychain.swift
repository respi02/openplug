import Foundation
import Security

/// The label PIN, kept in the Keychain so the user types it once.
///
/// Readable after the first unlock since boot, also while the phone is locked: a
/// session that iOS resumes in the background (car started, phone in a pocket) needs it.
enum Keychain {
    private static let query: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "org.openplug.app",
        kSecAttrAccount as String: "dongle-pin",
    ]

    static var pin: String? {
        get {
            var q = query
            q[kSecReturnData as String] = true
            var out: CFTypeRef?
            guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess,
                  let data = out as? Data else { return nil }
            return String(data: data, encoding: .utf8)
        }
        set {
            SecItemDelete(query as CFDictionary)
            guard let value = newValue, !value.isEmpty else { return }
            var q = query
            q[kSecValueData as String] = Data(value.utf8)
            q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            SecItemAdd(q as CFDictionary, nil)
        }
    }

    /// Moves a PIN saved by an earlier version (readable only while unlocked) to the
    /// accessibility above. A no-op when it cannot be read right now.
    static func upgradeAccessibility() {
        let update: [String: Any] = [kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly]
        SecItemUpdate(query as CFDictionary, update as CFDictionary)
    }
}
