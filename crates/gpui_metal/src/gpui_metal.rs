pub mod atlas;
pub mod renderer;

pub use atlas::MetalAtlas;
pub use renderer::{
    Context, InstanceBufferPool, MetalRenderer, Renderer, SharedRenderResources, SurfaceRenderer,
    new_renderer,
};
