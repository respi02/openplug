import SwiftUI

@main
struct OpenPlugApp: App {
    @StateObject private var model = DongleModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(model)
            #if DEBUG
                .task { model.autoconnectIfRequested() }
            #endif
            #if os(macOS)
                .frame(minWidth: 360, minHeight: 520)
            #endif
        }
    }
}
