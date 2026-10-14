//! # gnative-gfx
//!
//! Abstraksi rendering — kontrak `RenderBackend` + scene graph.
//! Crate ini **tidak** tahu API GPU (wgpu/CUDA); backend nyata hidup di
//! `gnative-gfx-wgpu` (primary) dan `gnative-gfx-cuda` (fallback).

pub mod backend;
pub mod frame;
pub mod scene;

pub use backend::{BackendKind, RenderBackend};
pub use frame::FrameStats;
pub use scene::{Node, Quad, Scene, StackDirection, StackNode, TextItem};
