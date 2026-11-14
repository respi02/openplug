import ActivityKit
import SwiftUI
import WidgetKit

@main
struct OpenPlugLiveBundle: WidgetBundle {
    var body: some Widget {
        TripLiveActivity()
    }
}

private let accent = Color(red: 0x7C / 255, green: 0xC7 / 255, blue: 0xE8 / 255)

private func km(_ v: Double) -> String {
    v.formatted(.number.precision(.fractionLength(v < 100 ? 1 : 0)))
}

private func duration(_ seconds: Int) -> String {
    let m = (seconds + 30) / 60
    return m < 60 ? "\(m) min" : "\(m / 60) h \(m % 60) min"
}

/// The trip in progress on the lock screen and in the Dynamic Island.
struct TripLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: TripAttributes.self) { context in
            LockScreen(state: context.state, startedAt: context.attributes.startedAt)
                .activityBackgroundTint(Color(red: 0x15 / 255, green: 0x18 / 255, blue: 0x1C / 255))
                .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { context in
            let s = context.state
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    VStack(alignment: .leading) {
                        Text("\(s.speedKmh)").font(.system(size: 34, weight: .semibold, design: .rounded)).monospacedDigit()
                        Text("km/h").font(.caption2).foregroundStyle(.secondary)
                    }
                    .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    VStack(alignment: .trailing) {
                        Text(km(s.distanceKm)).font(.system(size: 34, weight: .semibold, design: .rounded)).monospacedDigit()
                        Text("km").font(.caption2).foregroundStyle(.secondary)
                    }
                    .padding(.trailing, 4)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack {
                        Label(s.finished ? "Trip finished" : "Trip", systemImage: "car.fill").foregroundStyle(accent)
                        Spacer()
                        Text(duration(s.movingS) + (s.consumption.map { " · \($0.formatted(.number.precision(.fractionLength(1)))) l/100 km" } ?? ""))
                            .foregroundStyle(.secondary)
                    }
                    .font(.footnote)
                }
            } compactLeading: {
                Image(systemName: "car.fill").foregroundStyle(accent)
            } compactTrailing: {
                Text(km(s.distanceKm) + " km").monospacedDigit().foregroundStyle(accent)
            } minimal: {
                Image(systemName: "car.fill").foregroundStyle(accent)
            }
        }
    }
}

private struct LockScreen: View {
    let state: TripAttributes.ContentState
    let startedAt: Date

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Label(state.finished ? "Trip finished" : "Current trip", systemImage: "car.fill")
                    .font(.footnote.weight(.semibold)).foregroundStyle(accent)
                Spacer()
                Text("since " + startedAt.formatted(date: .omitted, time: .shortened))
                    .font(.footnote).foregroundStyle(.white.opacity(0.7))
            }
            HStack(alignment: .lastTextBaseline) {
                stat(km(state.distanceKm), "km", big: true)
                Spacer()
                if !state.finished { stat("\(state.speedKmh)", "km/h") }
                stat(duration(state.movingS), "driving")
                if let c = state.consumption {
                    stat(c.formatted(.number.precision(.fractionLength(1))), "l/100 km")
                }
            }
        }
        .padding(16)
        .foregroundStyle(.white)
    }

    private func stat(_ value: String, _ label: String, big: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(value).font(.system(size: big ? 34 : 20, weight: .semibold, design: .rounded)).monospacedDigit()
            Text(label).font(.caption2).foregroundStyle(.white.opacity(0.7))
        }
        .padding(.leading, big ? 0 : 10)
    }
}
