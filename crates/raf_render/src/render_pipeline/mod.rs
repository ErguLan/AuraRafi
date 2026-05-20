//! Core render pipeline: CPU scanline rasterizer with Z-buffer.
//!
//! This module contains the actual rendering engine:
//! - Framebuffer (color + depth buffer, resizable)
//! - Scanline rasterizer with per-pixel depth testing
//!
//! Contract: geometry in (transformed vertices) -> pixels out (RGBA buffer).

pub mod framebuffer;
pub mod rasterizer;
