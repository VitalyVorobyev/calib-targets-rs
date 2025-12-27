//! Embedded built-in dictionaries.
//!
//! The source-of-truth lives in `calib-targets-aruco/data/*_CODES.json`.

#![allow(clippy::unreadable_literal, non_upper_case_globals)]

include!(concat!(env!("OUT_DIR"), "/builtins.rs"));
