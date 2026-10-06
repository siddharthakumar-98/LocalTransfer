import SwiftUI

@main
struct LocalTransferApp: App {
    // M1 runs against the mock peer (ROADMAP §5); the real node arrives in M3.
    private let backend = LocalTransferApp.makeBackend()
    private let localRoot = LocalTransferApp.makeLocalRoot()

    var body: some Scene {
        WindowGroup("LocalTransfer") {
            ContentView(backend: backend, localRoot: localRoot)
                .frame(minWidth: 900, minHeight: 480)
        }
    }

    /// True when this process is the host app for unit tests.
    static var isRunningTests: Bool {
        ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil
    }

    /// The mock peer, which only writes inside
    /// ~/Library/Application Support/LocalTransfer/MockSandbox. Falls back to
    /// the never-connected stub if that folder can't be created.
    private static func makeBackend() -> CoreBackend {
        guard let root = defaultMockSandboxRoot() else { return CoreBackend.stub() }
        let config = MockConfig(sandboxRoot: root, profile: .wifiGood, seed: 1)
        return (try? CoreBackend.mock(config: config)) ?? CoreBackend.stub()
    }

    /// ~/Desktop. Under unit tests, an empty temporary folder instead, so a
    /// test run never triggers the Desktop privacy prompt (which would block CI).
    private static func makeLocalRoot() -> URL {
        if isRunningTests {
            let dir = FileManager.default.temporaryDirectory
                .appendingPathComponent("LocalTransferTestHost", isDirectory: true)
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            return dir
        }
        return FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Desktop", isDirectory: true)
    }
}
