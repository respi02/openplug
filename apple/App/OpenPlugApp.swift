import SwiftUI

@main
struct OpenPlugApp: App {
    // Created here, at launch, rather than with the first view: when iOS relaunches the
    // app in the background to hand back a dongle connection, no view is built, but the
    // model has to pick the session up.
    @ObservedObject private var model = DongleModel.shared

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
