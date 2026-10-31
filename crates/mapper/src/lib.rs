//! Mapping engine for the Speed Editor.
//!
//! Everything here is pure logic: device reports go in, [`Output`]s come out.
//! The app turns outputs into real key presses, program launches and LED
//! updates, which keeps this crate testable without hardware.

pub mod davinci_wheel;
pub mod engine;
pub mod keyboard;
pub mod native;
pub mod profile;
pub mod resolve;

pub use engine::{Engine, Mode, Output};
pub use profile::{Action, ActionDoc, KeyDoc, Mapping, PageDoc, Profile, ProfileDoc, ProfileError, Wheel, WheelDoc};
