import OpenPlugBLE
import SwiftUI

struct SettingsTab: View {
    @EnvironmentObject private var model: DongleModel
    @State private var pin = Keychain.pin ?? ""
    // on by default in debug builds, which are the ones taken on test drives
    #if DEBUG
    @AppStorage("recordValues") private var record = true
    @State private var useDemo = false
    @AppStorage("demoHost") private var demoHost = "127.0.0.1"
    #else
    @AppStorage("recordValues") private var record = false
    #endif

    private var target: Target {
        #if DEBUG
        if useDemo { return .demo(host: demoHost.trimmingCharacters(in: .whitespaces)) }
        #endif
        return .bluetooth
    }

    var body: some View {
        NavigationStack {
            Form {
                dongle
                CarSection()
                Section {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Runs in the background")
                        Text("While connected or waiting, iOS wakes OpenPlug when the car is in reach. Do not swipe the app away — iOS then stops waking it.")
                            .font(.subheadline).foregroundStyle(.secondary)
                    }
                    Toggle(isOn: $model.recordsRoutes) {
                        Text("Record routes")
                        Text(model.recordsRoutes
                            ? model.routePermission
                            : "Uses GPS while the car is connected: trips on a map and where the car is parked. Stays on this device.")
                    }
                    Toggle(isOn: $model.looksUpAddresses) {
                        Text("Addresses for trips")
                        Text("Looks up the street address of each trip's start and end, and of the parking place, with Apple Maps. Sends those positions to Apple.")
                    }
                    Toggle(isOn: $record) {
                        Text("Record values for replay")
                        Text("For bug reports and testing. No position, no VIN." + (model.isActive ? " Applies from the next connect." : ""))
                    }
                } header: {
                    Text("Recording")
                }
                Section {
                    NavigationLink {
                        SignalsView()
                    } label: {
                        LabeledContent("All live signals", value: model.readings.isEmpty ? "" : "\(model.readings.count)")
                    }
                    if let csv = model.journal.trips.isEmpty ? nil : model.exportCsv() {
                        ShareLink(item: csv, subject: Text("OpenPlug trips")) {
                            LabeledContent("Export trips", value: "CSV")
                        }
                    } else {
                        LabeledContent("Export trips", value: "none yet")
                    }
                    LabeledContent {
                        let n = model.recordingCount()
                        Text(n == 1 ? "1 file" : "\(n) files")
                    } label: {
                        Text("Recordings")
                        #if os(iOS)
                        Text("Files › On My iPhone › OpenPlug › Recordings")
                        #else
                        Text("Documents › OpenPlug Recordings")
                        #endif
                    }
                } header: {
                    Text("Data")
                } footer: {
                    Text("OpenPlug \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "") · open source (MIT). Talks to your dongle locally over Bluetooth — no account, no server. Not affiliated with the vehicle or dongle maker.")
                        .padding(.top, 8)
                }
            }
            #if os(macOS)
            .formStyle(.grouped)
            #endif
            .navigationTitle("Settings")
        }
    }

    @ViewBuilder private var dongle: some View {
        if model.isActive {
            Section("Dongle") {
                LabeledContent {
                    if case .connected = model.state {
                        Circle().fill(Palette.accent).frame(width: 10, height: 10)
                    } else {
                        ProgressView()
                    }
                } label: {
                    switch model.state {
                    case .connected(let name):
                        Text(name)
                        Text("Connected")
                    case .connecting(let what), .waiting(let what):
                        Text("DataPlug")
                        Text(what)
                    default:
                        Text("DataPlug")
                    }
                }
                Button("Disconnect and stop", role: .destructive) { model.disconnect() }
            }
        } else {
            Section {
                #if DEBUG
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
                #endif
                SecureField("PIN from the dongle label", text: $pin)
                    .autocorrectionDisabled()
                #if os(iOS)
                    .textInputAutocapitalization(.characters)
                #endif
                Button("Connect") {
                    let trimmed = pin.trimmingCharacters(in: .whitespacesAndNewlines)
                    Keychain.pin = trimmed
                    model.connect(to: target, pin: trimmed, record: record)
                }
                .disabled(pin.trimmingCharacters(in: .whitespaces).isEmpty)
                if case .failed(let message) = model.state {
                    Text(message).foregroundStyle(.red)
                }
            } header: {
                Text("Dongle")
            } footer: {
                Text("The PIN is printed on the dongle. It stays on this device, in the Keychain. OpenPlug then keeps looking for the dongle — it answers when the ignition is on.")
            }
        }
    }
}

/// What the dongle does not report: the tank size and the service interval.
private struct CarSection: View {
    @EnvironmentObject private var model: DongleModel
    @State private var tank = ""
    @State private var everyKm = ""
    @State private var everyMonths = 0
    @State private var loaded = false

    var body: some View {
        Section {
            LabeledContent("Tank size") {
                HStack {
                    TextField("e.g. 50", text: $tank).multilineTextAlignment(.trailing)
                    Text("l").foregroundStyle(.secondary)
                }
                #if os(iOS)
                .keyboardType(.decimalPad)
                #endif
            }
            LabeledContent("Service every") {
                HStack {
                    TextField("e.g. 15 000", text: $everyKm).multilineTextAlignment(.trailing)
                    Text("km").foregroundStyle(.secondary)
                }
                #if os(iOS)
                .keyboardType(.numberPad)
                #endif
            }
            Picker("or every", selection: $everyMonths) {
                Text("–").tag(0)
                ForEach([6, 12, 24, 36], id: \.self) { m in
                    Text(m % 12 == 0 ? (m == 12 ? "year" : "\(m / 12) years") : "\(m) months").tag(m)
                }
            }
        } header: {
            Text("Car")
        } footer: {
            Text("The tank size turns the litres the car reports into a fuel gauge. With the service interval of your car's service plan, OpenPlug counts down to the next service from the car's own \"since the last service\" counters, and reminds you 1000 km or 30 days before.")
        }
        .onAppear {
            guard !loaded else { return }
            loaded = true
            let car = model.journal.car
            let plain = { (v: Double) in v.formatted(.number.grouping(.never).precision(.fractionLength(0...1))) }
            tank = car.tankL.map(plain) ?? ""
            everyKm = car.serviceEveryKm.map(plain) ?? ""
            everyMonths = car.serviceEveryDays.map { Int((Double($0) / 30.4375).rounded()) } ?? 0
        }
        .onChange(of: tank) { save() }
        .onChange(of: everyKm) { save() }
        .onChange(of: everyMonths) { save() }
    }

    private func save() {
        guard loaded else { return }
        func parse(_ text: String) -> Double? {
            let digits = text.filter { !$0.isWhitespace }
            if digits.isEmpty { return nil }
            return (try? Double(digits, format: .number)) ?? Double(digits.replacingOccurrences(of: ",", with: "."))
        }
        let days = everyMonths > 0 ? UInt32((Double(everyMonths) * 30.4375).rounded()) : nil
        let settings = CarSettings(tankL: parse(tank), serviceEveryKm: parse(everyKm), serviceEveryDays: days)
        if settings != model.journal.car { model.setCar(settings) }
    }
}

/// Every data object the dongle streams, labelled or not: the diagnostic view.
private struct SignalsView: View {
    @EnvironmentObject private var model: DongleModel

    var body: some View {
        List {
            if model.readings.isEmpty {
                Text("Nothing yet — the values appear while the car is connected.").foregroundStyle(.secondary)
            }
            ForEach(model.readings, id: \.url) { r in
                HStack {
                    Text(r.name ?? String(format: "0x%06x", r.url))
                    Spacer()
                    Text(format(r)).monospacedDigit().foregroundStyle(r.available ? .primary : .secondary)
                }
            }
        }
        .navigationTitle("Live signals")
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
