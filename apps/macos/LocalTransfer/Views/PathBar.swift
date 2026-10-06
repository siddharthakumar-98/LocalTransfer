import SwiftUI
import UniformTypeIdentifiers

/// Finder's path bar: the current folder's ancestors, each clickable.
struct PathBar: View {
    let segments: [PathSegment]
    let onSelect: (PathSegment) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 3) {
                ForEach(segments) { segment in
                    if segment.id > 0 {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 8, weight: .semibold))
                            .foregroundStyle(.tertiary)
                    }
                    Button {
                        onSelect(segment)
                    } label: {
                        HStack(spacing: 3) {
                            Image(nsImage: FileTypes.icon(for: UTType.folder))
                                .resizable()
                                .frame(width: 14, height: 14)
                            Text(segment.title)
                                .lineLimit(1)
                        }
                        .font(.caption)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("pathSegment-\(segment.id)")
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
        }
        .frame(height: 22)
    }
}
