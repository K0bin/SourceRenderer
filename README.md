# SourceRenderer (needs a new name)

Toy game engine written in Rust.

It uses large parts of Bevy. Some major exceptions are windowing, asset management and renderer.
I had code for that before Bevy existed and I prefer my solutions.

The original goal was to render CSGO maps, hence the name.

## Building:

### Desktop

* Run `cargo build` in the root directory.
* Run it by either executing `cargo run` or by manually starting `target/release/sourcerenderer_sdl`.

### Web

* Run `wasm-pack platform/web/lib --target web` in the root directory.
* Build the tiny web page using `npm run build` in `platform/web/www`.
* Host the page locally using `npm run dev` in `platform/web/www`.

## Features:

* Low level unsafe graphics abstraction
    * Platforms:
        * Vulkan 1.3 (primary target)
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
    * Windows, Linux, Mac OS
        * SDL window
        * Vulkan renderer (use Kosmickrisp on Mac OS)
    * Android (WIP: broken right now)
        * Kotlin Android window
        * Vulkan renderer
    * Web
        * HTML + Typescript window
        * Requires cutting edge browser features
        * Engine running entirely in a worker
        * WebGPU renderer running in a separate worker (render thread, moving work to other workers is not possible with
          WebGPU)
* Async asset manager
    * Optionally multi-threaded asset loading
    * Asset hot-reloading
    * Rudimentary texture streaming (doesn't prioritize or unload anything yet)
    * GLTF asset loader
* Renderer:
    * Pipelined (render thread if multi-threading is enabled)
    * Trivialized worker-thread pipeline compilation (runs on a worker thread if multi-threading is enabled)
    * Semi-automatic barrier handling
    * Two render paths are planned:
        * Modern
        * Compatibility
