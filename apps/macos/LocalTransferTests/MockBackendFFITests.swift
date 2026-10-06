import XCTest
@testable import LocalTransfer

/// Exercises the mock peer through the UniFFI boundary: async calls, the
/// event callback, and the sandbox it writes to.
final class MockBackendFFITests: XCTestCase {
    private var sandbox: URL!

    override func setUpWithError() throws {
        sandbox = FileManager.default.temporaryDirectory
            .appendingPathComponent("LocalTransferTests-\(UUID().uuidString)")
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: sandbox)
    }

    func testMockListsFixtureAndReceivesIntoSandbox() async throws {
        let backend = try CoreBackend.mock(
            config: MockConfig(sandboxRoot: sandbox.path, profile: .fast, seed: 1))
        XCTAssertEqual(backend.peer().deviceName, "Mock Mac")
        XCTAssertEqual(backend.connection(), .connected(quality: .good))

        let entries = try await backend.listDir(path: RemotePath(root: "desktop", components: []))
        XCTAssertTrue(entries.contains { $0.name == "Projects" && $0.kind == .dir })

        let finished = expectation(description: "transfer finished")
        let listener = EventCollector { event in
            if case .transferFinished(_, .completed) = event { finished.fulfill() }
        }
        backend.setEventListener(listener: listener)
        _ = try await backend.startTransfer(req: .get(
            remote: [RemotePath(root: "desktop", components: ["todo.txt"])],
            dest: "/ignored-by-the-mock"))
        await fulfillment(of: [finished], timeout: 5)

        let received = try XCTUnwrap(backend.mockControl()).receivedDir()
        XCTAssertTrue(received.hasPrefix(sandbox.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: received + "/todo.txt"))
        backend.setEventListener(listener: nil)
    }

    func testListingOutsideTheSharedRootFails() async throws {
        let backend = try CoreBackend.mock(
            config: MockConfig(sandboxRoot: sandbox.path, profile: .fast, seed: 1))
        do {
            _ = try await backend.listDir(path: RemotePath(root: "desktop", components: [".."]))
            XCTFail("expected InvalidPath")
        } catch let error as BackendError {
            XCTAssertEqual(error, .InvalidPath)
        }
    }
}

/// Forwards events to a closure. Called on a Rust runtime thread.
private final class EventCollector: EventListener, @unchecked Sendable {
    private let handler: (Event) -> Void
    init(_ handler: @escaping (Event) -> Void) { self.handler = handler }
    func onEvent(event: Event) { handler(event) }
}
