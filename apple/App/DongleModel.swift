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
    /// Starts connecting without waiting (no-op where there is nothing to start).
    func begin()
    func open() throws
    func close()
    var displayName: String { get }
}

extension SessionTransport {
    func begin() {}
}

extension CoreBluetoothTransport: SessionTransport {
    /// Waits for the dongle as long as it takes: a pending connect, completed by iOS
    /// even while the app is suspended.
    func open() throws { try open(timeout: nil) }
    var displayName: String { peripheralName ?? "DataPlug" }
}

extension Target {
    /// How a wanted session is stored, to resume it after a relaunch.
    var stored: String {
        switch self {
        case .bluetooth: return "bluetooth"
        #if DEBUG
        case .demo(let host): return "demo:" + host
        #endif
        }
    }

    init?(stored: String) {
        if stored == "bluetooth" { self = .bluetooth; return }
        #if DEBUG
        if stored.hasPrefix("demo:") { self = .demo(host: String(stored.dropFirst(5))); return }
        #endif
        return nil
    }
}

/// What the trip journal currently holds.
struct JournalSnapshot: Equatable {
    var active: TripRecord?
    var trips: [TripRecord] = []
    var refuels: [RefuelRecord] = []
    var totals = JournalTotals(trips: 0, distanceKm: 0, movingS: 0, fuelUsedL: 0)
    /// The last known value of every data object, for showing the car while it is away.
    var last: [LastReading] = []
    /// True while a trip is being recorded right now.
    var tripRunning = false
    /// Where the car was when the link last dropped.
    var parked: ParkedAt?
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

    /// The file the current session records its values to, if recording is on.
    @Published private(set) var recording: String?

    private var trips: TripJournal?
    /// Whether `trips` is the fake dongle's journal (debug builds keep it apart, so
    /// synthetic values never end up in the car's trip log).
    private var tripsAreDemo = false
    private var control: SessionControl?

    /// The one model of the app. Created at launch (see `OpenPlugApp`), also when iOS
    /// relaunches the app in the background to hand back a dongle connection.
    static let shared = DongleModel()

    private init() {
        #if os(iOS)
        DongleCentral.shared.activate() // so iOS can restore its connections
        #endif
        Keychain.upgradeAccessibility()
        setUpLocation()
        openJournal(demo: false)
        resumeIfWanted()
    }

    // MARK: the wanted session, kept across launches

    private static let wantedKey = "sessionTarget"

    /// The session the user started and did not stop; nil when none.
    private var wantedTarget: Target? {
        get { UserDefaults.standard.string(forKey: Self.wantedKey).flatMap(Target.init(stored:)) }
        set { UserDefaults.standard.set(newValue?.stored, forKey: Self.wantedKey) }
    }

    /// Picks up the session the user left running, e.g. after iOS ended the app while
    /// it waited for the car and relaunched it when the dongle connected.
    private func resumeIfWanted() {
        #if DEBUG
        if ProcessInfo.processInfo.arguments.contains("--autoconnect") { return } // scripted run decides
        #endif
        guard let target = wantedTarget else { return }
        guard let pin = Keychain.pin, !pin.isEmpty else { return } // not readable before the first unlock
        connect(to: target, pin: pin, record: UserDefaults.standard.bool(forKey: "recordValues"))
    }

    /// Opens the car's journal, or the fake dongle's; a no-op if it is already open.
    private func openJournal(demo: Bool) {
        if trips != nil && tripsAreDemo == demo { return }
        do {
            let journal = try TripJournal.open(path: try Self.journalPath(demo: demo))
            _ = journal.tick(nowMs: nowMs()) // close a trip left paused by an earlier run
            trips = journal
            tripsAreDemo = demo
            journalProblem = nil
            self.journal = Self.snapshot(journal)
        } catch JournalError.Unreadable(let message) {
            trips = nil
            journalProblem = message
        } catch {
            trips = nil
            journalProblem = error.localizedDescription
        }
    }

    private static func journalPath(demo: Bool) throws -> String {
        #if DEBUG
        if let override = ProcessInfo.processInfo.environment["OPENPLUG_JOURNAL"] { return override }
        #endif
        let dir = try FileManager.default
            .url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appendingPathComponent("OpenPlug", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent(demo ? "journal-demo.json" : "journal.json").path
    }

    /// Where recordings go: visible in the Files app on iOS (On My iPhone › OpenPlug),
    /// in ~/Documents/OpenPlug Recordings on a Mac.
    static func recordingsDirectory() throws -> URL {
        #if DEBUG
        if let override = ProcessInfo.processInfo.environment["OPENPLUG_RECORDINGS"] {
            return URL(fileURLWithPath: override, isDirectory: true)
        }
        #endif
        let docs = try FileManager.default
            .url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        #if os(macOS)
        return docs.appendingPathComponent("OpenPlug Recordings", isDirectory: true)
        #else
        return docs.appendingPathComponent("Recordings", isDirectory: true)
        #endif
    }

    /// A recorder writing to a new file, or nil if it cannot be created (the session
    /// then runs without recording rather than not at all).
    private static func makeRecorder() -> (ValueRecorder, String)? {
        let format = DateFormatter()
        format.locale = Locale(identifier: "en_US_POSIX")
        format.dateFormat = "yyyyMMdd-HHmmss"
        let stamp = format.string(from: Date())
        guard let dir = try? recordingsDirectory() else { return nil }
        let path = dir.appendingPathComponent("drive-\(stamp).jsonl").path
        guard let recorder = try? ValueRecorder.open(path: path, nowMs: nowMs()) else { return nil }
        return (recorder, path)
    }

    var isActive: Bool { control != nil }

    /// Whether a PIN has been entered (the app is set up).
    var hasPin: Bool { !(Keychain.pin ?? "").isEmpty }

    // MARK: routes (GPS)

    private let location = LocationFeed()

    /// Whether the user wants trips to carry a route. Off until switched on in Settings.
    @Published var recordsRoutes = UserDefaults.standard.bool(forKey: "recordRoutes") {
        didSet {
            UserDefaults.standard.set(recordsRoutes, forKey: "recordRoutes")
            if recordsRoutes && !location.isAuthorized { location.requestPermission() }
            syncLocation()
        }
    }

    /// What the system currently allows, for the note under the switch.
    var routePermission: String {
        switch location.status {
        case .notDetermined: return "OpenPlug will ask for your location."
        case .denied, .restricted: return "Location is off for OpenPlug — allow it in the system settings."
        default:
            return location.worksInBackground
                ? "Recorded while the car is connected."
                : "Allowed only while the app is in use. For drives recorded without opening the app, set Location to Always in the system settings."
        }
    }

    private func setUpLocation() {
        location.onFix = { [weak self] lat, lon in
            self?.trips?.onPosition(lat: lat, lon: lon, nowMs: nowMs())
        }
        location.onAuthorizationChange = { [weak self] in
            self?.objectWillChange.send()
            self?.syncLocation()
        }
    }

    /// Positions are taken only while the car is connected.
    private func syncLocation() {
        if recordsRoutes, case .connected = state { location.start() } else { location.stop() }
    }

    /// The route of a trip; empty if none was recorded.
    func route(of trip: TripRecord) -> [RoutePoint] { trips?.track(startMs: trip.startMs) ?? [] }

    /// The finished trips as CSV text, or nil when there is no trip log.
    func exportCsv() -> String? { trips?.exportCsv() }

    /// How many value recordings are stored.
    func recordingCount() -> Int {
        guard let dir = try? Self.recordingsDirectory() else { return 0 }
        return (try? FileManager.default.contentsOfDirectory(atPath: dir.path).count) ?? 0
    }

    /// Starts a session. With `record`, the value stream of the whole session is also
    /// written to a new file, to replay the drive later (`fake_dongle --replay`, the
    /// `replay` example).
    func connect(to target: Target, pin: String, record: Bool = false) {
        guard control == nil else { return }
        wantedTarget = target
        let control = SessionControl()
        self.control = control
        readings = []
        state = .connecting("Looking for the dongle…")
        #if DEBUG
        if case .demo = target { openJournal(demo: true) } else { openJournal(demo: false) }
        #endif
        let trips = self.trips
        let made = record ? Self.makeRecorder() : nil
        let recorder = made?.0
        recording = made?.1

        Thread.detachNewThread { [weak self] in
            let end = Self.session(target: target, pin: pin, control: control, trips: trips, recorder: recorder) { update in
                Task { @MainActor in self?.apply(update) }
            }
            Task { @MainActor in self?.finish(end, control) }
        }
    }

    /// Stops the session for good (no more retries, nothing resumes it).
    func disconnect() {
        wantedTarget = nil
        control?.stop()
    }

    private enum Update {
        case state(LinkState)
        case readings([Reading])
        case journal(JournalSnapshot)
    }

    private func apply(_ update: Update) {
        switch update {
        case .state(let s):
            state = s
            syncLocation()
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
        recording = nil
        state = end
        syncLocation()
        if case .failed = end { wantedTarget = nil } // retrying cannot help
        if let trips { journal = Self.snapshot(trips) }
        #if DEBUG
        trace()
        #endif
    }

    private nonisolated static func snapshot(_ j: TripJournal) -> JournalSnapshot {
        JournalSnapshot(active: j.activeTrip(), trips: j.trips(), refuels: j.refuels(), totals: j.totals(),
                        last: j.lastValues(), tripRunning: j.tripRunning(), parked: j.parked())
    }

    // MARK: the session thread

    /// Connect, stream, and on loss wait and try again, until stopped or the PIN is
    /// rejected. Returns the state to end in.
    private nonisolated static func session(
        target: Target, pin: String, control: SessionControl, trips: TripJournal?,
        recorder: ValueRecorder?, publish: @escaping (Update) -> Void
    ) -> LinkState {
        func makeTransport() -> SessionTransport {
            switch target {
            case .bluetooth: return CoreBluetoothTransport(waitForBluetooth: true)
            #if DEBUG
            case .demo(let host): return TcpTransport(host: host, port: 4455)
            #endif
            }
        }
        let bluetooth = target == .bluetooth
        var failures = 0
        var transport = makeTransport()
        transport.begin()
        while true {
            guard control.adopt(transport) else { transport.close(); return .idle }

            var wasConnected = false
            let problem: String
            do {
                try stream(transport, pin: pin, trips: trips, recorder: recorder, publish: publish) {
                    wasConnected = true
                }
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
                recorder?.onDisconnected(nowMs: nowMs())
            }
            if let trips { publish(.journal(snapshot(trips))) }
            if control.isStopped { return .idle }

            // Look again at once: over BLE the pending connect placed by begin() is what
            // lets iOS reconnect while the app is suspended, so it must not wait.
            transport = makeTransport()
            transport.begin()

            // Back off: 5 s, 10 s, 20 s, then every 30 s. A sleeping thread freezes
            // while iOS suspends the app, so over BLE (where open() already waits for the
            // dongle) only repeated failures that never got values back off.
            failures += 1
            let delay = bluetooth && failures < 3 ? 0 : min(30, 5 * (1 << min(failures - 1, 3)))
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
        _ transport: SessionTransport, pin: String, trips: TripJournal?, recorder: ValueRecorder?,
        publish: @escaping (Update) -> Void, onConnected: () -> Void
    ) throws {
        try transport.open()
        publish(.state(.connecting("Authenticating…")))
        let dongle = try Dongle.connect(transport: transport, pin: pin)
        try dongle.subscribeDefaults()
        onConnected()
        trips?.onConnected(nowMs: nowMs())
        recorder?.onConnected(nowMs: nowMs())
        publish(.state(.connected(transport.displayName)))

        var latest: [UInt32: Reading] = [:]
        while true {
            let batch = try dongle.nextReadings()
            recorder?.onReadings(readings: batch, nowMs: nowMs())
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
    /// `OpenPlug --autoconnect bluetooth|demo [--record]` with the PIN in OPENPLUG_PIN
    /// connects on launch and traces the session to stdout.
    func autoconnectIfRequested() {
        let args = ProcessInfo.processInfo.arguments
        guard let i = args.firstIndex(of: "--autoconnect"), args.indices.contains(i + 1),
              let pin = ProcessInfo.processInfo.environment["OPENPLUG_PIN"] else { return }
        tracing = true
        connect(to: args[i + 1] == "demo" ? .demo(host: "127.0.0.1") : .bluetooth, pin: pin,
                record: args.contains("--record"))
        if let recording { print("recording to \(recording)") }
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
