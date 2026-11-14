#if os(iOS)
import ActivityKit
import Foundation

/// The trip in progress as a Live Activity (lock screen, Dynamic Island). Shared by
/// the app, which starts and updates it, and the widget extension, which draws it.
struct TripAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var speedKmh: Int
        var distanceKm: Double
        var movingS: Int
        /// l/100 km, once the trip is long enough to tell.
        var consumption: Double?
        /// True once the trip is over (the last state shown before it goes away).
        var finished: Bool
    }

    var startedAt: Date
}
#endif
