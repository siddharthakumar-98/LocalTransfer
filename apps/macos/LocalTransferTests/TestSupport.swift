import Foundation
import UniformTypeIdentifiers
@testable import LocalTransfer

/// An in-memory folder tree that counts how often each folder is listed.
final class FakeSource: FileSource, @unchecked Sendable {
    let rootName = "Desktop"
    private let tree: [[String]: [FileItem]]
    private let failing: Set<[String]>
    private let lock = NSLock()
    private var calls: [[String]: Int] = [:]

    init(tree: [[String]: [FileItem]], failing: Set<[String]> = []) {
        self.tree = tree
        self.failing = failing
    }

    func listCount(_ components: [String]) -> Int {
        lock.withLock { calls[components, default: 0] }
    }

    func list(_ components: [String]) async throws -> [FileItem] {
        lock.withLock { calls[components, default: 0] += 1 }
        if failing.contains(components) { throw BackendError.NotConnected }
        guard let items = tree[components] else { throw BackendError.NotFound }
        return items
    }
}

let referenceDate = Date(timeIntervalSince1970: 1_791_288_720) // 2026-10-06 12:12 UTC

func fakeFile(_ name: String, in parent: [String] = [], size: Int64, daysAgo: Double, kind: String) -> FileItem {
    FileItem(
        id: "F:" + (parent + [name]).joined(separator: "/"),
        name: name, isFolder: false, size: size,
        modified: referenceDate.addingTimeInterval(-daysAgo * 86_400),
        kind: kind, contentType: FileTypes.contentType(forName: name, isFolder: false),
        components: parent + [name], location: .remote)
}

func fakeFolder(_ name: String, in parent: [String] = [], daysAgo: Double) -> FileItem {
    FileItem(
        id: "F:" + (parent + [name]).joined(separator: "/"),
        name: name, isFolder: true, size: nil,
        modified: referenceDate.addingTimeInterval(-daysAgo * 86_400),
        kind: "Folder", contentType: .folder,
        components: parent + [name], location: .remote)
}

/// Waits (up to ~1 s) for `condition`, letting spawned tasks run.
@MainActor
func eventually(_ condition: () -> Bool) async -> Bool {
    for _ in 0..<200 {
        if condition() { return true }
        try? await Task.sleep(nanoseconds: 5_000_000)
    }
    return condition()
}
