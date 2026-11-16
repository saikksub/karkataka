//! Karkataka: video editing software that AI agents and people drive from the command line.
//!
//! - `cli`: the `karka` command.
//! - `engine`: renders a web page frame by frame in offscreen Chromium (CEF), to raw pixels.
//! - `encode`: pipes raw frames into FFmpeg.
//! - `audio`: mixes audio clips and muxes them into the video.

pub mod audio;
pub mod cli;
pub mod encode;
pub mod engine;
pub mod files;
pub mod project;
pub mod render;
