import AppKit
import UniformTypeIdentifiers

/// Type, Kind and icon lookup for rows.
enum FileTypes {
    /// The type implied by a name, the way Finder guesses it for files it
    /// can't inspect (remote files). Unknown extensions give a dynamic type
    /// that conforms to `.data`.
    static func contentType(forName name: String, isFolder: Bool) -> UTType {
        if isFolder { return .folder }
        let ext = (name as NSString).pathExtension
        guard !ext.isEmpty, let type = UTType(filenameExtension: ext) else { return .data }
        return type
    }

    /// Finder's "Kind" text: "Folder", "PDF document", … and "Document" for
    /// types the system can't describe.
    static func kind(for type: UTType) -> String {
        if type == .folder { return "Folder" }
        return type.localizedDescription ?? "Document"
    }

    static func kind(forName name: String, isFolder: Bool) -> String {
        kind(for: contentType(forName: name, isFolder: isFolder))
    }

    /// The icon Finder would show: the file's own icon for local items, the
    /// type's icon for remote ones.
    @MainActor
    static func icon(for item: FileItem) -> NSImage {
        switch item.location {
        case .local(let url):
            return IconCache.shared.icon(key: "L:" + url.path) {
                NSWorkspace.shared.icon(forFile: url.path)
            }
        case .remote:
            return icon(for: item.contentType)
        }
    }

    @MainActor
    static func icon(for type: UTType) -> NSImage {
        IconCache.shared.icon(key: "T:" + type.identifier) {
            NSWorkspace.shared.icon(for: type)
        }
    }
}

/// Rows re-render often; NSWorkspace builds a new image on every call.
@MainActor
private final class IconCache {
    static let shared = IconCache()
    private let cache = NSCache<NSString, NSImage>()

    func icon(key: String, make: () -> NSImage) -> NSImage {
        if let hit = cache.object(forKey: key as NSString) { return hit }
        let image = make()
        cache.setObject(image, forKey: key as NSString)
        return image
    }
}

/// Finder-style text for the Size and Date Modified columns.
enum FileFormat {
    static func size(_ item: FileItem) -> String {
        guard !item.isFolder, let bytes = item.size else { return "--" }
        return size(bytes: bytes)
    }

    static func size(bytes: Int64) -> String {
        bytes == 0 ? "Zero bytes" : ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
    }

    static func date(_ date: Date?) -> String {
        FinderDateFormatter.shared.string(from: date)
    }
}

/// "Today at 2:12 PM", "Yesterday at 9:05 AM", otherwise "Oct 6, 2026 at 2:12 PM".
struct FinderDateFormatter {
    static let shared = FinderDateFormatter()

    private let calendar: Calendar
    private let time: DateFormatter
    private let full: DateFormatter
    private let now: () -> Date

    init(calendar: Calendar = .current, locale: Locale = .current, now: @escaping () -> Date = Date.init) {
        self.calendar = calendar
        self.now = now
        time = DateFormatter()
        time.locale = locale
        time.timeZone = calendar.timeZone
        time.dateStyle = .none
        time.timeStyle = .short
        full = DateFormatter()
        full.locale = locale
        full.timeZone = calendar.timeZone
        full.dateStyle = .medium
        full.timeStyle = .short
    }

    func string(from date: Date?) -> String {
        guard let date else { return "--" }
        let today = now()
        if calendar.isDate(date, inSameDayAs: today) {
            return "Today at \(time.string(from: date))"
        }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: today),
           calendar.isDate(date, inSameDayAs: yesterday) {
            return "Yesterday at \(time.string(from: date))"
        }
        return full.string(from: date)
    }
}
