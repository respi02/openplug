#if DEBUG
import Foundation
import OpenPlugBLE

/// Development transport: reaches `cargo run --example fake_dongle` over TCP — on this
/// machine (Mac app, iOS simulator) or, from an iPhone, on a computer on the same
/// network. Debug builds only.
final class TcpTransport: SessionTransport, @unchecked Sendable {
    private let host: String
    private let port: UInt16
    private var fd: Int32 = -1

    init(host: String, port: UInt16) {
        self.host = host
        self.port = port
    }

    var displayName: String { "Fake dongle" }

    func open() throws {
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = port.bigEndian
        guard inet_pton(AF_INET, host, &addr.sin_addr) == 1 else {
            throw TransportError.Io(message: "bad address \(host)")
        }
        let s = socket(AF_INET, SOCK_STREAM, 0)
        var on: Int32 = 1
        setsockopt(s, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        let rc = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(s, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard rc == 0 else {
            let reason = String(cString: strerror(errno))
            Darwin.close(s)
            throw TransportError.Io(message: "fake dongle at \(host):\(port): \(reason)")
        }
        fd = s
    }

    func close() {
        if fd >= 0 { shutdown(fd, SHUT_RDWR) }
    }

    func send(data: Data) throws {
        var sent = 0
        while sent < data.count {
            let n = data.withUnsafeBytes { Darwin.send(fd, $0.baseAddress! + sent, data.count - sent, 0) }
            guard n > 0 else { throw TransportError.Io(message: "write failed") }
            sent += n
        }
    }

    func recv(maxLen: UInt32) throws -> Data {
        var buf = [UInt8](repeating: 0, count: Int(maxLen))
        let n = Darwin.recv(fd, &buf, buf.count, 0)
        return n > 0 ? Data(buf[..<n]) : Data() // empty = closed
    }
}
#endif
