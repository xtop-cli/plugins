//! Load a `.wasm` widget and print its manifest and a synthetic draw list as
//! JSON. This is the no-terminal end-to-end check for guests.
//!
//! ```text
//! cargo run -p xtop-plugin-wasm --example inspect -- path/to/widget.wasm
//! ```

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: inspect <widget.wasm>");
        std::process::exit(2);
    };
    match xtop_plugin_wasm::inspect(std::path::Path::new(&path)) {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("encode error: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}
