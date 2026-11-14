import OpenPlugBLE
import SwiftUI

/// Data objects the Car tab shows (see the catalog in research/notes/live_protocol.md).
private enum Url {
    static let speed: UInt32 = 0x020000
    static let fuelPercent: UInt32 = 0x020001
    static let fuel: UInt32 = 0x020002
    static let odometer: UInt32 = 0x020004
    static let range: UInt32 = 0x020005
    static let outside: UInt32 = 0x020006
    static let oilDash: UInt32 = 0x020030
    static let rpm: UInt32 = 0x040000
    static let coolant: UInt32 = 0x040001
    static let oil: UInt32 = 0x04000f
    static let battery: UInt32 = 0x050001
    static let serviceDays: UInt32 = 0x070009
    static let serviceKm: UInt32 = 0x070012
}

struct CarTab: View {
    @EnvironmentObject private var model: DongleModel
    let onSetUp: () -> Void

    private var connected: Bool {
        if case .connected = model.state { return true }
        return false
    }

    /// A value to show: live while connected, otherwise the last one remembered.
    private func value(_ url: UInt32) -> (value: Double, precision: Int)? {
        if connected, let r = model.readings.first(where: { $0.url == url && $0.available }) {
            return (Double(r.value), Int(r.precision))
        }
        if let l = model.journal.last.first(where: { $0.url == url }) {
            return (Double(l.value), Int(l.precision))
        }
        return nil
    }

    private func text(_ url: UInt32, _ unit: String, _ decimals: Int? = nil) -> String? {
        value(url).map { number($0.value, decimals ?? $0.precision) + unit }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    ScreenTitle("My car")
                    Spacer()
                    StatusPill(state: model.state)
                }
                intro
                if connected {
                    Hero(speed: text(Url.speed, "", 0) ?? "–", rpm: text(Url.rpm, "", 0) ?? "–",
                         trip: model.journal.tripRunning ? model.journal.active : nil)
                }
                if let due = model.journal.service, due.soon { ServiceDueCard(due: due) }
                tiles
                if !connected {
                    if model.recordsRoutes, let parked = model.journal.parked { ParkedCard(parked: parked) }
                    if let trip = model.journal.active ?? model.journal.trips.first { LastTrip(trip: trip) }
                    service
                }
                details
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)
        }
        .background(Palette.ground)
    }

    @ViewBuilder private var intro: some View {
        switch model.state {
        case .failed(let message):
            Panel {
                Text(message).foregroundStyle(.red)
                Button("Open settings", action: onSetUp).buttonStyle(.borderedProminent).padding(.top, 8)
            }
        case .idle where !model.hasPin:
            Panel {
                Text("Connect your dongle").font(.headline)
                Text("Enter the PIN printed on the DataPlug's label. OpenPlug finds the dongle over Bluetooth when the ignition is on.")
                    .foregroundStyle(Palette.quiet).padding(.top, 4)
                Button("Set up", action: onSetUp).buttonStyle(.borderedProminent).padding(.top, 12)
            }
        case .connected:
            EmptyView()
        default:
            let seen = model.journal.last.map(\.timeMs).max().map {
                "Last seen " + Date(timeIntervalSince1970: Double($0) / 1000)
                    .formatted(.relative(presentation: .named)) + ". "
            } ?? ""
            let hint = model.state == .idle
                ? "Connect in Settings to start recording."
                : "OpenPlug connects by itself when you switch the ignition on."
            Text(seen + hint).font(.subheadline).foregroundStyle(Palette.quiet)
        }
    }

    @ViewBuilder private var tiles: some View {
        let fuel = value(Url.fuel)
        let odometer = text(Url.odometer, "", 0)
        if fuel != nil || odometer != nil {
            HStack(alignment: .top, spacing: 12) {
                Panel {
                    Label("Fuel", systemImage: "fuelpump").font(.footnote.weight(.semibold)).foregroundStyle(Palette.quiet)
                    BigValue(fuel.map { number($0.value, 1) + " l" } ?? "–")
                    if let level = value(Url.fuelPercent).map({ $0.value / 100 }) ?? model.journal.fuelFraction {
                        ProgressView(value: min(max(level, 0), 1))
                            .tint(level < 0.15 ? Palette.amber : Palette.accent)
                            .padding(.vertical, 6)
                    }
                    if let range = text(Url.range, " km range", 0) { Caption(range) }
                }
                Panel {
                    Caption("Odometer")
                    BigValue(odometer ?? "–")
                    Caption("km")
                }
            }
        }
    }

    @ViewBuilder private var service: some View {
        let days = text(Url.serviceDays, " days", 0)
        let distance = text(Url.serviceKm, " km", 0)
        if let due = model.journal.service, !due.soon {
            Panel {
                Caption("Next service")
                Text(ServiceDueCard.left(due)).font(.title3.weight(.semibold)).padding(.top, 8)
            }
        } else if model.journal.service == nil, days != nil || distance != nil {
            Panel {
                Caption("Since the last service")
                HStack {
                    if let days { Text(days).font(.title3.weight(.semibold)).frame(maxWidth: .infinity, alignment: .leading) }
                    if let distance { Text(distance).font(.title3.weight(.semibold)).frame(maxWidth: .infinity, alignment: .leading) }
                }
                .padding(.top, 8)
            }
        }
    }

    @ViewBuilder private var details: some View {
        let rows: [(String, String)] = {
            var out: [(String, String)] = []
            if connected {
                if let v = text(Url.coolant, " °C", 0) { out.append(("Coolant", v)) }
                if let v = text(Url.oil, " °C", 0) ?? text(Url.oilDash, " °C", 0) { out.append(("Oil", v)) }
            }
            if let v = text(Url.battery, " V", 1) { out.append((connected ? "Battery" : "Battery when last seen", v)) }
            if connected, let v = text(Url.outside, " °C", 0) { out.append(("Outside", v)) }
            return out
        }()
        if !rows.isEmpty {
            Panel {
                ForEach(Array(rows.enumerated()), id: \.offset) { i, row in
                    if i > 0 { Divider().overlay(Palette.line).padding(.vertical, 10) }
                    HStack {
                        Text(row.0)
                        Spacer()
                        Text(row.1).fontWeight(.semibold).monospacedDigit()
                    }
                }
            }
        }
    }
}

/// A reminder that the service is near or overdue.
private struct ServiceDueCard: View {
    let due: ServiceDue

    static func left(_ due: ServiceDue) -> String {
        var parts: [String] = []
        if let km = due.kmLeft { parts.append(km < 0 ? number(-km, 0) + " km overdue" : "in " + number(km, 0) + " km") }
        if let d = due.daysLeft {
            parts.append(d < 0 ? "\(-d) days overdue" : d == 0 ? "today" : d == 1 ? "tomorrow" : "in \(d) days")
        }
        return parts.joined(separator: " or ")
    }

    var body: some View {
        let overdue = (due.kmLeft ?? 1) < 0 || (due.daysLeft ?? 1) < 0
        HStack(spacing: 12) {
            Image(systemName: "wrench.and.screwdriver")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(Palette.amber)
                .frame(width: 32, height: 32)
                .background(Palette.amberSoft, in: RoundedRectangle(cornerRadius: 10))
            VStack(alignment: .leading, spacing: 2) {
                Text(overdue ? "Service overdue" : "Service due soon").fontWeight(.semibold)
                Text(Self.left(due)).font(.subheadline).foregroundStyle(Palette.quiet)
            }
            Spacer()
        }
        .padding(16)
        .background(Palette.panel, in: RoundedRectangle(cornerRadius: 18))
        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Palette.amber.opacity(0.5)))
    }
}

private struct StatusPill: View {
    let state: LinkState

    var body: some View {
        let (label, active): (String, Bool) = {
            switch state {
            case .connected: return ("Connected · recording", true)
            case .connecting: return ("Looking for the car", false)
            case .waiting: return ("Waiting for the car", false)
            case .failed: return ("Stopped", false)
            case .idle: return ("Not connected", false)
            }
        }()
        HStack(spacing: 8) {
            Circle().fill(active ? Palette.accent : Palette.quiet).frame(width: 8, height: 8)
            Text(label).font(.footnote.weight(.semibold))
        }
        .padding(.horizontal, 12).padding(.vertical, 6)
        .background(active ? Palette.accentSoft : Palette.chip, in: Capsule())
        .foregroundStyle(active ? Palette.onAccentSoft : Palette.quiet)
    }
}

/// The dark card shown while connected: speed, engine speed and the trip in progress.
private struct Hero: View {
    let speed: String
    let rpm: String
    let trip: TripRecord?

    private let quiet = Palette.onHero.opacity(0.7)

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(trip != nil ? "CURRENT TRIP" : "LIVE")
                Spacer()
                if let trip { Text("since " + clock(trip.startMs)) }
            }
            .font(.footnote.weight(.semibold)).foregroundStyle(quiet)

            HStack(alignment: .lastTextBaseline) {
                Text(speed).font(.system(size: 72, weight: .semibold, design: .rounded)).monospacedDigit()
                Text("km/h").foregroundStyle(quiet)
                Spacer()
                VStack(alignment: .trailing) {
                    Text(rpm).font(.title2.weight(.semibold)).monospacedDigit()
                    Text("rpm").font(.caption).foregroundStyle(quiet)
                }
            }
            .padding(.top, 6)

            if let trip {
                Divider().overlay(Color.white.opacity(0.15)).padding(.vertical, 14)
                HStack {
                    stat(km(trip.distanceKm), "distance")
                    stat(duration(trip.movingS), "driving")
                    stat(trip.consumptionLPer100km.map { number($0, 1) + " l" } ?? "–", "per 100 km")
                }
            }
        }
        .padding(20)
        .foregroundStyle(Palette.onHero)
        .background(Palette.hero, in: RoundedRectangle(cornerRadius: 22))
    }

    private func stat(_ value: String, _ label: String) -> some View {
        VStack(alignment: .leading) {
            Text(value).font(.title3.weight(.semibold)).monospacedDigit()
            Text(label).font(.caption).foregroundStyle(quiet)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct BigValue: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text).font(.system(size: 28, weight: .semibold, design: .rounded)).monospacedDigit().padding(.top, 8)
    }
}

private struct LastTrip: View {
    let trip: TripRecord

    var body: some View {
        Panel {
            HStack {
                Caption("Last trip")
                Spacer()
                Caption(Date(timeIntervalSince1970: Double(trip.startMs) / 1000)
                    .formatted(.relative(presentation: .named, unitsStyle: .abbreviated))
                    + " · " + clock(trip.startMs) + " – " + clock(trip.endMs))
            }
            HStack(alignment: .lastTextBaseline, spacing: 14) {
                Text(km(trip.distanceKm)).font(.system(size: 28, weight: .semibold, design: .rounded)).monospacedDigit()
                Text(([duration(trip.movingS)] + (trip.consumptionLPer100km.map { [number($0, 1) + " l/100 km"] } ?? []))
                    .joined(separator: " · "))
                    .font(.subheadline).foregroundStyle(Palette.quiet)
            }
            .padding(.top, 8)
        }
    }
}
