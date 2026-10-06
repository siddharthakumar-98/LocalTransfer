//! UniFFI's Swift bindgen, built from this workspace so its version always
//! matches the `uniffi` proc-macro used by `lt-ffi`. Called by
//! `scripts/build-rust.sh`.

#![forbid(unsafe_code)]

fn main() {
    uniffi::uniffi_bindgen_swift();
}
