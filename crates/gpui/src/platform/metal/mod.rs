pub(crate) mod atlas;
pub mod renderer;

pub(crate) use atlas::MetalAtlas;
pub(crate) use renderer::{
    InstanceBufferPool, MetalRenderer, SharedRenderResources, SurfaceRenderer,
};
