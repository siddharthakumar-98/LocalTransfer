import Foundation
import Observation

/// The app's view of the backend: peer identity, live connection state, and
/// events delivered on the main actor.
@MainActor
@Observable
final class BackendClient {
    let backend: CoreBackend
    let peer: PeerInfo
    private(set) var connection: ConnectionState

    /// Called when the peer's folder at these components changed.
    var onRemoteChanged: (([String]) -> Void)?

    @ObservationIgnored private var forwarder: EventForwarder?

    init(backend: CoreBackend) {
        self.backend = backend
        peer = backend.peer()
        connection = backend.connection()
        let forwarder = EventForwarder { [weak self] event in
            Task { @MainActor in self?.handle(event) }
        }
        self.forwarder = forwarder
        backend.setEventListener(listener: forwarder)
    }

    func handle(_ event: Event) {
        switch event {
        case .connection(let state):
            connection = state
        case .remoteChanged(let path):
            onRemoteChanged?(path.components)
        default:
            break // Transfer events are handled from M1.4.
        }
    }
}

/// Receives events on a Rust runtime thread and hands them to a closure.
final class EventForwarder: EventListener, @unchecked Sendable {
    private let handler: @Sendable (Event) -> Void

    init(_ handler: @escaping @Sendable (Event) -> Void) {
        self.handler = handler
    }

    func onEvent(event: Event) {
        handler(event)
    }
}

extension ConnectionState {
    /// Short text for the connection status under the peer's name.
    var label: String {
        switch self {
        case .connected(let quality):
            quality == .good ? "Connected" : "Connected (weak signal)"
        case .connecting:
            "Reconnecting…"
        case .disconnected(let reason):
            "Not connected: \(reason)"
        }
    }
}
