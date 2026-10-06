import XCTest
@testable import LocalTransfer

final class FileSourceTests: XCTestCase {
    private var dir: URL!

    override func setUpWithError() throws {
        dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("FileSourceTests-\(UUID().uuidString)", isDirectory: true)
        let fm = FileManager.default
        try fm.createDirectory(at: dir.appendingPathComponent("Sub"), withIntermediateDirectories: true)
        try fm.createDirectory(at: dir.appendingPathComponent("Thing.app/Contents"), withIntermediateDirectories: true)
        try Data("abc".utf8).write(to: dir.appendingPathComponent("b.txt"))
        try Data().write(to: dir.appendingPathComponent(".hidden"))
        try Data("# hi".utf8).write(to: dir.appendingPathComponent("Sub/inner.md"))
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: dir)
    }

    func testLocalSourceListsLikeFinder() async throws {
        let source = LocalFileSource(root: dir)
        XCTAssertEqual(source.rootName, dir.lastPathComponent)
        let items = try await source.list([])
        XCTAssertEqual(Set(items.map(\.name)), ["b.txt", "Sub", "Thing.app"], "hidden files skipped")

        let sub = try XCTUnwrap(items.first { $0.name == "Sub" })
        XCTAssertTrue(sub.isFolder)
        XCTAssertNil(sub.size)
        XCTAssertEqual(sub.kind, "Folder")

        let app = try XCTUnwrap(items.first { $0.name == "Thing.app" })
        XCTAssertFalse(app.isFolder, "packages behave like files")

        let text = try XCTUnwrap(items.first { $0.name == "b.txt" })
        XCTAssertEqual(text.size, 3)
        XCTAssertFalse(text.kind.isEmpty)
        XCTAssertNotNil(text.modified)

        let inner = try await source.list(["Sub"])
        XCTAssertEqual(inner.map(\.components), [["Sub", "inner.md"]])
        guard case .local(let url) = inner[0].location else { return XCTFail("local item") }
        XCTAssertEqual(url.lastPathComponent, "inner.md")
    }

    func testRemoteSourceListsTheMockPeer() async throws {
        let backend = try CoreBackend.mock(
            config: MockConfig(sandboxRoot: dir.appendingPathComponent("sandbox").path, profile: .fast, seed: 1))
        let source = RemoteFileSource(backend: backend)
        XCTAssertEqual(source.rootName, "Desktop")

        let root = try await source.list([])
        let projects = try XCTUnwrap(root.first { $0.name == "Projects" })
        XCTAssertTrue(projects.isFolder)
        XCTAssertEqual(projects.kind, "Folder")
        XCTAssertEqual(projects.id, "R:Projects")
        let report = try XCTUnwrap(root.first { $0.name == "Quarterly Report.pdf" })
        XCTAssertEqual(report.contentType, .pdf)
        XCTAssertGreaterThan(report.size ?? 0, 0)

        let inside = try await source.list(["Projects", "LocalTransfer"])
        XCTAssertTrue(inside.contains { $0.components == ["Projects", "LocalTransfer", "README.md"] })
        let unknown = try XCTUnwrap(inside.first { $0.name == "build-log.unknownext" })
        XCTAssertEqual(unknown.kind, "Document")
    }
}
