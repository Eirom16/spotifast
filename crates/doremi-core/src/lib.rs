//! Platform-neutral starting point for Doremi.
//!
//! F0 defines identity only. Queue, catalog and playback contracts will be
//! extracted with regression tests, not copied from the Spotify application.
//!
//! ```
//! assert_eq!(doremi_core::PRODUCT_NAME, "Doremi");
//! assert!(!doremi_core::VERSION.is_empty());
//! ```

#![forbid(unsafe_code)]

/// The new product name, deliberately separate from the legacy executable.
pub const PRODUCT_NAME: &str = "Doremi";

/// Baseline version, not a claim that a Doremi release has been published.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
