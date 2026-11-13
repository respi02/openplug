import Foundation
import OpenPlugBLE

/// What the connection is doing, as the UI shows it.
enum LinkState: Equatable {
    case idle
    case connecting(String)
    case connected(String)
    /// The link dropped (or the dongle was not found); a retry is scheduled.
    case waiting(String)
    case failed(String)
}

/// Where to connect. `demo` reaches the core's `fake_dongle` example over TCP and
/// exists in debug builds only. Its host is 127.0.0.1 on a Mac or in the simulator; on
/// a real iPhone it has to be the address of the computer running `fake_dongle`.
enum Target: Hashable {
    case bluetooth
    #if DEBUG
    case demo(host: String)
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

/// What the trip journal currently holds.
struct JournalSnapshot: Equatable {
    var active: TripRecord?
    var trips: [TripRecord] = []
    var refuels: [RefuelRecord] = []
    var totals = JournalTotals(trips: 0, distanceKm: 0, movingS: 0, fuelUsedL: 0)
}

/// Shared between the UI and the session thread: lets the user stop a session that
/// is blocked in the core or sleeping before a retry.
private final class SessionControl: @unchecked Sendable {
    private let lock = NSLock()
    private var stopped = false
    private var transport: SessionTransport?

    var isStopped: Bool {
        lock.lock(); defer { lock.unlock() }
        return stopped
    }

    /// Registers the transport in use; false if the session was stopped meanwhile.
    func adopt(_ t: SessionTransport) -> Bool {
        lock.lock(); defer { lock.unlock() }
        transport = stopped ? nil : t
        return !stopped
    }

    func stop() {
        lock.lock()
        stopped = true
        let t = transport
        lock.unlock()
        t?.close() // unblocks the session thread
    }
}

private func nowMs() -> UInt64 { UInt64(Date().timeIntervalSince1970 * 1000) }

/// Owns the session. Every core call blocks, so the session lives on its own thread
/// and publishes to the main actor. Once started it keeps the link up: when the
/// dongle goes away (ignition off, out of range) it retries until stopped.
@MainActor
final class DongleModel: ObservableObject {
    @Published private(set) var state: LinkState = .idle
    @Published private(set) var readings: [Reading] = []
    @Published private(set) var journal = JournalSnapshot()
    /// Set if the trip log on disk could not be opened; trips are then not recorded.
    @Published private(set) var journalProblem: String?

    private let trips: TripJournal?
    private var control: SessionControl?

    init() {
        do {
            let journal = try TripJournal.open(path: try Self.journalPath())
            _ = journal.tick(nowMs: nowMs()) // close a trip left paused by an earlier run
            trips = journal
            self.journal = Self.snapshot(journal)
        } catch JournalError.Unreadable(let message) {
            trips = nil
            journalProblem = message
        } catch {
            trips = nil
            journalProblem = error.localizedDescription
        }
    }

    private static func journalPath() throws -> String {
        #if DEBUG
        if let override = ProcessInfo.processInfo.environment["OPENPLUG_JOURNAL"] { return override }
        #endif
        let dir = try FileManager.default
            .url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appendingPathComponent("OpenPlug", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("journal.json").path
    }

    var isActive: Bool { control != nil }

    func connect(to target: Target, pin: String) {
        guard control == nil else { return }
        let control = SessionControl()
        self.control = control
        readings = []
        state = .connecting("Looking for the dongle…")
        let trips = self.trips

        Thread.detachNewThread { [weak self] in
            let end = Self.session(target: target, pin: pin, control: control, trips: trips) { update in
                Task { @MainActor in self?.apply(update) }
            }
            Task { @MainActor in self?.finish(end, control) }
        }
    }

    /// Stops the session for good (no more retries).
    func disconnect() {
        control?.stop()
    }

    private enum Update {
        case state(LinkState)
        case readings([Reading])
        case journal(JournalSnapshot)
    }

    private func apply(_ update: Update) {
        switch update {
        case .state(let s): state = s
        case .readings(let r): readings = r
        case .journal(let j): journal = j
        }
        #if DEBUG
        trace()
        #endif
    }

    private func finish(_ end: LinkState, _ finished: SessionControl) {
        guard control === finished else { return }
        control = nil
        state = end
        if let trips { journal = Self.snapshot(trips) }
        #if DEBUG
        trace()
        #endif
    }

    private nonisolated static func snapshot(_ j: TripJournal) -> JournalSnapshot {
        JournalSnapshot(active: j.activeTrip(), trips: j.trips(), refuels: j.refuels(), totals: j.totals())
    }

    // MARK: the session thread

    /// Connect, stream, and on loss wait and try again, until stopped or the PIN is
    /// rejected. Returns the state to end in.
    private nonisolated static func session(
        target: Target, pin: String, control: SessionControl, trips: TripJournal?,
        publish: @escaping (Update) -> Void
    ) -> LinkState {
        var failures = 0
        while true {
            let transport: SessionTransport
            switch target {
            case .bluetooth: transport = CoreBluetoothTransport()
            #if DEBUG
            case .demo(let host): transport = TcpTransport(host: host, port: 4455)
            #endif
            }
            guard control.adopt(transport) else { transport.close(); return .idle }

            var wasConnected = false
            let problem: String
            do {
                try stream(transport, pin: pin, trips: trips, publish: publish) { wasConnected = true }
                problem = "The connection ended."
            } catch DongleError.WrongPin {
                transport.close()
                return .failed("The dongle rejected the PIN.")
            } catch DongleError.Closed {
                problem = "The dongle closed the connection."
            } catch let e as CoreBluetoothTransport.OpenError {
                if case .bluetoothUnavailable = e {
                    transport.close()
                    return .failed(sentence(e.description))
                }
                problem = wasConnected ? "Connection lost." : "Dongle not found."
            } catch TransportError.Io(let message) {
                problem = wasConnected ? "Connection lost." : sentence(message)
            } catch {
                problem = wasConnected ? "Connection lost." : sentence(error.localizedDescription)
            }
            transport.close()
            if wasConnected {
                failures = 0
                trips?.onDisconnected(nowMs: nowMs())
            }
            if let trips { publish(.journal(snapshot(trips))) }
            if control.isStopped { return .idle }

            // Back off: 5 s, 10 s, 20 s, then every 30 s.
            failures += 1
            let delay = min(30, 5 * (1 << min(failures - 1, 3)))
            for left in stride(from: delay, to: 0, by: -1) {
                if control.isStopped { return .idle }
                publish(.state(.waiting("\(problem) Trying again in \(left) s…")))
                Thread.sleep(forTimeInterval: 1)
                if let trips, trips.tick(nowMs: nowMs()) != nil { publish(.journal(snapshot(trips))) }
            }
            publish(.state(.connecting("Looking for the dongle…")))
        }
    }

    /// One connection: authenticate, subscribe, pump readings until the link ends.
    private nonisolated static func stream(
        _ transport: SessionTransport, pin: String, trips: TripJournal?,
        publish: @escaping (Update) -> Void, onConnected: () -> Void
    ) throws {
        try transport.open()
        publish(.state(.connecting("Authenticating…")))
        let dongle = try Dongle.connect(transport: transport, pin: pin)
        try dongle.subscribeDefaults()
        onConnected()
        trips?.onConnected(nowMs: nowMs())
        publish(.state(.connected(transport.displayName)))

        var latest: [UInt32: Reading] = [:]
        while true {
            let batch = try dongle.nextReadings()
            for r in batch { latest[r.url] = r }
            // labelled values first, then by id
            let sorted = latest.values.sorted {
                ($0.name == nil ? 1 : 0, $0.url) < ($1.name == nil ? 1 : 0, $1.url)
            }
            publish(.readings(sorted))
            if let trips {
                trips.onReadings(readings: batch, nowMs: nowMs())
                publish(.journal(snapshot(trips)))
            }
        }
    }

    private nonisolated static func sentence(_ s: String) -> String {
        let t = s.prefix(1).uppercased() + s.dropFirst()
        return t.hasSuffix(".") ? t : t + "."
    }

    // MARK: development aid

    #if DEBUG
    /// `OpenPlug --autoconnect bluetooth|demo` with the PIN in OPENPLUG_PIN connects
    /// on launch and traces the session to stdout.
    func autoconnectIfRequested() {
        let args = ProcessInfo.processInfo.arguments
        guard let i = args.firstIndex(of: "--autoconnect"), args.indices.contains(i + 1),
              let pin = ProcessInfo.processInfo.environment["OPENPLUG_PIN"] else { return }
        tracing = true
        connect(to: args[i + 1] == "demo" ? .demo(host: "127.0.0.1") : .bluetooth, pin: pin)
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
        if let a = journal.active {
            print(String(format: "trip: active %.3f km, %d s moving; finished trips: %d", a.distanceKm, a.movingS, journal.trips.count))
        }
        fflush(stdout)
    }
    #endif
}
