//! Static Enigma unpacking and virtual-machine analysis.
#![deny(unsafe_code)]

#[cfg(target_arch = "wasm32")]
macro_rules! eprintln {
    ($($arg:tt)*) => {
        crate::common::io::browser_log(&format!($($arg)*))
    };
}

#[cfg(any(target_arch = "wasm32", test))]
pub mod browser;
pub mod common;
pub mod devirt;
pub mod evb;
pub mod unpack;
