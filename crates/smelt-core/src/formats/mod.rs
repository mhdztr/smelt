//! Native format converters. One module per format; each pairs a `Reader`
//! and a `Writer` over the IDM. Registration happens in
//! `crate::{reader_for, writer_for}`.

pub mod html;
pub mod markdown;
pub mod text;
