//! Application lifecycle: the AppDelegate, panel ownership, hotkey wiring,
//! shared runtime state, and login-item management.

mod delegate;
pub mod hotkey;
pub mod login_item;
pub mod shared;

pub use delegate::AppDelegate;
