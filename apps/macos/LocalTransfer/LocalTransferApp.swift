import SwiftUI

@main
struct LocalTransferApp: App {
    // M1 runs against the mock peer (ROADMAP §5); the real node arrives in M3.
    private let backend = LocalTransferApp.makeBackend()

    var body: some Scene {
        WindowGroup("LocalTransfer") {
            ContentView(backend: backend)
                .frame(minWidth: 760, minHeight: 440)
        }
    }

    /// The mock peer, which only writes inside
    /// ~/Library/Application Support/LocalTransfer/MockSandbox. Falls back to
    /// the never-connected stub if that folder can't be created.
    private static func makeBackend() -> CoreBackend {
        guard let root = defaultMockSandboxRoot() else { return CoreBackend.stub() }
        let config = MockConfig(sandboxRoot: root, profile: .wifiGood, seed: 1)
        return (try? CoreBackend.mock(config: config)) ?? CoreBackend.stub()
    }
}
