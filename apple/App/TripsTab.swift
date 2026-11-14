import OpenPlugBLE
import SwiftUI

/// A row of the log: a trip (finished or in progress) or a refuel.
private enum Entry: Identifiable {
    case trip(TripRecord, running: Bool)
    case refuel(RefuelRecord)

    var timeMs: UInt64 {
        switch self {
        case .trip(let t, _): return t.startMs
        case .refuel(let r): return r.timeMs
        }
    }

    var id: String {
        switch self {
        case .trip(let t, _): return "t\(t.startMs)"
        case .refuel(let r): return "r\(r.timeMs)"
        }
    }

    var day: Date { Calendar.current.startOfDay(for: Date(timeIntervalSince1970: Double(timeMs) / 1000)) }
}

struct TripsTab: View {
    @EnvironmentObject private var model: DongleModel
    @State private var period = 1

    var body: AnyView {
        let calendar = Calendar.current
        let now = Date()
        let today = calendar.startOfDay(for: now)
        let from: Date = {
            switch period {
            case 0: return calendar.date(byAdding: .day, value: -6, to: today)!
            case 1: return calendar.date(from: calendar.dateComponents([.year, .month], from: now))!
            default: return calendar.date(from: calendar.dateComponents([.year], from: now))!
            }
        }()
        let fromMs = UInt64(from.timeIntervalSince1970 * 1000)
        let j = model.journal
        let trips = j.trips.filter { $0.startMs >= fromMs }
        let entries: [Entry] = ((j.active.map { [Entry.trip($0, running: true)] } ?? [])
            + trips.map { Entry.trip($0, running: false) }
            + j.refuels.filter { $0.timeMs >= fromMs }.map(Entry.refuel))
            .sorted { $0.timeMs > $1.timeMs }
        let days = Dictionary(grouping: entries, by: \.day).sorted { $0.key > $1.key }

        #if DEBUG
        // `--detail` opens the newest trip directly (for scripted checks)
        if CommandLine.arguments.contains("--detail"), let trip = j.active ?? j.trips.first {
            return AnyView(TripDetail(trip: trip))
        }
        #endif
        return AnyView(NavigationStack {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 14) {
                ScreenTitle("Trips")
                if let problem = model.journalProblem {
                    Text("Trips are not being recorded: \(problem)").foregroundStyle(.red)
                }
                Picker("Period", selection: $period) {
                    Text("7 days").tag(0)
                    Text(now.formatted(.dateTime.month(.wide))).tag(1)
                    Text(now.formatted(.dateTime.year())).tag(2)
                }
                .pickerStyle(.segmented)

                Summary(trips: trips)

                if entries.isEmpty {
                    Text("No trips in this period. A trip starts when the car moves while OpenPlug is connected, and ends 15 minutes after the connection drops.")
                        .font(.subheadline).foregroundStyle(Palette.quiet)
                }
                ForEach(days, id: \.key) { day, rows in
                    VStack(alignment: .leading, spacing: 8) {
                        Caption(label(day, today: today).uppercased())
                        Panel {
                            ForEach(Array(rows.enumerated()), id: \.element.id) { i, entry in
                                if i > 0 { Divider().overlay(Palette.line).padding(.vertical, 12) }
                                switch entry {
                                case .trip(let trip, let running):
                                    NavigationLink {
                                        TripDetail(trip: trip)
                                    } label: {
                                        TripRow(trip: trip, running: running)
                                    }
                                    .buttonStyle(.plain)
                                case .refuel(let refuel): RefuelRow(refuel: refuel)
                                }
                            }
                        }
                    }
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)
        }
        .background(Palette.ground)
        #if os(iOS)
        .toolbar(.hidden, for: .navigationBar)
        #endif
        })
    }

    private func label(_ day: Date, today: Date) -> String {
        let calendar = Calendar.current
        if day == today { return "Today" }
        if day == calendar.date(byAdding: .day, value: -1, to: today) { return "Yesterday" }
        return day.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated))
    }
}

private struct Summary: View {
    let trips: [TripRecord]

    var body: some View {
        let distance = trips.reduce(0) { $0 + $1.distanceKm }
        let moving = trips.reduce(UInt64(0)) { $0 + $1.movingS }
        let withFuel = trips.filter { $0.fuelUsedL != nil }
        let fuel = withFuel.reduce(0) { $0 + ($1.fuelUsedL ?? 0) }
        let fuelDistance = withFuel.reduce(0) { $0 + $1.distanceKm }
        Panel {
            HStack {
                stat(km(distance), trips.count == 1 ? "1 trip" : "\(trips.count) trips")
                stat(duration(moving), "driving")
            }
            HStack {
                stat(fuel > 0 ? litres(fuel) : "–", "fuel used")
                stat(fuel > 0 && fuelDistance >= 1 ? number(fuel / fuelDistance * 100, 1) : "–", "l/100 km average")
            }
            .padding(.top, 14)
        }
    }

    private func stat(_ value: String, _ label: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(value).font(.system(size: 24, weight: .semibold, design: .rounded)).monospacedDigit()
            Text(label).font(.caption).foregroundStyle(Palette.quiet)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct TripRow: View {
    let trip: TripRecord
    let running: Bool

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(clock(trip.startMs) + " – " + (running ? "now" : clock(trip.endMs))).fontWeight(.semibold)
                Text(details).font(.subheadline).foregroundStyle(Palette.quiet)
            }
            Spacer()
            Text(km(trip.distanceKm)).font(.system(size: 20, weight: .semibold, design: .rounded)).monospacedDigit()
            Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(Palette.quiet)
        }
        .contentShape(Rectangle())
    }

    private var details: String {
        var parts = [duration(trip.movingS)]
        if let c = trip.consumptionLPer100km { parts.append(number(c, 1) + " l/100 km") }
        if running { parts.append("in progress") }
        return parts.joined(separator: " · ")
    }
}

private struct RefuelRow: View {
    let refuel: RefuelRecord

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "fuelpump")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(Palette.amber)
                .frame(width: 32, height: 32)
                .background(Palette.amberSoft, in: RoundedRectangle(cornerRadius: 10))
            VStack(alignment: .leading, spacing: 2) {
                Text("Refuel").fontWeight(.semibold)
                Text(clock(refuel.timeMs) + (refuel.odometerKm.map { " · at \(number($0, 0)) km" } ?? ""))
                    .font(.subheadline).foregroundStyle(Palette.quiet)
            }
            Spacer()
            Text("+" + litres(refuel.addedL)).fontWeight(.semibold).foregroundStyle(Palette.amber).monospacedDigit()
        }
    }
}
