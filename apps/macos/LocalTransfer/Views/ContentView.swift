import SwiftUI

/// The main window: "This Mac" on the left, the other Mac on the right.
struct ContentView: View {
    @State private var client: BackendClient
    @State private var localPane: PaneModel
    @State private var remotePane: PaneModel

    private let coreVersion = ltCoreVersion()
    private let machineName = MachineName.current

    init(backend: CoreBackend, localRoot: URL) {
        _client = State(initialValue: BackendClient(backend: backend))
        _localPane = State(initialValue: PaneModel(source: LocalFileSource(root: localRoot)))
        _remotePane = State(initialValue: PaneModel(source: RemoteFileSource(backend: backend)))
    }

    var body: some View {
        VStack(spacing: 0) {
            HSplitView {
                PaneView(
                    model: localPane,
                    title: "This Mac",
                    subtitle: "\(machineName) · ~/Desktop",
                    systemImage: "laptopcomputer",
                    identifier: "localPane")
                PaneView(
                    model: remotePane,
                    title: "Other Mac",
                    subtitle: "\(client.peer.deviceName) · \(client.connection.label)",
                    systemImage: "laptopcomputer.and.arrow.down",
                    identifier: "remotePane")
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
        .onAppear {
            client.onRemoteChanged = { components in
                Task { await remotePane.refresh(changed: components) }
            }
        }
    }
}
