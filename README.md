# LocalTransfer

LocalTransfer sends files between your Macs over the local network, in both directions, with **no cloud and no accounts**. Pair two Macs once with a short code. After that they find each other automatically over Bonjour and transfer over end-to-end encrypted QUIC, verifying every byte with BLAKE3. It has a Rust core, a CLI called `lt`, and (planned) a native SwiftUI app with a Finder-style two-pane browser and a menu-bar drop target. It's built for Apple Silicon Macs.

> **Status:** design phase. Nothing is runnable yet. See the milestones below.

## Key features
- Zero-config discovery on the LAN (Bonjour), with BLE and AWDL fallbacks planned
- Send and pull in both directions; browse the other Mac's shared folders
- Resumable, chunked transfers that continue after Wi-Fi drops or sleep
- `~/Desktop ↔ ~/Desktop` by default (configurable)
- CLI now; SwiftUI app with drag-and-drop and a menu-bar agent later

## Security summary
- **Pairing:** a 6-digit, single-use code with SPAKE2, bound to the TLS session (no MITM, no offline guessing)
- **Authentication:** mutual TLS 1.3 with **pinned Ed25519 device keys** stored in the macOS Keychain. Unknown devices are rejected.
- **Integrity:** BLAKE3 per chunk and per file, verified before anything is committed
- **Filesystem safety:** paths are confined to shared roots via `cap-std` (no traversal, no symlink escapes). Writes are atomic (temp file, fsync, verify, no-replace rename). Existing files are **never overwritten**; a clash becomes `name (1).ext`.
- **Rust hygiene:** `#![forbid(unsafe_code)]` outside isolated FFI crates, clippy pedantic, cargo-audit, cargo-deny and cargo-fuzz

## Planned CLI
```
lt pair                          # pair this Mac with another
lt peers                         # list paired Macs
lt serve                         # receive in the foreground (until the app/agent lands)
lt send report.pdf air           # → air:~/Desktop/report.pdf
lt ls air:~/Desktop
lt get air:~/Desktop/x.zip
```

## Milestones
| Milestone | Status |
|---|---|
| M1: CLI send over LAN with pairing | ⏳ Not started |
| M2: Remote listing, pull, folders, resume | ⏳ Not started |
| M3: SwiftUI GUI, menu bar, background agent | ⏳ Not started |
| M4: BLE presence, AWDL, content-addressed skip | ⏳ Not started |

Full design, wire protocol and test plan: **[ROADMAP.md](ROADMAP.md)**.

## License
Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
