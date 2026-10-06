import SwiftUI

@main
struct LocalTransferApp: App {
    var body: some Scene {
        WindowGroup("LocalTransfer") {
            // M1.1 runs against the stub backend; the mock peer replaces it in M1.2.
            ContentView(backend: CoreBackend.stub())
                .frame(minWidth: 760, minHeight: 440)
        }
    }
}
