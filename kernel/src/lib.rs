//! Syndicate kernel: price discovery for assets that do not trade.
//!
//! The crate has no dependencies and builds two ways from one source: a
//! native library and CLI used for calibration and backtesting, and a
//! WebAssembly module the browser calls directly.

pub mod calib;
pub mod data;
pub mod engine;
pub mod eval;
#[cfg(not(target_arch = "wasm32"))]
pub mod export;
pub mod filter;
pub mod insight;
pub mod json;
pub mod liquidity;
pub mod quote;
pub mod stats;
pub mod truth;
pub mod wasm;
