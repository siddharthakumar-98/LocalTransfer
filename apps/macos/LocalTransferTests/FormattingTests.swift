import XCTest
import UniformTypeIdentifiers
@testable import LocalTransfer

final class FormattingTests: XCTestCase {
    // MARK: Size

    func testSizeColumn() {
        let folder = fakeFolder("f", daysAgo: 0)
        XCTAssertEqual(FileFormat.size(folder), "--")
        XCTAssertEqual(FileFormat.size(bytes: 0), "Zero bytes")
        XCTAssertEqual(FileFormat.size(bytes: 999), "999 bytes")
        XCTAssertEqual(FileFormat.size(bytes: 2_000), "2 KB")
        XCTAssertEqual(FileFormat.size(bytes: 1_234_567), "1.2 MB")
        XCTAssertEqual(FileFormat.size(bytes: 4_500_000_000), "4.5 GB")
    }

    // MARK: Date Modified

    private func formatter() -> FinderDateFormatter {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return FinderDateFormatter(
            calendar: calendar, locale: Locale(identifier: "en_US"), now: { referenceDate })
    }

    /// Recent ICU puts a narrow no-break space before AM/PM.
    private func plain(_ s: String) -> String {
        s.replacingOccurrences(of: "\u{202F}", with: " ")
    }

    func testDateModifiedColumn() {
        let f = formatter()
        XCTAssertEqual(plain(f.string(from: referenceDate.addingTimeInterval(-3_600))), "Today at 11:12 AM")
        XCTAssertEqual(plain(f.string(from: referenceDate.addingTimeInterval(-86_400))), "Yesterday at 12:12 PM")
        XCTAssertEqual(
            plain(f.string(from: Date(timeIntervalSince1970: 1_740_816_000))), // 2025-03-01 08:00 UTC
            "Mar 1, 2025 at 8:00 AM")
        XCTAssertEqual(f.string(from: nil), "--")
    }

    // MARK: Type, Kind and icon

    func testContentTypeFromName() {
        XCTAssertEqual(FileTypes.contentType(forName: "Photos", isFolder: true), .folder)
        XCTAssertEqual(FileTypes.contentType(forName: "Report.pdf", isFolder: false), .pdf)
        XCTAssertEqual(FileTypes.contentType(forName: "Shot.PNG", isFolder: false), .png)
        XCTAssertEqual(FileTypes.contentType(forName: "Backup.zip", isFolder: false), .zip)
        let unknown = FileTypes.contentType(forName: "data.unknownext", isFolder: false)
        XCTAssertTrue(unknown.isDynamic)
        XCTAssertTrue(unknown.conforms(to: .data))
        XCTAssertEqual(FileTypes.contentType(forName: "README", isFolder: false), .data)
    }

    func testKindText() {
        XCTAssertEqual(FileTypes.kind(forName: "Photos", isFolder: true), "Folder")
        XCTAssertEqual(FileTypes.kind(forName: "Report.pdf", isFolder: false), UTType.pdf.localizedDescription)
        XCTAssertEqual(FileTypes.kind(forName: "Shot.png", isFolder: false), UTType.png.localizedDescription)
        XCTAssertEqual(FileTypes.kind(forName: "Backup.zip", isFolder: false), UTType.zip.localizedDescription)
        XCTAssertEqual(FileTypes.kind(forName: "data.unknownext", isFolder: false), "Document")
        XCTAssertTrue(FileTypes.kind(forName: "Report.pdf", isFolder: false).contains("PDF"))
    }

    @MainActor
    func testEveryRowGetsARealIcon() {
        let items = [
            fakeFolder("Photos", daysAgo: 0),
            fakeFile("Report.pdf", size: 1, daysAgo: 0, kind: ""),
            fakeFile("Shot.png", size: 1, daysAgo: 0, kind: ""),
            fakeFile("Backup.zip", size: 1, daysAgo: 0, kind: ""),
            fakeFile("data.unknownext", size: 1, daysAgo: 0, kind: ""),
        ]
        let icons = items.map { FileTypes.icon(for: $0) }
        for (item, icon) in zip(items, icons) {
            XCTAssertGreaterThan(icon.size.width, 0, item.name)
            XCTAssertFalse(icon.representations.isEmpty, item.name)
        }
        XCTAssertNotEqual(icons[0].tiffRepresentation, icons[1].tiffRepresentation, "folder vs PDF")
        XCTAssertNotEqual(icons[1].tiffRepresentation, icons[2].tiffRepresentation, "PDF vs PNG")
    }
}
