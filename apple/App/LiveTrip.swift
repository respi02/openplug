#if os(iOS)
import ActivityKit
import Foundation
import OpenPlugBLE

/// Keeps the Live Activity in step with the trip in progress.
///
/// iOS lets an app *start* a Live Activity only while it is in the foreground; updating
/// and ending work from the background. A trip that begins while the app is in the
/// background (the usual case: the session reconnects by itself) therefore gets its
/// Live Activity when the app is next opened during the drive. Starting is simply
/// retried while a trip runs.
@MainActor
final class LiveTrip {
    static let shared = LiveTrip()

    private var activity: Activity<TripAttributes>?
    private var shown: TripAttributes.ContentState?
    private var lastUpdate = Date.distantPast
    private var lastAttempt = Date.distantPast

    /// Call whenever the trip or the speed changes; cheap when nothing is to be done.
    func update(trip: TripRecord?, running: Bool, speedKmh: Double?) {
        let now = Date()
        guard let trip, running else {
            finish()
            return
        }
        let state = TripAttributes.ContentState(
            speedKmh: Int((speedKmh ?? 0).rounded()),
            distanceKm: (trip.distanceKm * 10).rounded() / 10,
            movingS: Int(trip.movingS),
            consumption: trip.consumptionLPer100km.map { ($0 * 10).rounded() / 10 },
            finished: false
        )
        // stays fresh for two minutes; without updates iOS then marks it stale
        let content = ActivityContent(state: state, staleDate: now.addingTimeInterval(120))

        if activity == nil {
            // one left over from before the app was relaunched, else a new one
            let started = Date(timeIntervalSince1970: Double(trip.startMs) / 1000)
            activity = Activity<TripAttributes>.activities.first { $0.attributes.startedAt == started }
            if activity == nil, now.timeIntervalSince(lastAttempt) >= 5 {
                lastAttempt = now
                guard ActivityAuthorizationInfo().areActivitiesEnabled else { return }
                activity = try? Activity.request(attributes: TripAttributes(startedAt: started), content: content)
                shown = activity == nil ? nil : state
                lastUpdate = now
                return
            }
        }
        guard let activity, state != shown, now.timeIntervalSince(lastUpdate) >= 3 else { return }
        shown = state
        lastUpdate = now
        Task { await activity.update(content) }
    }

    /// The trip is over (or paused): show its last numbers for a while, then go away.
    private func finish() {
        let ending = Activity<TripAttributes>.activities
        guard activity != nil || !ending.isEmpty else { return }
        var last = shown
        last?.finished = true
        last?.speedKmh = 0
        activity = nil
        shown = nil
        Task {
            for a in ending {
                let state = last ?? a.content.state
                await a.end(ActivityContent(state: state, staleDate: nil), dismissalPolicy: .after(Date().addingTimeInterval(600)))
            }
        }
    }
}
#endif
