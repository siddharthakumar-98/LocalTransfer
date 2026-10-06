import Foundation
import UniformTypeIdentifiers

/// One row in a pane: a file or folder on this Mac or on the other Mac.
struct FileItem: Identifiable, Hashable, Sendable {
    enum Location: Hashable, Sendable {
        case local(URL)
        case remote
    }

    /// Unique within a pane: `L:<path>` for local items, `R:<components>` for remote ones.
    let id: String
    let name: String
    let isFolder: Bool
    /// `nil` for folders (shown as "--", like Finder).
    let size: Int64?
    let modified: Date?
    /// Finder's "Kind" text, e.g. "PDF document".
    let kind: String
    let contentType: UTType
    /// Path relative to the pane's root folder.
    let components: [String]
    let location: Location

    /// Folders sort below every file when sorting by size.
    var sizeSortKey: Int64 { size ?? -1 }
    var dateSortKey: Date { modified ?? .distantPast }
}

/// One clickable segment of a pane's path bar.
struct PathSegment: Identifiable, Hashable {
    /// 0 is the pane's root folder.
    let id: Int
    let title: String
    let components: [String]
}
