//! Platform-neutral starting point for Doremi.
//!
//! The domain and queue reducer perform no I/O. Hosts interpret returned
//! effects; provider and platform types never enter the reducer.
//!
//! ```
//! assert_eq!(doremi_core::PRODUCT_NAME, "Doremi");
//! assert!(!doremi_core::VERSION.is_empty());
//! ```

#![forbid(unsafe_code)]

pub mod media;
pub mod queue;

/// The new product name, deliberately separate from the legacy executable.
pub const PRODUCT_NAME: &str = "Doremi";

/// Baseline version, not a claim that a Doremi release has been published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
