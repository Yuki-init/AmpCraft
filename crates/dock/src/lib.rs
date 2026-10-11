//! Docking math: panel graph, edge snapping and group drag. Pure, no I/O.
//!
//! The platform layer reports pointer positions and screen work areas; this crate decides
//! where every panel goes. See [`Layout`] for the classic rules it implements.

pub mod geom;
pub mod layout;
pub mod snap;

pub use geom::{Point, Rect, Side, Size, attached_side};
pub use layout::{Drag, Layout, PanelId};
pub use snap::{DEFAULT_SNAP_DISTANCE, snap_offset};
