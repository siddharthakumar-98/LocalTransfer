import SwiftUI

/// Finder's list view: Name, Date Modified, Size and Kind, sortable, with
/// folders that expand in place.
struct FileTable: View {
    @Bindable var model: PaneModel

    var body: some View {
        Table(of: FileItem.self, selection: $model.selection, sortOrder: $model.sortOrder) {
            TableColumn("Name", value: \.name, comparator: .localizedStandard) { item in
                NameCell(item: item)
            }
            .width(min: 160, ideal: 240)

            TableColumn("Date Modified", value: \.dateSortKey) { item in
                Text(FileFormat.date(item.modified))
                    .foregroundStyle(.secondary)
            }
            .width(min: 110, ideal: 160)

            TableColumn("Size", value: \.sizeSortKey) { item in
                Text(FileFormat.size(item))
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .width(min: 56, ideal: 72)

            TableColumn("Kind", value: \.kind, comparator: .localizedStandard) { item in
                Text(item.kind)
                    .foregroundStyle(.secondary)
            }
            .width(min: 80, ideal: 120)
        } rows: {
            FileRows(items: model.items, model: model)
        }
        .contextMenu(forSelectionType: FileItem.ID.self) { _ in
            EmptyView()
        } primaryAction: { ids in
            // Double-click (or Return) on a folder opens it.
            guard ids.count == 1, let id = ids.first, let item = model.item(withID: id) else { return }
            if item.isFolder {
                Task { await model.open(item) }
            }
        }
    }
}

/// Rows for one folder level; folders recurse into their (lazily loaded) children.
struct FileRows: TableRowContent {
    typealias TableRowValue = FileItem

    let items: [FileItem]
    let model: PaneModel

    var tableRowBody: some TableRowContent<FileItem> {
        ForEach(items) { item in
            if item.isFolder {
                DisclosureTableRow(item, isExpanded: expansion(of: item)) {
                    FileRows(items: model.children(of: item), model: model)
                }
            } else {
                TableRow(item)
            }
        }
    }

    private func expansion(of item: FileItem) -> Binding<Bool> {
        Binding(
            get: { model.isExpanded(item) },
            set: { model.setExpanded(item, $0) })
    }
}

private struct NameCell: View {
    let item: FileItem

    var body: some View {
        HStack(spacing: 6) {
            Image(nsImage: FileTypes.icon(for: item))
                .resizable()
                .interpolation(.high)
                .frame(width: 16, height: 16)
            Text(item.name)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .help(item.name)
    }
}
