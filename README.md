# SourceRendererRS
Source Engine map renderer written in Rust

It's a Rust port of https://github.com/K0bin/sourceloader and I plan to add a Vulkan renderer.
The main goal of this project is to learn Rust and Vulkan.

What's working:
* extremely basic Vulkan forward renderer
* loading BSP levels
  * basic brush geometry
  * displacements (at least to some degree)
* loading 2D VTF textures
* loading the most basic VMT materials
* somewhat broken FPS camera with somewhat broken late latching on the GPU
