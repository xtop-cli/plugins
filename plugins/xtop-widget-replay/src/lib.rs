//! Host-side utilities for runtime widgets: replay a [`DrawList`] onto a
//! ratatui frame and build the serializable [`State`] a guest receives.
//!
//! Both runtime widget hosts (`xtop-plugin-wasm`, `xtop-plugin-external`)
//! depend on this crate; guests never do.

mod replay;
mod state;

pub use replay::replay;
pub use state::{snapshot_to_contract, state_from_context};
