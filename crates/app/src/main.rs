//! Player binary. Wiring only; logic lives in the library crates.

fn main() {
    println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
}
