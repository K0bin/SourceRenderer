# SourceRenderer (needs a new name)
Toy game engine written in Rust

## Features:
* Low level unsafe graphics abstraction
  * Platforms:
    * Vulkan 1.3 (primary target)
    * Metal 3
    * WebGPU (web only, not for native)
  * Features:
    * Binding model based on slots but grouped by binding frequency
    * Push constants for small very frequently changed data
    * Bindless (if supported)
    * Ray tracing (if supported, RT pipelines on Vulkan, RT queries on Vulkan & Metal)
    * Multi draw indirect (if supported on Vulkan & Metal)
    * Occlusion queries (TODO)
* Shared graphics abstraction on top of that:
    * Submission batching (runs on a worker thread if multi-threading is enabled)
    * Resource lifetimes are automatically handled by delaying destruction as soon as they are unused
    * Handles memory allocation (allocator is rather primitive right now)
    * Buffer allocator for long-lived buffers
    * Buffer allocator for per-frame buffers using a bump allocator
    * Resource upload handling, batching and running it on separate transfer queue (TODO: implement support for direct uploads from the CPU on devices with unified memory)
* Platform abstraction with support for:
  * Windows & Linux
    * SDL window
    * Vulkan renderer
  * Mac OS
    * SDL window
    * Metal renderer
  * Android (WIP: broken right now)
    * Kotlin Android window
    * Vulkan renderer
  * Web
    * HTML + Typescript window
    * Requires cutting edge browser features
    * Engine running entirely in a worker
    * WebGPU renderer (WIP: needs work on shader translation, single threaded)
* Async asset manager
  * Optionally multi-threaded asset loading
  * Asset hot-reloading
  * Rudimentary texture streaming (doesn't prioritize or unload anything yet)
  * GLTF asset loader
* Renderer:
    * Pipelined (render thread if multi-threading is enabled)
    * Fork-join for CPU-driven geometry passes (if multi-threading is enabled)
    * Trivialized worker-thread pipeline compilation (runs on a worker thread if multi-threading is enabled)
    * Semi-automatic barrier handling
    * Automatic managing of texture views for GPU
    * Geometry
    * Two render paths are planned: (mostly broken or not yet implemented right now, existing code needs to be overhauled and cleaned up)
      * Modern
        * GPU-driven multi draw indirect
        * Bindless materials
        * Frustum culling in a compute shader
        * Occlusion culling in a compute shader by reprojecting previous frames depth buffer
        * Depth prepass
        * Tile-based forward shading
        * PBR
        * SSAO
        * Bloom
        * Depth of field
        * Motion blur
        * TAA
      * Compatibility
        * CPU-driven but will hopefully use batching and instancing
        * Occlusion culling (haven't decided whether it's based on occlusion queries)
        * Re-ordering to save re-binding
        * Tile-based forward shading
        * SSAO
        * Bloom
        * Depth of field
        * Motion blur
        * Haven't decided between TAA or MSAA
