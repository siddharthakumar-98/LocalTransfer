import SwiftUI

/// One side of the window: header with back/forward, the file list, and the path bar.
struct PaneView: View {
    @Bindable var model: PaneModel
    let title: String
    let subtitle: String
    let systemImage: String
    let identifier: String

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            FileTable(model: model)
                .overlay { placeholder }
            Divider()
            PathBar(segments: model.pathSegments) { segment in
                Task { await model.navigate(to: segment.components) }
            }
        }
        .frame(minWidth: 360)
        .accessibilityIdentifier(identifier)
        .task { await model.load() }
    }

    private var header: some View {
        HStack(spacing: 8) {
            ControlGroup {
                Button {
                    Task { await model.goBack() }
                } label: {
                    Image(systemName: "chevron.left")
                }
                .disabled(!model.canGoBack)
                .help("Back")
                .accessibilityIdentifier("\(identifier)-back")

                Button {
                    Task { await model.goForward() }
                } label: {
                    Image(systemName: "chevron.right")
                }
                .disabled(!model.canGoForward)
                .help("Forward")
                .accessibilityIdentifier("\(identifier)-forward")
            }
            .controlGroupStyle(.navigation)
            .fixedSize()

            Image(systemName: systemImage)
                .font(.title2)
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.headline)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            if model.isLoading {
                ProgressView().controlSize(.small)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private var placeholder: some View {
        if let message = model.errorMessage {
            ContentUnavailableView(
                "Can't show this folder",
                systemImage: "exclamationmark.triangle",
                description: Text(message))
        } else if model.items.isEmpty && !model.isLoading {
            ContentUnavailableView("Empty folder", systemImage: "folder")
        }
    }
}
