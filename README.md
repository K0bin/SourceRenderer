# SourceRenderer (needs a new name)
Toy game engine written in Rust.

It uses large parts of Bevy. Some major exceptions are windowing, asset management and renderer.
I had code for that before Bevy existed and I prefer my solutions.

## Features:
* Low level unsafe graphics abstraction
  * Platforms:
    * Vulkan 1.3 (primary target)
    * Metal 3
    * WebGPU (web only, not for native)
  * Features:
    * Binding model based on slots but grouped by binding frequency
    * Push constants for small very frequently changed data (Emulated using a bump allocated UBO on WebGPU)
    * Combined image+sampler emulation on Metal & WebGPU
    * Bindless (if supported)
    * Ray tracing (if supported, RT pipelines on Vulkan, RT queries on Vulkan & Metal)
    * Multi draw indirect (if supported on Vulkan & Metal)
    * Texture uploads either directly on the CPU or via a separate transfer queue
    * Occlusion queries
* Shared graphics abstraction on top of that:
    * Submission batching (runs on a worker thread if multi-threading is enabled)
    * Resource lifetimes are automatically handled by delaying destruction to when they are unused
    * Automatic reuse of command buffers
    * Handles memory allocation (allocator is rather primitive right now)
    * Buffer allocator for long-lived buffers
    * Buffer allocator for per-frame buffers using a bump allocator
    * Resource upload handling and batching
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
    * WebGPU renderer (single threaded)
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
    * Geometry living in a single big buffer to allow better instancing
    * Two render paths are planned: (mostly broken or not yet implemented right now, existing code needs to be overhauled and cleaned up)
      * Modern
        * GPU-driven multi draw indirect
        * Bindless materials
        * Frustum culling in a compute shader
        * Occlusion culling in a compute shader using the two pass hierarchical depth buffer approach
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
        * Occlusion culling using occlusion queries done with AABBs of geometry and a one frame delay to avoid synchronization
        * Re-ordering to save re-binding
        * Tile-based forward shading
        * PBR
        * SSAO
        * Bloom
        * Depth of field
        * Motion blur
        * Haven't decided between TAA or MSAA
