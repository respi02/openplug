import OpenPlugBLE
import SwiftUI

struct ContentView: View {
    // `--trips` opens on the Trips tab (handy for screenshots and scripted checks)
    @State private var tab = CommandLine.arguments.contains("--trips") ? 1
        : CommandLine.arguments.contains("--settings") ? 2 : 0

    var body: some View {
        TabView(selection: $tab) {
            CarTab(onSetUp: { tab = 2 })
                .tabItem { Label("Car", systemImage: "car.fill") }
                .tag(0)
            TripsTab()
                .tabItem { Label("Trips", systemImage: "road.lanes") }
                .tag(1)
            SettingsTab()
                .tabItem { Label("Settings", systemImage: "gearshape") }
                .tag(2)
        }
        .tint(Palette.accent)
    }
}

// MARK: - the look, shared by the tabs

/// One calm look: a quiet ground, white panels, one petrol-blue accent, amber for
/// refuels and reminders.
enum Palette {
    static let accent = dynamic(light: 0x0B6E99, dark: 0x7CC7E8)
    static let accentSoft = dynamic(light: 0xE1EFF5, dark: 0x0D3A4D)
    static let onAccentSoft = dynamic(light: 0x08506F, dark: 0xCDEBF7)
    static let ground = dynamic(light: 0xF4F5F2, dark: 0x101315)
    static let panel = dynamic(light: 0xFFFFFF, dark: 0x1A1E21)
    static let line = dynamic(light: 0xE2E5E0, dark: 0x2C3136)
    static let chip = dynamic(light: 0xE8EBE6, dark: 0x2A2F33)
    static let quiet = dynamic(light: 0x4D5550, dark: 0xB7BDB8)
    static let hero = dynamic(light: 0x15181C, dark: 0x23282C)
    static let onHero = Color(hex: 0xF4F5F2)
    static let amber = dynamic(light: 0x8A4A07, dark: 0xF0B46A)
    static let amberSoft = dynamic(light: 0xFBEBD6, dark: 0x4A2C06)

    private static func dynamic(light: UInt32, dark: UInt32) -> Color {
        #if os(iOS)
        Color(UIColor { $0.userInterfaceStyle == .dark ? UIColor(Color(hex: dark)) : UIColor(Color(hex: light)) })
        #else
        Color(NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            return NSColor(Color(hex: isDark ? dark : light))
        })
        #endif
    }
}

extension Color {
    init(hex: UInt32) {
        self.init(red: Double((hex >> 16) & 0xff) / 255, green: Double((hex >> 8) & 0xff) / 255, blue: Double(hex & 0xff) / 255)
    }
}

/// A white, bordered, rounded panel.
struct Panel<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) { content }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.panel, in: RoundedRectangle(cornerRadius: 18))
            .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Palette.line))
    }
}

/// A small caption above a panel or beside a value.
struct Caption: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text).font(.footnote.weight(.semibold)).foregroundStyle(Palette.quiet)
    }
}

struct ScreenTitle: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text).font(.system(size: 30, weight: .bold, design: .rounded))
    }
}

func number(_ v: Double, _ decimals: Int) -> String {
    v.formatted(.number.precision(.fractionLength(decimals)))
}

func km(_ v: Double) -> String { number(v, v < 100 ? 1 : 0) + " km" }

func litres(_ v: Double) -> String { number(v, 1) + " l" }

func duration(_ seconds: UInt64) -> String {
    let m = (seconds + 30) / 60
    return m < 60 ? "\(m) min" : "\(m / 60) h \(m % 60) min"
}

func clock(_ ms: UInt64) -> String {
    Date(timeIntervalSince1970: Double(ms) / 1000).formatted(date: .omitted, time: .shortened)
}
