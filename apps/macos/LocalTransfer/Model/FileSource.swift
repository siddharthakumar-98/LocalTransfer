import Foundation
import UniformTypeIdentifiers

/// Where a pane's rows come from.
protocol FileSource: Sendable {
    /// Title of the first path-bar segment, e.g. "Desktop".
    var rootName: String { get }
    /// The contents of the folder at `components` (relative to the root).
    func list(_ components: [String]) async throws -> [FileItem]
}

/// Lists a folder on this Mac. Read-only.
struct LocalFileSource: FileSource {
    let root: URL

    var rootName: String { root.lastPathComponent }

    private static let keys: [URLResourceKey] = [
        .isDirectoryKey, .isPackageKey, .fileSizeKey,
        .contentModificationDateKey, .localizedTypeDescriptionKey, .contentTypeKey,
    ]

    func list(_ components: [String]) async throws -> [FileItem] {
        let dir = components.reduce(root) { $0.appendingPathComponent($1, isDirectory: true) }
        let urls = try FileManager.default.contentsOfDirectory(
            at: dir, includingPropertiesForKeys: Self.keys, options: [.skipsHiddenFiles])
        return urls.map { url in
            let values = try? url.resourceValues(forKeys: Set(Self.keys))
            // Packages (apps, Keynote files) behave like files, as in Finder.
            let isFolder = (values?.isDirectory ?? false) && !(values?.isPackage ?? false)
            let name = url.lastPathComponent
            let type = isFolder ? .folder
                : (values?.contentType ?? FileTypes.contentType(forName: name, isFolder: false))
            return FileItem(
                id: "L:" + url.path,
                name: name,
                isFolder: isFolder,
                size: isFolder ? nil : values?.fileSize.map(Int64.init),
                modified: values?.contentModificationDate,
                kind: isFolder ? "Folder"
                    : (values?.localizedTypeDescription ?? FileTypes.kind(for: type)),
                contentType: type,
                components: components + [name],
                location: .local(url))
        }
    }
}

/// Lists a folder on the other Mac through the backend.
struct RemoteFileSource: FileSource {
    let backend: CoreBackend
    /// Shared-root name on the peer.
    var root = "desktop"

    var rootName: String { "Desktop" }

    func list(_ components: [String]) async throws -> [FileItem] {
        let entries = try await backend.listDir(path: RemotePath(root: root, components: components))
        return entries.map { entry in
            let isFolder = entry.kind == .dir
            let type = FileTypes.contentType(forName: entry.name, isFolder: isFolder)
            let path = components + [entry.name]
            return FileItem(
                id: "R:" + path.joined(separator: "/"),
                name: entry.name,
                isFolder: isFolder,
                size: isFolder ? nil : Int64(clamping: entry.size),
                modified: entry.modified,
                kind: FileTypes.kind(for: type),
                contentType: type,
                components: path,
                location: .remote)
        }
    }
}

extension BackendError {
    /// What to tell the user. (UniFFI's own description is just the case name.)
    var userMessage: String {
        switch self {
        case .NotConnected: "Not connected to the other Mac."
        case .ConnectionLost: "The connection to the other Mac was lost."
        case .NotFound: "This folder no longer exists."
        case .PermissionDenied: "You don't have permission to see this folder."
        case .OutsideRoot: "That folder isn't shared."
        case .InvalidPath: "That path isn't valid."
        case .Conflict: "A file with that name already exists."
        case .HashMismatch: "The file was damaged in transit."
        case .Io(let message): "File error: \(message)"
        case .Internal(let message): "Something went wrong: \(message)"
        }
    }
}

/// A readable message for any error a source can throw.
func userMessage(for error: Error) -> String {
    if let backendError = error as? BackendError { return backendError.userMessage }
    let ns = error as NSError
    if ns.domain == NSCocoaErrorDomain, ns.code == NSFileReadNoPermissionError {
        return "LocalTransfer doesn't have permission to read this folder. "
            + "Allow it in System Settings → Privacy & Security → Files & Folders."
    }
    return error.localizedDescription
}
