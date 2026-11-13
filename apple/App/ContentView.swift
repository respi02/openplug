import OpenPlugBLE
import SwiftUI

struct ContentView: View {
    // `--trips` opens on the Trips tab (handy for screenshots and scripted checks)
    @State private var tab = CommandLine.arguments.contains("--trips") ? 1 : 0

    var body: some View {
        TabView(selection: $tab) {
            NavigationStack { LiveTab() }
                .tabItem { Label("Live", systemImage: "gauge.with.dots.needle.50percent") }
                .tag(0)
            NavigationStack { TripsTab() }
                .tabItem { Label("Trips", systemImage: "road.lanes") }
                .tag(1)
        }
    }
}

// MARK: - Live values

private struct LiveTab: View {
    @EnvironmentObject private var model: DongleModel

    var body: some View {
        Group {
            if case .connected(let name) = model.state {
                LiveView().navigationTitle(name)
            } else {
                SetupView().navigationTitle("OpenPlug")
            }
        }
        .toolbar {
            if model.isActive {
                ToolbarItem(placement: .primaryAction) {
                    Button("Disconnect") { model.disconnect() }
                }
            }
        }
    }
}

private struct SetupView: View {
    @EnvironmentObject private var model: DongleModel
    @State private var pin = Keychain.pin ?? ""
    #if DEBUG
    @State private var useDemo = false
    @AppStorage("demoHost") private var demoHost = "127.0.0.1"
    #endif

    private var target: Target {
        #if DEBUG
        if useDemo { return .demo(host: demoHost.trimmingCharacters(in: .whitespaces)) }
        #endif
        return .bluetooth
    }

    var body: some View {
        Form {
            #if DEBUG
            Section {
                Picker("Connect to", selection: $useDemo) {
                    Text("Dongle (Bluetooth)").tag(false)
                    Text("Fake dongle (TCP, port 4455)").tag(true)
                }
                if useDemo {
                    TextField("Host running fake_dongle", text: $demoHost)
                        .autocorrectionDisabled()
                    #if os(iOS)
                        .textInputAutocapitalization(.never)
                        .keyboardType(.numbersAndPunctuation)
                    #endif
                }
            } footer: {
                if useDemo {
                    Text("127.0.0.1 works on a Mac and in the simulator. On an iPhone enter the address of the computer running fake_dongle --listen 0.0.0.0:4455, on the same network.")
                }
            }
            .disabled(model.isActive)
            #endif
            Section {
                SecureField("PIN from the dongle label", text: $pin)
                    .autocorrectionDisabled()
                #if os(iOS)
                    .textInputAutocapitalization(.characters)
                #endif
            } footer: {
                Text("The PIN is printed on the dongle. It stays on this device, in the Keychain.")
            }
            .disabled(model.isActive)
            Section {
                if !model.isActive {
                    Button("Connect") {
                        let trimmed = pin.trimmingCharacters(in: .whitespacesAndNewlines)
                        Keychain.pin = trimmed
                        model.connect(to: target, pin: trimmed)
                    }
                    .disabled(pin.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                switch model.state {
                case .connecting(let what), .waiting(let what):
                    HStack {
                        ProgressView()
                        Text(what).foregroundStyle(.secondary).padding(.leading, 8)
                    }
                case .failed(let message):
                    Text(message).foregroundStyle(.red)
                default:
                    EmptyView()
                }
            } footer: {
                if model.isActive {
                    Text("OpenPlug keeps trying until the dongle answers — it only does when the ignition is on.")
                }
            }
        }
        #if os(macOS)
        .formStyle(.grouped)
        #endif
    }
}

private struct LiveView: View {
    @EnvironmentObject private var model: DongleModel

    var body: some View {
        List {
            if model.readings.isEmpty {
                Text("Waiting for the first values…").foregroundStyle(.secondary)
            }
            ForEach(model.readings, id: \.url) { r in
                HStack {
                    Text(r.name ?? String(format: "0x%06x", r.url))
                    Spacer()
                    Text(format(r)).monospacedDigit().foregroundStyle(r.available ? .primary : .secondary)
                }
            }
        }
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
    }

    private func format(_ r: Reading) -> String {
        guard r.available else { return "—" }
        let number = r.value.formatted(.number.precision(.fractionLength(Int(r.precision))))
        return r.unit.map { "\(number) \($0)" } ?? number
    }
}

// MARK: - Trips

private struct TripsTab: View {
    @EnvironmentObject private var model: DongleModel

    var body: some View {
        let j = model.journal
        List {
            if let problem = model.journalProblem {
                Section {
                    Text("Trips are not being recorded: \(problem)").foregroundStyle(.red)
                }
            }
            if let trip = j.active {
                Section("Current trip") { TripRow(trip: trip, running: true) }
            }
            if j.totals.trips > 0 {
                Section("All trips") {
                    LabeledContent("Trips", value: "\(j.totals.trips)")
                    LabeledContent("Distance", value: km(j.totals.distanceKm))
                    LabeledContent("Time driving", value: duration(j.totals.movingS))
                    if j.totals.fuelUsedL > 0 {
                        LabeledContent("Fuel used", value: litres(j.totals.fuelUsedL))
                    }
                }
                Section("History") {
                    ForEach(j.trips, id: \.startMs) { TripRow(trip: $0, running: false) }
                }
            }
            if !j.refuels.isEmpty {
                Section("Refuels") {
                    ForEach(j.refuels, id: \.timeMs) { r in
                        LabeledContent {
                            Text("+" + litres(r.addedL)).monospacedDigit()
                        } label: {
                            Text(date(r.timeMs))
                            if let odo = r.odometerKm {
                                Text("at \(odo.formatted(.number.precision(.fractionLength(0)))) km")
                            }
                        }
                    }
                }
            }
            if j.active == nil && j.totals.trips == 0 && j.refuels.isEmpty && model.journalProblem == nil {
                Text("No trips yet. A trip starts when the car moves while OpenPlug is connected, and ends 15 minutes after the connection drops.")
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Trips")
    }
}

private struct TripRow: View {
    let trip: TripRecord
    let running: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(date(trip.startMs)).font(.headline)
                Spacer()
                Text(km(trip.distanceKm)).font(.headline).monospacedDigit()
            }
            Text(details).font(.subheadline).foregroundStyle(.secondary)
        }
        .padding(.vertical, 2)
    }

    private var details: String {
        var parts = [duration(trip.movingS) + (running ? " so far" : "")]
        if trip.movingS > 0 {
            parts.append("avg \(trip.avgSpeedKmh.formatted(.number.precision(.fractionLength(0)))) km/h")
        }
        parts.append("max \(trip.maxSpeedKmh.formatted(.number.precision(.fractionLength(0)))) km/h")
        if let c = trip.consumptionLPer100km {
            parts.append("\(c.formatted(.number.precision(.fractionLength(1))))\u{a0}l/100\u{a0}km")
        } else if let f = trip.fuelUsedL, f > 0 {
            parts.append(litres(f))
        }
        return parts.joined(separator: " · ")
    }
}

private func km(_ v: Double) -> String {
    v.formatted(.number.precision(.fractionLength(v < 100 ? 1 : 0))) + " km"
}

private func litres(_ v: Double) -> String {
    v.formatted(.number.precision(.fractionLength(1))) + " l"
}

private func duration(_ seconds: UInt64) -> String {
    let m = (seconds + 30) / 60
    return m < 60 ? "\(m) min" : "\(m / 60) h \(m % 60) min"
}

private func date(_ ms: UInt64) -> String {
    Date(timeIntervalSince1970: Double(ms) / 1000).formatted(date: .abbreviated, time: .shortened)
}
