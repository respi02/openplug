import CoreLocation
import Foundation

/// The phone's position while the car is connected, for the route of a trip and the
/// place the car was left. Runs only between `start()` and `stop()`; nothing leaves
/// the device.
final class LocationFeed: NSObject, CLLocationManagerDelegate {
    private let manager = CLLocationManager()
    private var running = false
    /// Called on the main thread with every usable fix.
    var onFix: ((_ lat: Double, _ lon: Double) -> Void)?
    /// Called when the user answers (or changes) the permission.
    var onAuthorizationChange: (() -> Void)?

    override init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyBest
        manager.distanceFilter = 10
        manager.activityType = .automotiveNavigation
        #if os(iOS)
        manager.pausesLocationUpdatesAutomatically = false
        #endif
    }

    var status: CLAuthorizationStatus { manager.authorizationStatus }

    /// Whether fixes also arrive when the app was woken in the background.
    var worksInBackground: Bool { status == .authorizedAlways }

    var isAuthorized: Bool {
        #if os(iOS)
        status == .authorizedAlways || status == .authorizedWhenInUse
        #else
        status == .authorizedAlways
        #endif
    }

    /// Asks for "Always": a session resumed by iOS in the background starts its
    /// location updates there, which "While Using" does not allow.
    func requestPermission() {
        manager.requestAlwaysAuthorization()
    }

    func start() {
        guard !running, isAuthorized else { return }
        running = true
        #if os(iOS)
        manager.allowsBackgroundLocationUpdates = true
        manager.showsBackgroundLocationIndicator = false
        #endif
        manager.startUpdatingLocation()
    }

    func stop() {
        guard running else { return }
        running = false
        manager.stopUpdatingLocation()
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        for l in locations where l.horizontalAccuracy >= 0 && l.horizontalAccuracy <= 50 {
            onFix?(l.coordinate.latitude, l.coordinate.longitude)
        }
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {}

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        onAuthorizationChange?()
    }
}
