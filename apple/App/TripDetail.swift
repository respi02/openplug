import Charts
import MapKit
import OpenPlugBLE
import SwiftUI

/// One trip: its route on a map (when positions were recorded), the speed along it,
/// and its numbers.
struct TripDetail: View {
    @EnvironmentObject private var model: DongleModel
    let trip: TripRecord
    @State private var route: [RoutePoint] = []

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(day + " · " + clock(trip.startMs) + " – " + clock(trip.endMs))
                        .font(.subheadline).foregroundStyle(Palette.quiet)
                    Text(km(trip.distanceKm)).font(.system(size: 48, weight: .semibold, design: .rounded)).monospacedDigit()
                }

                if route.count >= 2 {
                    RouteMap(route: route)
                        .frame(height: 260)
                        .clipShape(RoundedRectangle(cornerRadius: 18))
                        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Palette.line))
                    Panel {
                        HStack {
                            Caption("Speed")
                            Spacer()
                            Caption("max \(number(Double(trip.maxSpeedKmh), 0)) km/h")
                        }
                        Chart(route, id: \.timeMs) { p in
                            LineMark(
                                x: .value("Time", Date(timeIntervalSince1970: Double(p.timeMs) / 1000)),
                                y: .value("km/h", p.speedKmh)
                            )
                            .interpolationMethod(.monotone)
                            .foregroundStyle(Palette.accent)
                        }
                        .chartYAxis { AxisMarks(position: .leading) }
                        .chartXAxis {
                            AxisMarks(values: .automatic(desiredCount: 3)) {
                                AxisGridLine()
                                AxisValueLabel(format: .dateTime.hour().minute())
                            }
                        }
                        .frame(height: 110)
                        .padding(.top, 10)
                    }
                } else {
                    Text("No route was recorded for this trip. Switch on Record routes in Settings to see trips on a map.")
                        .font(.subheadline).foregroundStyle(Palette.quiet)
                }

                Panel {
                    Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 16) {
                        GridRow {
                            stat(duration(trip.movingS), "driving")
                            stat(number(trip.avgSpeedKmh, 0) + " km/h", "average speed")
                        }
                        GridRow {
                            stat(trip.fuelUsedL.flatMap { $0 > 0 ? number($0, 2) + " l" : nil } ?? "–", "fuel used")
                            stat(trip.consumptionLPer100km.map { number($0, 1) + " l" } ?? "–", "per 100 km")
                        }
                        GridRow {
                            stat(number(Double(trip.maxRpm), 0) + " rpm", "highest engine speed")
                            stat(trip.endOdometerKm.map { number($0, 0) + " km" } ?? "–", "odometer at the end")
                        }
                    }
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)
        }
        .background(Palette.ground)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        // a trip in progress grows: reload its route as the journal moves on
        .task(id: model.journal.active?.startMs == trip.startMs ? model.journal.active?.movingS : nil) {
            route = model.route(of: trip)
        }
    }

    private var day: String {
        Date(timeIntervalSince1970: Double(trip.startMs) / 1000)
            .formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated))
    }

    private func stat(_ value: String, _ label: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(value).font(.title3.weight(.semibold)).monospacedDigit()
            Text(label).font(.caption).foregroundStyle(Palette.quiet)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A trip's route with its start and end.
struct RouteMap: View {
    let route: [RoutePoint]

    var body: some View {
        let line = route.map { CLLocationCoordinate2D(latitude: $0.lat, longitude: $0.lon) }
        Map(initialPosition: .automatic) {
            MapPolyline(coordinates: line).stroke(Palette.accent, lineWidth: 4)
            if let first = line.first { Marker("Start", systemImage: "flag", coordinate: first).tint(.gray) }
            if let last = line.last { Marker("End", systemImage: "flag.checkered", coordinate: last).tint(Palette.accent) }
        }
    }
}

/// Where the car was left, on a small map that opens the Maps app.
struct ParkedCard: View {
    let parked: ParkedAt

    var body: some View {
        let place = CLLocationCoordinate2D(latitude: parked.lat, longitude: parked.lon)
        Panel {
            HStack {
                Caption("Parked")
                Spacer()
                Caption(Date(timeIntervalSince1970: Double(parked.timeMs) / 1000)
                    .formatted(.relative(presentation: .named)))
            }
            Map(initialPosition: .region(MKCoordinateRegion(center: place, latitudinalMeters: 500, longitudinalMeters: 500)),
                interactionModes: []) {
                Marker("Car", systemImage: "car.fill", coordinate: place).tint(Palette.accent)
            }
            .frame(height: 150)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .padding(.top, 10)
            if let url = URL(string: "http://maps.apple.com/?ll=\(parked.lat),\(parked.lon)&q=Parked%20car") {
                Link("Open in Maps", destination: url).font(.subheadline.weight(.semibold)).padding(.top, 10)
            }
        }
    }
}
