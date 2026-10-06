import Foundation
import Observation

/// State of one pane: the folder shown, its rows, expansion, sorting and
/// back/forward history. UI-free so it can be unit tested.
@MainActor
@Observable
final class PaneModel {
    let source: any FileSource

    /// The folder shown, relative to the source's root.
    private(set) var path: [String] = []
    private(set) var items: [FileItem] = []
    private(set) var isLoading = false
    private(set) var errorMessage: String?
    var selection: Set<FileItem.ID> = []

    /// Column sort; Name ascending by default, as in Finder.
    var sortOrder: [KeyPathComparator<FileItem>] = [PaneModel.byName] {
        didSet { resort() }
    }

    private var backStack: [[String]] = []
    private var forwardStack: [[String]] = []
    private var expanded: Set<FileItem.ID> = []
    private var childCache: [FileItem.ID: [FileItem]] = [:]
    /// Bumped on navigation so late results for an old folder are dropped.
    private var generation = 0

    static let byName = KeyPathComparator(\FileItem.name, comparator: .localizedStandard)

    init(source: any FileSource) {
        self.source = source
    }

    var canGoBack: Bool { !backStack.isEmpty }
    var canGoForward: Bool { !forwardStack.isEmpty }

    var pathSegments: [PathSegment] {
        [PathSegment(id: 0, title: source.rootName, components: [])]
            + path.indices.map { PathSegment(id: $0 + 1, title: path[$0], components: Array(path[...$0])) }
    }

    // MARK: Loading

    /// (Re)loads the current folder, keeping expanded folders expanded.
    func load() async {
        generation += 1
        let current = generation
        isLoading = true
        do {
            let listed = try await source.list(path)
            guard current == generation else { return }
            items = sorted(listed)
            errorMessage = nil
            let stillExpanded = expanded
            childCache = childCache.filter { stillExpanded.contains($0.key) }
            for item in allLoadedItems() where expanded.contains(item.id) {
                await loadChildren(of: item)
            }
        } catch {
            guard current == generation else { return }
            items = []
            errorMessage = userMessage(for: error)
        }
        if current == generation { isLoading = false }
    }

    /// Reloads whatever shows `components`: the current folder or an expanded one.
    func refresh(changed components: [String]) async {
        if components == path {
            await load()
        } else if let item = allLoadedItems().first(where: { $0.components == components }),
                  expanded.contains(item.id) {
            await loadChildren(of: item)
        }
    }

    // MARK: Navigation

    func open(_ item: FileItem) async {
        guard item.isFolder else { return }
        await navigate(to: item.components)
    }

    func navigate(to components: [String]) async {
        guard components != path else { return }
        backStack.append(path)
        forwardStack.removeAll()
        await show(components)
    }

    func goBack() async {
        guard let previous = backStack.popLast() else { return }
        forwardStack.append(path)
        await show(previous)
    }

    func goForward() async {
        guard let next = forwardStack.popLast() else { return }
        backStack.append(path)
        await show(next)
    }

    private func show(_ components: [String]) async {
        path = components
        items = []
        expanded = []
        childCache = [:]
        selection = []
        await load()
    }

    // MARK: Disclosure

    func isExpanded(_ item: FileItem) -> Bool {
        expanded.contains(item.id)
    }

    /// Expanding loads the folder's contents on first use.
    func setExpanded(_ item: FileItem, _ isExpanded: Bool) {
        guard item.isFolder else { return }
        if isExpanded {
            expanded.insert(item.id)
            if childCache[item.id] == nil {
                Task { await loadChildren(of: item) }
            }
        } else {
            expanded.remove(item.id)
        }
    }

    func children(of item: FileItem) -> [FileItem] {
        childCache[item.id] ?? []
    }

    func loadChildren(of item: FileItem) async {
        let current = generation
        do {
            let listed = try await source.list(item.components)
            guard current == generation else { return }
            childCache[item.id] = sorted(listed)
        } catch {
            guard current == generation else { return }
            childCache[item.id] = []
            expanded.remove(item.id)
        }
    }

    /// Finds a row anywhere in the visible tree.
    func item(withID id: FileItem.ID) -> FileItem? {
        allLoadedItems().first { $0.id == id }
    }

    // MARK: Sorting

    private func sorted(_ list: [FileItem]) -> [FileItem] {
        // Ties (same size, same kind, …) fall back to Finder's name order.
        list.sorted(using: sortOrder + [Self.byName])
    }

    private func resort() {
        items = sorted(items)
        childCache = childCache.mapValues(sorted)
    }

    private func allLoadedItems() -> [FileItem] {
        items + childCache.values.flatMap { $0 }
    }
}
