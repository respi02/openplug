import CoreBluetooth
import Foundation

/// The app's one Core Bluetooth central, shared by every [`CoreBluetoothTransport`].
///
/// On iOS it is created with a restore identifier, so the system keeps the app's
/// pending and open connections while the app is suspended or even terminated, and
/// relaunches the app in the background when the dongle connects. For that the central
/// has to exist from launch: call ``activate()`` early (app init).
///
/// It remembers the last dongle it connected to, so a later session reconnects by
/// identifier — a pending connect that never times out and that iOS completes in the
/// background — instead of scanning.
public final class DongleCentral: NSObject, @unchecked Sendable {
    public static let shared = DongleCentral()

    #if os(iOS)
    public static let restoreIdentifier = "org.openplug.central"
    #endif

    /// Every Core Bluetooth callback runs here.
    let queue = DispatchQueue(label: "openplug.ble")
    private(set) var central: CBCentralManager!

    // Touched only on `queue`.
    private weak var client: CoreBluetoothTransport?
    /// Peripherals handed back by state restoration, or connected with no transport
    /// waiting for them (the connect completed while the app was relaunching).
    private var spare: [CBPeripheral] = []

    private override init() {
        super.init()
        var options: [String: Any] = [:]
        #if os(iOS)
        options[CBCentralManagerOptionRestoreIdentifierKey] = Self.restoreIdentifier
        #endif
        central = CBCentralManager(delegate: self, queue: queue, options: options)
    }

    /// Creates the central now, so iOS can hand back its restored state.
    public func activate() {}

    private static let knownKey = "OpenPlug.dongleIdentifier"

    /// The Core Bluetooth identifier of the last dongle connected (per phone).
    public static var knownPeripheral: UUID? {
        get { UserDefaults.standard.string(forKey: knownKey).flatMap(UUID.init(uuidString:)) }
        set { UserDefaults.standard.set(newValue?.uuidString, forKey: knownKey) }
    }

    // MARK: for the transport (on `queue`)

    func attach(_ t: CoreBluetoothTransport) {
        dispatchPrecondition(condition: .onQueue(queue))
        client = t
        if central.state != .unknown { t.centralDidUpdateState(central) }
    }

    func detach(_ t: CoreBluetoothTransport) {
        dispatchPrecondition(condition: .onQueue(queue))
        if client === t { client = nil }
    }

    /// A peripheral that is already connected to us, if any (restored, or connected
    /// while no transport was waiting).
    func takeConnected() -> CBPeripheral? {
        dispatchPrecondition(condition: .onQueue(queue))
        spare.removeAll { $0.state == .disconnected }
        if let i = spare.firstIndex(where: { $0.state == .connected }) {
            return spare.remove(at: i)
        }
        return central.retrieveConnectedPeripherals(withServices: [CoreBluetoothTransport.service])
            .first { $0.state == .connected }
    }

    /// A restored peripheral still being connected (a pending connect from before the
    /// app was relaunched).
    func takeConnecting() -> CBPeripheral? {
        dispatchPrecondition(condition: .onQueue(queue))
        guard let i = spare.firstIndex(where: { $0.state == .connecting }) else { return nil }
        return spare.remove(at: i)
    }
}

extension DongleCentral: CBCentralManagerDelegate {
    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        client?.centralDidUpdateState(central)
    }

    #if os(iOS)
    public func centralManager(_ central: CBCentralManager, willRestoreState dict: [String: Any]) {
        let restored = dict[CBCentralManagerRestoredStatePeripheralsKey] as? [CBPeripheral] ?? []
        spare.append(contentsOf: restored)
    }
    #endif

    public func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral,
                               advertisementData: [String: Any], rssi RSSI: NSNumber) {
        client?.didDiscover(peripheral, advertisementData: advertisementData)
    }

    public func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        if let client, client.owns(peripheral) {
            client.didConnect(peripheral)
        } else {
            spare.append(peripheral) // the next transport picks it up
        }
    }

    public func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        client?.didFailToConnect(peripheral, error: error)
    }

    public func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        spare.removeAll { $0 === peripheral }
        client?.didDisconnect(peripheral, error: error)
    }
}
