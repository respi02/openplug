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
/// Core Bluetooth runs on a private queue; `open`, `send` and `recv` block the
/// caller, which must not be the main thread of a UI app.
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

    private let queue = DispatchQueue(label: "openplug.ble")
    private var central: CBCentralManager!
    private var peripheral: CBPeripheral?
    private var dataChar: CBCharacteristic?

    // Guarded by `cond`.
    private let cond = NSCondition()
    private var rx = Data()
    private var closed = false
    private var stage = "starting Bluetooth"
    private var openResult: Result<Void, OpenError>?
    private var writeResult: Result<Void, TransportError>?

    // Touched only on `queue`.
    private var pendingBells = 0
    private var reading = false

    /// The name the dongle reported over BLE (seen: `DP_2_1_<serial>` and `CARe`).
    public private(set) var peripheralName: String?

    public override init() {
        super.init()
        central = CBCentralManager(delegate: self, queue: queue)
    }

    /// Scans for a dongle, connects and prepares both characteristics.
    public func open(timeout: TimeInterval = 20) throws {
        let deadline = Date().addingTimeInterval(timeout)
        cond.lock()
        defer { cond.unlock() }
        while openResult == nil {
            if !cond.wait(until: deadline) {
                let where_ = stage
                queue.async { self.teardown() }
                throw OpenError.timeout(where_)
            }
        }
        try openResult!.get()
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
        if let p = peripheral { central.cancelPeripheralConnection(p) }
        markClosed()
    }

    private func markClosed() {
        cond.lock()
        closed = true
        cond.broadcast()
        cond.unlock()
    }

    /// One read per doorbell, one at a time.
    private func pump() {
        guard !reading, pendingBells > 0, let p = peripheral, let c = dataChar else { return }
        pendingBells -= 1
        reading = true
        p.readValue(for: c)
    }
}

extension CoreBluetoothTransport: CBCentralManagerDelegate {
    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        switch central.state {
        case .poweredOn:
            setStage("scanning for the dongle")
            central.scanForPeripherals(withServices: [Self.service])
        case .unknown, .resetting:
            break
        case .unauthorized:
            finishOpen(.failure(.bluetoothUnavailable("permission denied")))
        case .poweredOff:
            finishOpen(.failure(.bluetoothUnavailable("powered off")))
            markClosed()
        default:
            finishOpen(.failure(.bluetoothUnavailable("unsupported")))
        }
    }

    public func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral,
                               advertisementData: [String: Any], rssi RSSI: NSNumber) {
        guard self.peripheral == nil else { return }
        self.peripheral = peripheral
        peripheralName = advertisementData[CBAdvertisementDataLocalNameKey] as? String ?? peripheral.name
        central.stopScan()
        setStage("connecting")
        peripheral.delegate = self
        central.connect(peripheral)
    }

    public func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        setStage("discovering services")
        peripheral.discoverServices([Self.service])
    }

    public func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        finishOpen(.failure(.failed("connect failed: \(error?.localizedDescription ?? "unknown")")))
        markClosed()
    }

    public func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        finishOpen(.failure(.failed("disconnected: \(error?.localizedDescription ?? "closed")")))
        markClosed()
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
