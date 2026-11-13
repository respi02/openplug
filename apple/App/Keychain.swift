import Foundation
import Security

/// The label PIN, kept in the Keychain so the user types it once.
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
            SecItemAdd(q as CFDictionary, nil)
        }
    }
}
