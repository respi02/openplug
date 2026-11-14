import CoreLocation
import Foundation

/// Street addresses for positions, from the system's reverse geocoder (Apple Maps).
/// Used only when the user switches it on: each lookup sends one position to Apple.
@MainActor
final class Addresses {
    private let geocoder = CLGeocoder()
    private var cache: [String: String] = [:]

    /// The address of a place, or "" when there is none (open country); nil when the
    /// lookup failed (offline, rate limited) and is worth trying again later.
    func lookUp(lat: Double, lon: Double) async -> String? {
        let key = String(format: "%.4f,%.4f", lat, lon) // ~10 m
        if let hit = cache[key] { return hit }
        do {
            let marks = try await geocoder.reverseGeocodeLocation(CLLocation(latitude: lat, longitude: lon))
            let text = marks.first.map(Self.format) ?? ""
            cache[key] = text
            return text
        } catch let error as CLError where error.code == .geocodeFoundNoResult {
            cache[key] = ""
            return ""
        } catch {
            return nil
        }
    }

    /// "Street 12, Town", or the best of what there is.
    private static func format(_ p: CLPlacemark) -> String {
        let street = [p.thoroughfare, p.subThoroughfare].compactMap { $0 }.joined(separator: " ")
        let first = street.isEmpty ? (p.name ?? "") : street
        return [first, p.locality ?? p.subAdministrativeArea ?? ""]
            .filter { !$0.isEmpty }
            .reduce(into: [String]()) { if !$0.contains($1) { $0.append($1) } }
            .joined(separator: ", ")
    }
}
