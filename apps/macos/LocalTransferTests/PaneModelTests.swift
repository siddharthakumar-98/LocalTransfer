import XCTest
@testable import LocalTransfer

@MainActor
final class PaneModelTests: XCTestCase {
    private func makeModel() -> (PaneModel, FakeSource) {
        let source = FakeSource(
            tree: [
                []: [
                    fakeFile("File 10.txt", size: 10, daysAgo: 3, kind: "Plain Text"),
                    fakeFile("file 2.txt", size: 2_000, daysAgo: 1, kind: "Plain Text"),
                    fakeFile("Apple.pdf", size: 500, daysAgo: 2, kind: "PDF document"),
                    fakeFolder("banana", daysAgo: 4),
                    fakeFile("Zeta.png", size: 50, daysAgo: 0, kind: "PNG image"),
                    fakeFolder("broken", daysAgo: 5),
                ],
                ["banana"]: [
                    fakeFile("z.md", in: ["banana"], size: 1, daysAgo: 1, kind: "Markdown"),
                    fakeFolder("inner", in: ["banana"], daysAgo: 1),
                    fakeFile("a.md", in: ["banana"], size: 2, daysAgo: 2, kind: "Markdown"),
                ],
                ["banana", "inner"]: [],
            ],
            failing: [["broken"]])
        return (PaneModel(source: source), source)
    }

    private func names(_ model: PaneModel) -> [String] { model.items.map(\.name) }

    // MARK: Sorting

    func testDefaultSortIsFinderNameOrder() async {
        let (model, _) = makeModel()
        await model.load()
        // Case-insensitive, numbers compared numerically, folders mixed in.
        XCTAssertEqual(names(model), ["Apple.pdf", "banana", "broken", "file 2.txt", "File 10.txt", "Zeta.png"])
        model.sortOrder = [KeyPathComparator(\FileItem.name, comparator: .localizedStandard, order: .reverse)]
        XCTAssertEqual(names(model), ["Zeta.png", "File 10.txt", "file 2.txt", "broken", "banana", "Apple.pdf"])
    }

    func testSortByDateModifiedBothWays() async {
        let (model, _) = makeModel()
        await model.load()
        model.sortOrder = [KeyPathComparator(\FileItem.dateSortKey)]
        XCTAssertEqual(names(model), ["broken", "banana", "File 10.txt", "Apple.pdf", "file 2.txt", "Zeta.png"])
        model.sortOrder = [KeyPathComparator(\FileItem.dateSortKey, order: .reverse)]
        XCTAssertEqual(names(model), ["Zeta.png", "file 2.txt", "Apple.pdf", "File 10.txt", "banana", "broken"])
    }

    func testSortBySizeBothWaysWithFoldersAtTheSmallEnd() async {
        let (model, _) = makeModel()
        await model.load()
        model.sortOrder = [KeyPathComparator(\FileItem.sizeSortKey)]
        XCTAssertEqual(names(model), ["banana", "broken", "File 10.txt", "Zeta.png", "Apple.pdf", "file 2.txt"])
        model.sortOrder = [KeyPathComparator(\FileItem.sizeSortKey, order: .reverse)]
        // Equal sizes (the two folders) keep name order.
        XCTAssertEqual(names(model), ["file 2.txt", "Apple.pdf", "Zeta.png", "File 10.txt", "banana", "broken"])
    }

    func testSortByKindBothWaysWithNameTieBreak() async {
        let (model, _) = makeModel()
        await model.load()
        model.sortOrder = [KeyPathComparator(\FileItem.kind, comparator: .localizedStandard)]
        XCTAssertEqual(names(model), ["banana", "broken", "Apple.pdf", "file 2.txt", "File 10.txt", "Zeta.png"])
        model.sortOrder = [KeyPathComparator(\FileItem.kind, comparator: .localizedStandard, order: .reverse)]
        XCTAssertEqual(names(model), ["Zeta.png", "file 2.txt", "File 10.txt", "Apple.pdf", "banana", "broken"])
    }

    func testSortAppliesToExpandedChildren() async {
        let (model, _) = makeModel()
        await model.load()
        let banana = model.items.first { $0.name == "banana" }!
        await model.loadChildren(of: banana)
        XCTAssertEqual(model.children(of: banana).map(\.name), ["a.md", "inner", "z.md"])
        model.sortOrder = [KeyPathComparator(\FileItem.name, comparator: .localizedStandard, order: .reverse)]
        XCTAssertEqual(model.children(of: banana).map(\.name), ["z.md", "inner", "a.md"])
    }

    // MARK: Navigation history and path bar

    func testBackAndForward() async {
        let (model, _) = makeModel()
        await model.load()
        XCTAssertFalse(model.canGoBack)
        XCTAssertFalse(model.canGoForward)

        await model.open(model.items.first { $0.name == "banana" }!)
        XCTAssertEqual(model.path, ["banana"])
        XCTAssertEqual(names(model), ["a.md", "inner", "z.md"])
        XCTAssertTrue(model.canGoBack)

        await model.open(model.items.first { $0.name == "inner" }!)
        XCTAssertEqual(model.path, ["banana", "inner"])

        await model.goBack()
        XCTAssertEqual(model.path, ["banana"])
        XCTAssertTrue(model.canGoForward)
        await model.goBack()
        XCTAssertEqual(model.path, [])
        XCTAssertFalse(model.canGoBack)

        await model.goForward()
        XCTAssertEqual(model.path, ["banana"])
        await model.goForward()
        XCTAssertEqual(model.path, ["banana", "inner"])
        XCTAssertFalse(model.canGoForward)
    }

    func testNavigatingSomewhereNewClearsForward() async {
        let (model, _) = makeModel()
        await model.load()
        await model.navigate(to: ["banana"])
        await model.goBack()
        XCTAssertTrue(model.canGoForward)
        await model.navigate(to: ["banana", "inner"])
        XCTAssertFalse(model.canGoForward)
        await model.navigate(to: ["banana", "inner"]) // same folder: no history entry
        await model.goBack()
        XCTAssertEqual(model.path, [])
    }

    func testOpeningAFileDoesNothing() async {
        let (model, _) = makeModel()
        await model.load()
        await model.open(model.items.first { $0.name == "Apple.pdf" }!)
        XCTAssertEqual(model.path, [])
        XCTAssertFalse(model.canGoBack)
    }

    func testPathBarSegments() async {
        let (model, _) = makeModel()
        await model.load()
        XCTAssertEqual(model.pathSegments.map(\.title), ["Desktop"])

        await model.navigate(to: ["banana", "inner"])
        let segments = model.pathSegments
        XCTAssertEqual(segments.map(\.title), ["Desktop", "banana", "inner"])
        XCTAssertEqual(segments.map(\.components), [[], ["banana"], ["banana", "inner"]])
        XCTAssertEqual(segments.map(\.id), [0, 1, 2])

        // Clicking a segment navigates there and can be undone with Back.
        await model.navigate(to: segments[1].components)
        XCTAssertEqual(model.path, ["banana"])
        await model.goBack()
        XCTAssertEqual(model.path, ["banana", "inner"])
    }

    // MARK: Disclosure and errors

    func testExpandingLoadsChildrenLazily() async {
        let (model, source) = makeModel()
        await model.load()
        let banana = model.items.first { $0.name == "banana" }!
        XCTAssertEqual(source.listCount(["banana"]), 0, "nothing loaded before expanding")

        model.setExpanded(banana, true)
        XCTAssertTrue(model.isExpanded(banana))
        let loaded = await eventually { !model.children(of: banana).isEmpty }
        XCTAssertTrue(loaded)
        XCTAssertEqual(source.listCount(["banana"]), 1)

        model.setExpanded(banana, false)
        model.setExpanded(banana, true)
        XCTAssertEqual(source.listCount(["banana"]), 1, "cached after the first expand")
        XCTAssertEqual(model.item(withID: "F:banana/z.md")?.name, "z.md")
    }

    func testErrorsAreShownAsMessages() async {
        let (model, _) = makeModel()
        await model.load()
        await model.navigate(to: ["broken"])
        XCTAssertEqual(model.errorMessage, "Not connected to the other Mac.")
        XCTAssertTrue(model.items.isEmpty)
        await model.goBack()
        XCTAssertNil(model.errorMessage)
    }
}
