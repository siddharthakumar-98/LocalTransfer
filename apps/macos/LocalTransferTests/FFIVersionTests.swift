import XCTest
@testable import LocalTransfer

final class FFIVersionTests: XCTestCase {
    /// The version fetched over UniFFI from the linked Rust library must match
    /// lt-core's Cargo manifest version, which build-rust.sh reads at build time.
    func testFFIVersionMatchesCrateVersion() {
        XCTAssertEqual(ltCoreVersion(), RustBuildInfo.ltCoreCrateVersion)
    }
}
