//! The forms, grouped the way the nav groups them.
//!
//! Each file is one area of the shell rather than one page: a page is a list in [`crate::pages`], and two of
//! them that configure the same thing — the network page and the Bluetooth page both being one form over one
//! service — would otherwise be two files of fifty lines.

pub mod appearance;
pub mod applications;
pub mod audio;
pub mod bars;
pub mod lock;
pub mod notifications;
pub mod system;
pub mod wallpaper;
