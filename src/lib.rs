pub mod batch;
pub mod clipboard;
pub mod gemini;
pub mod prompt;
pub mod tui;

// re-exports para uso externo

pub use clipboard::ClipboardManager;
pub use gemini::{ListingRequest, ShopeeListing, generate_listing};
