//! Platform traits and the X11 backend.

/// Name of this crate, used in diagnostics.
pub const CRATE_NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_set() {
        assert_eq!(CRATE_NAME, "amp-platform");
    }
}
