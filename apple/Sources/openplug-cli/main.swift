// Command-line client for macOS: find a dongle over BLE, authenticate with the label
// PIN and print live values.
//
//   OPENPLUG_PIN=… swift run openplug-cli [--seconds N]
//
// The PIN comes from OPENPLUG_PIN, or from a prompt.

import Foundation
import OpenPlugBLE
import OpenPlugCore

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("error: \(message)\n".utf8))
    exit(1)
}

let args = CommandLine.arguments
let seconds = args.firstIndex(of: "--seconds").flatMap { args.indices.contains($0 + 1) ? Double(args[$0 + 1]) : nil }

var pin = ProcessInfo.processInfo.environment["OPENPLUG_PIN"] ?? ""
if pin.isEmpty {
    FileHandle.standardError.write(Data("PIN from the dongle label: ".utf8))
    pin = (readLine() ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
}

let transport = CoreBluetoothTransport()
do {
    FileHandle.standardError.write(Data("looking for a dongle …\n".utf8))
    try transport.open()
    FileHandle.standardError.write(Data("connected to \(transport.peripheralName ?? "dongle"); handshake …\n".utf8))

    let dongle = try Dongle.connect(transport: transport, pin: pin)
    try dongle.subscribeDefaults()
    FileHandle.standardError.write(Data("authenticated; streaming\n".utf8))

    let deadline = seconds.map { Date().addingTimeInterval($0) }
    while deadline.map({ Date() < $0 }) ?? true {
        for r in try dongle.nextReadings() {
            let label = r.name ?? String(format: "0x%06x", r.url)
            let value = r.available
                ? String(format: "%.\(r.precision)f", r.value) + (r.unit.map { " \($0)" } ?? "")
                : "—"
            print("\(label): \(value)")
        }
    }
    transport.close()
} catch DongleError.WrongPin {
    fail("the dongle rejected the PIN")
} catch {
    transport.close()
    fail("\(error)")
}
