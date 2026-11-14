import CoreBluetooth
import Foundation
@_exported import OpenPlugCore

/// The dongle's BLE transport (see research/notes/ble_transport.md).
///
/// The same bare TLS records as on Bluetooth Classic, moved like this:
/// - send: write the bytes to the data characteristic (with response);
/// - receive: each one-byte `0xff` notification on the doorbell characteristic means
///   one chunk is waiting; read the data characteristic to fetch it.
///
/// Finding the dongle: a known dongle (``DongleCentral/knownPeripheral``) gets a
/// pending connect, which never times out and which iOS completes in the background;
/// at the same time a scan for the dongle's service finds a new one. A connection
/// restored by the system after a relaunch is picked up directly.
///
/// Core Bluetooth runs on the shared central's queue; `open`, `send` and `recv` block
/// the caller, which must not be the main thread of a UI app.
public final class CoreBluetoothTransport: NSObject, ByteTransport, @unchecked Sendable {
    public static let service = CBUUID(string: "732DB477-BFD6-4694-9378-86C246F35DEF")
    static let dataUUID = CBUUID(string: "478A86F3-E47C-4618-9B15-3C7C92CC8184")
    static let doorbellUUID = CBUUID(string: "2EFB2DD4-CDC3-11E6-9D9D-CEC0C932CE01")

    public enum OpenError: Error, CustomStringConvertible {
        case bluetoothUnavailable(String)
        case timeout(String)
        case failed(String)

        public var description: String {
            switch self {
            case .bluetoothUnavailable(let s): return "Bluetooth unavailable: \(s)"
            case .timeout(let s): return "timed out while \(s)"
            case .failed(let s): return s
            }
        }
    }

    private let hub = DongleCentral.shared
    private var queue: DispatchQueue { hub.queue }
    private var central: CBCentralManager { hub.central }

    // Touched only on `queue`.
    private var begun = false
    /// Being connected (pending connect) but not yet ours.
    private var candidates: [CBPeripheral] = []
    private var peripheral: CBPeripheral?
    private var dataChar: CBCharacteristic?
    private var pendingBells = 0
    private var reading = false

    // Guarded by `cond`.
    private let cond = NSCondition()
    private var rx = Data()
    private var closed = false
    private var stage = "starting Bluetooth"
    private var openResult: Result<Void, OpenError>?
    private var writeResult: Result<Void, TransportError>?

    /// The name the dongle reported over BLE (seen: `DP_2_1_<serial>` and `CARe`).
    public private(set) var peripheralName: String?

    /// While Bluetooth is off, keep waiting for it (an app session) instead of failing
    /// (a one-shot command-line run).
    private let waitForBluetooth: Bool

    public init(waitForBluetooth: Bool = false) {
        self.waitForBluetooth = waitForBluetooth
        super.init()
    }

    /// Starts looking for the dongle without waiting. Call it as early as possible
    /// after losing a connection: the pending connect it places is what lets iOS
    /// reconnect while the app is suspended.
    public func begin() {
        queue.async {
            guard !self.begun else { return }
            self.begun = true
            self.hub.attach(self)
        }
    }

    /// Finds the dongle, connects and prepares both characteristics. Waits at most
    /// `timeout` seconds, or until ``close()`` when `timeout` is nil.
    public func open(timeout: TimeInterval? = 20) throws {
        begin()
        let deadline = timeout.map { Date().addingTimeInterval($0) }
        cond.lock()
        defer { cond.unlock() }
        while openResult == nil && !closed {
            if let deadline {
                if !cond.wait(until: deadline) {
                    let where_ = stage
                    cond.unlock()
                    close()
                    cond.lock()
                    throw OpenError.timeout(where_)
                }
            } else {
                cond.wait()
            }
        }
        guard let result = openResult else { throw OpenError.failed("closed") }
        try result.get()
    }

    public func close() {
        queue.async { self.teardown() }
    }

    // MARK: ByteTransport

    public func send(data: Data) throws {
        cond.lock()
        defer { cond.unlock() }
        guard !closed, let p = peripheral, let c = dataChar else {
            throw TransportError.Io(message: "not connected")
        }
        writeResult = nil
        queue.async { p.writeValue(data, for: c, type: .withResponse) }
        while writeResult == nil && !closed {
            cond.wait()
        }
        guard let result = writeResult else {
            throw TransportError.Io(message: "disconnected during write")
        }
        try result.get()
    }

    public func recv(maxLen: UInt32) throws -> Data {
        cond.lock()
        defer { cond.unlock() }
        while rx.isEmpty && !closed {
            cond.wait()
        }
        let n = min(Int(maxLen), rx.count)
        let out = rx.prefix(n)
        rx.removeFirst(n)
        return Data(out) // empty once closed and drained
    }

    // MARK: internals (on `queue`)

    private func setStage(_ s: String) {
        cond.lock(); stage = s; cond.unlock()
    }

    private func finishOpen(_ r: Result<Void, OpenError>) {
        cond.lock()
        if openResult == nil { openResult = r }
        cond.broadcast()
        cond.unlock()
    }

    private func teardown() {
        if central.isScanning { central.stopScan() }
        for p in candidates + [peripheral].compactMap({ $0 }) {
            central.cancelPeripheralConnection(p)
        }
        candidates = []
        hub.detach(self)
        markClosed()
    }

    private func markClosed() {
        cond.lock()
        closed = true
        cond.broadcast()
        cond.unlock()
    }

    private var isClosed: Bool {
        cond.lock(); defer { cond.unlock() }
        return closed
    }

    func owns(_ p: CBPeripheral) -> Bool {
        p === peripheral || candidates.contains { $0 === p }
    }

    /// Takes `p` as the dongle and sets it up (it is connected).
    private func adopt(_ p: CBPeripheral) {
        if central.isScanning { central.stopScan() }
        for other in candidates where other !== p {
            central.cancelPeripheralConnection(other)
        }
        candidates = []
        peripheral = p
        peripheralName = peripheralName ?? p.name
        DongleCentral.knownPeripheral = p.identifier
        p.delegate = self
        setStage("discovering services")
        p.discoverServices([Self.service])
    }

    private func connect(_ p: CBPeripheral) {
        guard !candidates.contains(where: { $0 === p }) else { return }
        candidates.append(p)
        central.connect(p) // pending until the dongle is in reach; never times out
    }

    /// Where to look once Bluetooth is on.
    private func search() {
        guard peripheral == nil, !isClosed else { return }
        if let p = hub.takeConnected() {
            return adopt(p) // restored by the system, or connected meanwhile
        }
        if let p = hub.takeConnecting() {
            candidates.append(p) // a restored pending connect: keep waiting on it
        }
        if let id = DongleCentral.knownPeripheral,
           let p = central.retrievePeripherals(withIdentifiers: [id]).first {
            setStage("waiting for the dongle")
            connect(p)
        } else {
            setStage("scanning for the dongle")
        }
        // A scan also finds a dongle we have not met (or one that changed identifier).
        central.scanForPeripherals(withServices: [Self.service])
    }

    func centralDidUpdateState(_ central: CBCentralManager) {
        switch central.state {
        case .poweredOn:
            search()
        case .unknown, .resetting:
            break
        case .unauthorized:
            finishOpen(.failure(.bluetoothUnavailable("permission denied")))
        case .poweredOff:
            if peripheral != nil || !waitForBluetooth {
                finishOpen(.failure(.bluetoothUnavailable("powered off")))
                markClosed()
            } else {
                candidates = [] // Core Bluetooth dropped them; search again once it is on
                setStage("waiting for Bluetooth")
            }
        default:
            finishOpen(.failure(.bluetoothUnavailable("unsupported")))
        }
    }

    func didDiscover(_ p: CBPeripheral, advertisementData: [String: Any]) {
        guard peripheral == nil else { return }
        if let name = advertisementData[CBAdvertisementDataLocalNameKey] as? String { peripheralName = name }
        setStage("connecting")
        connect(p)
    }

    func didConnect(_ p: CBPeripheral) {
        guard peripheral == nil else { return }
        adopt(p)
    }

    func didFailToConnect(_ p: CBPeripheral, error: Error?) {
        guard owns(p) else { return }
        candidates.removeAll { $0 === p }
        if candidates.isEmpty && peripheral == nil {
            finishOpen(.failure(.failed("connect failed: \(error?.localizedDescription ?? "unknown")")))
            markClosed()
        }
    }

    func didDisconnect(_ p: CBPeripheral, error: Error?) {
        guard owns(p) else { return }
        finishOpen(.failure(.failed("disconnected: \(error?.localizedDescription ?? "closed")")))
        markClosed()
    }

    /// One read per doorbell, one at a time.
    private func pump() {
        guard !reading, pendingBells > 0, let p = peripheral, let c = dataChar else { return }
        pendingBells -= 1
        reading = true
        p.readValue(for: c)
    }
}

extension CoreBluetoothTransport: CBPeripheralDelegate {
    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard let s = peripheral.services?.first(where: { $0.uuid == Self.service }) else {
            return finishOpen(.failure(.failed("the dongle service is missing")))
        }
        peripheral.discoverCharacteristics([Self.dataUUID, Self.doorbellUUID], for: s)
    }

    public func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        let chars = service.characteristics ?? []
        guard let data = chars.first(where: { $0.uuid == Self.dataUUID }),
              let bell = chars.first(where: { $0.uuid == Self.doorbellUUID }) else {
            return finishOpen(.failure(.failed("the dongle characteristics are missing")))
        }
        dataChar = data
        setStage("enabling notifications")
        peripheral.setNotifyValue(true, for: bell)
    }

    public func peripheral(_ peripheral: CBPeripheral, didUpdateNotificationStateFor characteristic: CBCharacteristic, error: Error?) {
        if let error {
            finishOpen(.failure(.failed("enabling notifications failed: \(error.localizedDescription)")))
        } else {
            finishOpen(.success(()))
        }
    }

    public func peripheral(_ peripheral: CBPeripheral, didWriteValueFor characteristic: CBCharacteristic, error: Error?) {
        cond.lock()
        writeResult = error.map { .failure(TransportError.Io(message: $0.localizedDescription)) } ?? .success(())
        cond.broadcast()
        cond.unlock()
    }

    public func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        if characteristic.uuid == Self.doorbellUUID {
            pendingBells += 1 // the payload (0xff) carries no data
        } else if characteristic.uuid == Self.dataUUID {
            reading = false
            if let v = characteristic.value, !v.isEmpty, error == nil {
                cond.lock()
                rx.append(v)
                cond.broadcast()
                cond.unlock()
            }
        }
        pump()
    }
}
