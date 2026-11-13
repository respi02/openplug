import OpenPlugBLE
import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: DongleModel

    var body: some View {
        NavigationStack {
            Group {
                if case .connected(let name) = model.state {
                    LiveView(dongleName: name)
                } else {
                    SetupView()
                }
            }
            .navigationTitle("OpenPlug")
        }
    }
}

private struct SetupView: View {
    @EnvironmentObject private var model: DongleModel
    @State private var pin = Keychain.pin ?? ""
    @State private var target = Target.bluetooth

    var body: some View {
        Form {
            #if DEBUG
            Section {
                Picker("Connect to", selection: $target) {
                    Text("Dongle (Bluetooth)").tag(Target.bluetooth)
                    Text("Fake dongle (TCP 127.0.0.1:4455)").tag(Target.demo)
                }
            }
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
            Section {
                Button("Connect") {
                    let trimmed = pin.trimmingCharacters(in: .whitespacesAndNewlines)
                    Keychain.pin = trimmed
                    model.connect(to: target, pin: trimmed)
                }
                .disabled(model.isBusy || pin.trimmingCharacters(in: .whitespaces).isEmpty)

                switch model.state {
                case .connecting(let what):
                    HStack {
                        ProgressView()
                        Text(what).foregroundStyle(.secondary).padding(.leading, 8)
                    }
                case .failed(let message):
                    Text(message).foregroundStyle(.red)
                default:
                    EmptyView()
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
    let dongleName: String

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
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Disconnect") { model.disconnect() }
            }
        }
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .navigationTitle(dongleName)
    }

    private func format(_ r: Reading) -> String {
        guard r.available else { return "—" }
        let number = r.value.formatted(.number.precision(.fractionLength(Int(r.precision))))
        return r.unit.map { "\(number) \($0)" } ?? number
    }
}
