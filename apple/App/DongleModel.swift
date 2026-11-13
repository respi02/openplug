import Foundation
import OpenPlugBLE

/// What the connection is doing, as the UI shows it.
enum LinkState: Equatable {
    case idle
    case connecting(String)
    case connected(String)
    case failed(String)
}

/// Where to connect. `demo` reaches the core's `fake_dongle` example on this machine
/// and exists in debug builds only.
enum Target: Hashable {
    case bluetooth
    #if DEBUG
    case demo
    #endif
}

/// A transport the model can open and tear down.
protocol SessionTransport: ByteTransport {
    func open() throws
    func close()
    var displayName: String { get }
}

extension CoreBluetoothTransport: SessionTransport {
    func open() throws { try open(timeout: 20) }
    var displayName: String { peripheralName ?? "DataPlug" }
}

/// Owns the session: every core call blocks, so it lives on its own thread and
/// publishes to the main actor.
@MainActor
final class DongleModel: ObservableObject {
    @Published private(set) var state: LinkState = .idle
    @Published private(set) var readings: [Reading] = []

    private var transport: SessionTransport?
    private var userClosed = false

    var isBusy: Bool {
        switch state {
        case .connecting, .connected: return true
        default: return false
        }
    }

    func connect(to target: Target, pin: String) {
        guard !isBusy else { return }
        let transport: SessionTransport
        switch target {
        case .bluetooth: transport = CoreBluetoothTransport()
        #if DEBUG
        case .demo: transport = TcpTransport(host: "127.0.0.1", port: 4455)
        #endif
        }
        self.transport = transport
        userClosed = false
        readings = []
        state = .connecting("Looking for the dongle…")

        Thread.detachNewThread { [weak self] in
            let end = Self.run(transport, pin: pin) { update in
                Task { @MainActor in self?.apply(update) }
            }
            transport.close()
            Task { @MainActor in self?.finish(end) }
        }
    }

    /// Closing the transport unblocks the session thread, which then ends.
    func disconnect() {
        userClosed = true
        transport?.close()
    }

    private enum Update {
        case state(LinkState)
        case readings([Reading])
    }

    private func apply(_ update: Update) {
        switch update {
        case .state(let s): state = s
        case .readings(let r): readings = r
        }
        #if DEBUG
        trace()
        #endif
    }

    #if DEBUG
    /// Development aid: `OpenPlug --autoconnect bluetooth|demo` with the PIN in
    /// OPENPLUG_PIN connects on launch and traces the session to stdout.
    func autoconnectIfRequested() {
        let args = ProcessInfo.processInfo.arguments
        guard let i = args.firstIndex(of: "--autoconnect"), args.indices.contains(i + 1),
              let pin = ProcessInfo.processInfo.environment["OPENPLUG_PIN"] else { return }
        tracing = true
        connect(to: args[i + 1] == "demo" ? .demo : .bluetooth, pin: pin)
    }

    private var tracing = false
    private var traced: LinkState?

    private func trace() {
        guard tracing else { return }
        if traced != state {
            traced = state
            print("state: \(state)")
        }
        if !readings.isEmpty {
            let labelled = readings.filter { $0.name != nil && $0.available }
            print("readings: \(readings.count) objects, e.g. " + labelled.prefix(3)
                .map { "\($0.name!) = \($0.value)" }.joined(separator: ", "))
        }
        fflush(stdout)
    }
    #endif

    private func finish(_ end: LinkState) {
        state = userClosed ? .idle : end
        transport = nil
        #if DEBUG
        trace()
        #endif
    }

    /// The whole session, on a background thread. Returns the state to end in.
    private nonisolated static func run(
        _ transport: SessionTransport, pin: String, publish: @escaping (Update) -> Void
    ) -> LinkState {
        do {
            try transport.open()
            publish(.state(.connecting("Authenticating…")))
            let dongle = try Dongle.connect(transport: transport, pin: pin)
            try dongle.subscribeDefaults()
            publish(.state(.connected(transport.displayName)))

            var latest: [UInt32: Reading] = [:]
            while true {
                for r in try dongle.nextReadings() { latest[r.url] = r }
                // labelled values first, then by id
                let sorted = latest.values.sorted {
                    ($0.name == nil ? 1 : 0, $0.url) < ($1.name == nil ? 1 : 0, $1.url)
                }
                publish(.readings(sorted))
            }
        } catch DongleError.WrongPin {
            return .failed("The dongle rejected the PIN.")
        } catch DongleError.Closed {
            return .failed("The dongle closed the connection.")
        } catch let e as CoreBluetoothTransport.OpenError {
            return .failed(e.description.prefix(1).uppercased() + e.description.dropFirst() + ".")
        } catch {
            return .failed(error.localizedDescription)
        }
    }
}
