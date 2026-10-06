# LocalTransfer

LocalTransfer sends files between your Macs over the local network, in both directions, with **no cloud and no accounts**. Pair two Macs once with a short code. After that they find each other over Bonjour and transfer over end-to-end encrypted QUIC, verifying every byte with BLAKE3. It's a native SwiftUI app with a Finder-style two-pane browser, built on a Rust core that is shared with a CLI called `lt`. It's built for Apple Silicon Macs.

> **Status:** design phase. Nothing is runnable yet. The GUI comes first, against a mock peer (M1).

## Key features
- Finder-style two-pane browser, "This Mac" next to the other Mac, with drag-and-drop and live progress
- Zero-config discovery on the LAN (Bonjour), with a manual `--addr` fallback and `lt doctor` for when Wi-Fi gets in the way
- Send and pull in both directions; resumable transfers that continue after Wi-Fi drops or sleep
- `~/Desktop ↔ ~/Desktop` by default (configurable)

## Security summary
- **Pairing:** a 6-digit, single-use code with SPAKE2, bound to the TLS session (no MITM, no offline guessing)
- **Authentication:** mutual TLS 1.3 with **pinned Ed25519 device keys** stored in the macOS Keychain. Unknown devices are rejected.
- **Integrity:** BLAKE3 per chunk and per file, verified before anything is committed
- **Filesystem safety:** paths are confined to shared roots via `cap-std`. Writes are atomic (temp file, verify, no-replace rename). Existing files are **never overwritten**; a clash becomes `name (1).ext`. The mock peer writes only to a sandbox folder.
- **Rust hygiene:** `#![forbid(unsafe_code)]` outside isolated FFI crates, clippy pedantic, cargo-audit, cargo-deny and cargo-fuzz

## Planned CLI (arrives in M2)
```
lt pair                          # pair this Mac with another
lt send report.pdf air           # → air:~/Desktop/report.pdf
lt ls air:~/Desktop
lt get air:~/Desktop/x.zip
lt doctor air                    # why can't my Macs see each other?
```

## Milestones
| Milestone | Status |
|---|---|
| M1: GUI with a mock peer | 🚧 In progress (M1.1–M1.3 ✅) |
| M2: Real CLI transfer over the same Wi-Fi | ⏳ Not started |
| M3: GUI + real networking combined | ⏳ Not started |
| Later: BLE, AWDL, dedupe, menu-bar agent, Share extension | 💤 Shelved |

Full design, wire protocol, sub-steps and test plan: **[ROADMAP.md](ROADMAP.md)**.

## License
Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
