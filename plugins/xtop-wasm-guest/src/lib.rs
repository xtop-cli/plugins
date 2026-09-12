//! Guest SDK for xtop runtime widgets written in Rust.
//!
//! A guest is a `cdylib` compiled to `wasm32-unknown-unknown` that exports
//! the small ABI `xtop-plugin-wasm` expects. [`export_widget!`] generates
//! every export; you only write a manifest and a render function:
//!
//! ```ignore
//! use xtop_wasm_contract::{DrawList, Manifest, Op, Rect, Span, State};
//! use xtop_wasm_guest::export_widget;
//!
//! export_widget! {
//!     manifest: || Manifest {
//!         name: "wasm-hello".to_string(),
//!         version: env!("CARGO_PKG_VERSION").to_string(),
//!         description: "hello from wasm".to_string(),
//!         ..Manifest::default()
//!     },
//!     render: |state: &State| {
//!         let mut list = DrawList::new();
//!         list.push(Op::Text {
//!             rect: Rect::full(state.width, state.height),
//!             spans: vec![Span::new(format!("tick {}", state.tick))],
//!             align: Align::Left,
//!             wrap: false,
//!         });
//!         list
//!     },
//! }
//! ```
//!
//! Build with:
//!
//! ```text
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! # ABI
//!
//! | Export | Signature | Meaning |
//! |---|---|---|
//! | `alloc` | `(i32) -> i32` | allocate `len` bytes in guest memory |
//! | `dealloc` | `(i32, i32) -> ()` | free a buffer returned by `alloc` |
//! | `manifest` | `() -> i32` | write the manifest JSON, return its pointer |
//! | `render` | `(i32, i32) -> i32` | parse the state JSON at `(ptr, len)`, write the draw-list JSON, return its pointer (0 on bad input) |
//! | `result_len` | `() -> i32` | length of the JSON written by the last call |
//!
//! The host reads `result_len()` immediately after `manifest()`/`render()`.
//! A guest that needs scratch state across ticks keeps it in a `static`.

pub use xtop_wasm_contract as contract;
pub use xtop_wasm_contract::{
    Align, Border, Color, Dataset, DrawList, Manifest, Marker, Op, Rect, Span, State,
};

/// Implementation details used by [`export_widget!`]; not public API.
#[doc(hidden)]
pub mod __private {
    pub use serde;
    pub use serde_json;
}

/// Export the WASM widget ABI for a manifest and a render function.
///
/// Both arguments are expressions: `manifest` is a `Fn() -> Manifest` and
/// `render` is a `Fn(&State) -> DrawList`. They are stored as `fn` pointers
/// in the caller's scope (so they see the caller's imports and statics) and
/// the ABI exports call them.
#[macro_export]
macro_rules! export_widget {
    (
        manifest: $manifest:expr,
        render: $render:expr $(,)?
    ) => {
        #[doc(hidden)]
        static __XTOP_WIDGET_MANIFEST: fn() -> $crate::contract::Manifest = $manifest;
        #[doc(hidden)]
        static __XTOP_WIDGET_RENDER: fn(&$crate::contract::State) -> $crate::contract::DrawList =
            $render;

        #[allow(static_mut_refs)]
        mod __xtop_widget_exports {
            static mut RESULT: Vec<u8> = Vec::new();

            /// Allocate a host-owned buffer inside guest memory.
            #[no_mangle]
            pub extern "C" fn alloc(len: i32) -> i32 {
                if len <= 0 {
                    return 0;
                }
                let mut buf = vec![0u8; len as usize];
                let ptr = buf.as_mut_ptr() as i32;
                core::mem::forget(buf);
                ptr
            }

            /// Free a buffer previously returned by `alloc`.
            #[no_mangle]
            pub extern "C" fn dealloc(ptr: i32, len: i32) {
                if ptr == 0 || len <= 0 {
                    return;
                }
                unsafe {
                    drop(Vec::from_raw_parts(
                        ptr as *mut u8,
                        len as usize,
                        len as usize,
                    ));
                }
            }

            /// Length of the JSON payload written by the last ABI call.
            #[no_mangle]
            pub extern "C" fn result_len() -> i32 {
                unsafe { RESULT.len() as i32 }
            }

            /// Write the widget manifest and return its pointer.
            #[no_mangle]
            pub extern "C" fn manifest() -> i32 {
                let value = super::__XTOP_WIDGET_MANIFEST();
                store(&value)
            }

            /// Parse the state JSON, render, and return the draw-list pointer.
            #[no_mangle]
            pub extern "C" fn render(state_ptr: i32, state_len: i32) -> i32 {
                if state_ptr == 0 || state_len <= 0 {
                    return 0;
                }
                let bytes = unsafe {
                    core::slice::from_raw_parts(state_ptr as *const u8, state_len as usize)
                };
                let state = match $crate::__private::serde_json::from_slice::<$crate::contract::State>(bytes) {
                    Ok(state) => state,
                    Err(_) => return 0,
                };
                let value = super::__XTOP_WIDGET_RENDER(&state);
                store(&value)
            }

            fn store<T: $crate::__private::serde::Serialize>(value: &T) -> i32 {
                let json = match $crate::__private::serde_json::to_vec(value) {
                    Ok(json) => json,
                    Err(_) => return 0,
                };
                unsafe {
                    RESULT = json;
                    RESULT.as_ptr() as i32
                }
            }
        }
    };
}
