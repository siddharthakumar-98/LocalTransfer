import SwiftUI

/// The main window: "This Mac" on the left, the other Mac on the right.
struct ContentView: View {
    let backend: CoreBackend

    private let coreVersion = ltCoreVersion()

    var body: some View {
        let peer = backend.peer()
        VStack(spacing: 0) {
            HSplitView {
                PaneView(
                    title: "This Mac",
                    subtitle: "~/Desktop",
                    systemImage: "laptopcomputer"
                )
                PaneView(
                    title: peer.deviceName,
                    subtitle: backend.connection().label,
                    systemImage: "laptopcomputer.and.arrow.down"
                )
            }
            Divider()
            HStack {
                Spacer()
                Text("lt-core \(coreVersion)")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("coreVersion")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
        }
    }
}

/// One side of the window. The Finder-style file list arrives in M1.3.
struct PaneView: View {
    let title: String
    let subtitle: String
    let systemImage: String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: systemImage)
                    .font(.title2)
                    .foregroundStyle(.secondary)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.headline)
                    Text(subtitle).font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(10)
            Divider()
            ContentUnavailableView(
                "No files yet",
                systemImage: "folder",
                description: Text("The file browser arrives in M1.3.")
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(minWidth: 320)
    }
}

extension ConnectionState {
    /// Short text for the connection status under the peer's name.
    var label: String {
        switch self {
        case .connected(let quality):
            return quality == .good ? "Connected" : "Connected (weak signal)"
        case .connecting:
            return "Connecting…"
        case .disconnected(let reason):
            return "Not connected: \(reason)"
        }
    }
}
