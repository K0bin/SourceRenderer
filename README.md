# SourceRenderer
Source Engine map renderer written in Rust

It's a Rust port of https://github.com/K0bin/sourceloader and I plan to add a Vulkan renderer.
The main goal of this project is to learn Rust and Vulkan.

Features:
* rather basic Vulkan forward renderer
  * temporal anti aliasing
  * SSAO
  * clustered forward shading (currently only supports point lights)
  * incomplete PBR lighting
  * Two render paths:
    * Conservative:
      * Frustum culling & occlusion culling (based on GPU queries)
    * GPU driven:
      * frustum culling in compute
      * occlusion culling in compute using a hierarchical z-buffer
      * bindless textures
  * Vulkan ray tracing
    * Soft shadows (denoising is still TODO)
  * Late latching just before submission to minimize latency
  * Texture streaming using a transfer queue
* Pipelined rendering
* loading Source engine maps:
  * loading BSP levels
    * basic brush geometry
    * displacements (at least to some degree)
    * light maps
    * static models
  * loading 2D VTF textures
  * loading the most basic VMT materials
* loading GLTF levels (currently without textures and needs to be rewritten to use less memory)

Platforms:
* Desktop
  * using SDL2
  * tested on Linux & Windows
  * shouldn't take too much work to make it work on Mac OS
* Web version
  * using web workers & SharedArrayBuffers for threading
  * extremely limited WebGL2 renderer
* Android version
  * used the Vulkan renderer
  * using Khronos Vulkan extension layers to support timeline semaphores
    and synchronization2 on older drivers
