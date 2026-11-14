// A BLE stand-in for the dongle, for testing the iOS app without the car.
//
//   (cd core && cargo run --example fake_dongle)          # the dongle's TLS end
//   swift run openplug-fake-ble [--seconds N] [--tcp 127.0.0.1:4455]
//
// The Mac advertises the dongle's service and serves its two characteristics the way
// the dongle does (research/notes/ble_transport.md): what the phone writes to the
// data characteristic goes to fake_dongle over TCP; what comes back is queued in
// chunks of at most 512 bytes, each announced by a one-byte 0xff notification on the
// doorbell characteristic and fetched by a read of the data characteristic.
// Stopping it (Ctrl-C, or --seconds) looks to the phone like the ignition going off.
// PIN as configured in fake_dongle (default 1234).

import CoreBluetooth
import Foundation

let args = CommandLine.arguments
func opt(_ name: String) -> String? {
    args.firstIndex(of: name).flatMap { args.indices.contains($0 + 1) ? args[$0 + 1] : nil }
}

let serviceUUID = CBUUID(string: "732DB477-BFD6-4694-9378-86C246F35DEF")
let dataUUID = CBUUID(string: "478A86F3-E47C-4618-9B15-3C7C92CC8184")
let doorbellUUID = CBUUID(string: "2EFB2DD4-CDC3-11E6-9D9D-CEC0C932CE01")

func log(_ s: String) {
    let t = Date().formatted(date: .omitted, time: .standard)
    print("[\(t)] \(s)")
    fflush(stdout)
}

/// A TCP connection to fake_dongle, read on a background thread.
final class Upstream {
    private let fd: Int32
    init?(host: String, port: UInt16, onData: @escaping (Data) -> Void, onClose: @escaping () -> Void) {
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = port.bigEndian
        guard inet_pton(AF_INET, host, &addr.sin_addr) == 1 else { return nil }
        let s = socket(AF_INET, SOCK_STREAM, 0)
        var on: Int32 = 1
        setsockopt(s, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        let rc = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(s, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard rc == 0 else { close(s); return nil }
        fd = s
        Thread.detachNewThread {
            var buf = [UInt8](repeating: 0, count: 4096)
            while true {
                let n = recv(s, &buf, buf.count, 0)
                if n <= 0 { break }
                onData(Data(buf[..<n]))
            }
            onClose()
        }
    }

    func send(_ data: Data) {
        data.withUnsafeBytes { _ = Darwin.send(fd, $0.baseAddress!, data.count, 0) }
    }

    func shutdown() { Darwin.shutdown(fd, SHUT_RDWR) }
}

final class FakeDongle: NSObject, CBPeripheralManagerDelegate {
    private var manager: CBPeripheralManager!
    private let host: String
    private let port: UInt16
    private var dataChar: CBMutableCharacteristic!
    private var bell: CBMutableCharacteristic!

    // On the main queue.
    private var upstream: Upstream?
    private var chunks: [Data] = []
    private var current = Data()
    private var pendingBells = 0
    private var subscribed = false

    init(host: String, port: UInt16) {
        self.host = host
        self.port = port
        super.init()
        manager = CBPeripheralManager(delegate: self, queue: .main)
    }

    func stop() {
        manager.stopAdvertising()
        manager.removeAllServices()
        upstream?.shutdown()
    }

    func peripheralManagerDidUpdateState(_ p: CBPeripheralManager) {
        guard p.state == .poweredOn else { return log("Bluetooth state \(p.state.rawValue)") }
        dataChar = CBMutableCharacteristic(type: dataUUID, properties: [.read, .write], value: nil,
                                           permissions: [.readable, .writeable])
        bell = CBMutableCharacteristic(type: doorbellUUID, properties: [.notify], value: nil, permissions: [])
        let service = CBMutableService(type: serviceUUID, primary: true)
        service.characteristics = [dataChar, bell]
        p.add(service)
    }

    func peripheralManager(_ p: CBPeripheralManager, didAdd service: CBService, error: Error?) {
        p.startAdvertising([CBAdvertisementDataServiceUUIDsKey: [serviceUUID],
                            CBAdvertisementDataLocalNameKey: "OpenPlugFake"])
    }

    func peripheralManagerDidStartAdvertising(_ p: CBPeripheralManager, error: Error?) {
        log("advertising the dongle service" + (error.map { " — error \($0)" } ?? ""))
    }

    func peripheralManager(_ p: CBPeripheralManager, central: CBCentral, didSubscribeTo characteristic: CBCharacteristic) {
        log("phone subscribed (max update \(central.maximumUpdateValueLength) B); new session")
        subscribed = true
        chunks = []
        current = Data()
        pendingBells = 0
        upstream?.shutdown()
        upstream = Upstream(host: host, port: port, onData: { data in
            DispatchQueue.main.async { self.queue(data) }
        }, onClose: {
            DispatchQueue.main.async { log("fake_dongle closed the connection") }
        })
        if upstream == nil { log("cannot reach fake_dongle at \(host):\(port)") }
    }

    func peripheralManager(_ p: CBPeripheralManager, central: CBCentral, didUnsubscribeFrom characteristic: CBCharacteristic) {
        log("phone unsubscribed")
        subscribed = false
        upstream?.shutdown()
        upstream = nil
    }

    private func queue(_ data: Data) {
        var rest = data
        while !rest.isEmpty {
            chunks.append(rest.prefix(512))
            rest = rest.dropFirst(512)
            pendingBells += 1
        }
        ring()
    }

    private func ring() {
        while pendingBells > 0 && subscribed {
            guard manager.updateValue(Data([0xff]), for: bell, onSubscribedCentrals: nil) else { return } // retried below
            pendingBells -= 1
        }
    }

    func peripheralManagerIsReady(toUpdateSubscribers p: CBPeripheralManager) {
        ring()
    }

    func peripheralManager(_ p: CBPeripheralManager, didReceiveWrite requests: [CBATTRequest]) {
        var data = Data()
        for r in requests.sorted(by: { $0.offset < $1.offset }) {
            if let v = r.value { data.append(v) }
        }
        upstream?.send(data)
        p.respond(to: requests[0], withResult: .success)
    }

    func peripheralManager(_ p: CBPeripheralManager, didReceiveRead request: CBATTRequest) {
        if request.offset == 0 {
            current = chunks.isEmpty ? Data() : chunks.removeFirst()
        }
        guard request.offset <= current.count else {
            return p.respond(to: request, withResult: .invalidOffset)
        }
        request.value = current.subdata(in: request.offset..<current.count)
        p.respond(to: request, withResult: .success)
    }
}

let tcp = (opt("--tcp") ?? "127.0.0.1:4455").split(separator: ":")
let fake = FakeDongle(host: String(tcp[0]), port: UInt16(tcp.count > 1 ? String(tcp[1]) : "4455") ?? 4455)
signal(SIGINT, SIG_IGN)
let sigint = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
sigint.setEventHandler { fake.stop(); log("stopped"); exit(0) }
sigint.resume()
if let s = opt("--seconds").flatMap(Double.init) {
    DispatchQueue.main.asyncAfter(deadline: .now() + s) { fake.stop(); log("stopped after \(Int(s)) s"); exit(0) }
}
RunLoop.main.run()
