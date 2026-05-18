# TinkerField

Tinkerfield is a toy 3D game engine / construction game based on Signed
Distance Fields (SDFs), where you can edit the world in real time. It uses
SDFs for all 3D graphics (no polygons).

One of my main goals with this project just to show people we're already at
the point where it's possible to build some kind of game engine with SDFs.
It doesn't have to be complicated, and you don't need some killer GPU to
make it work either.

Supported platforms: macOS and Linux. Windows may work but is untested.

Dependencies:
- The [Rust toolchain](https://www.rust-lang.org/tools/install)
- wgpu, winit, png crates

Running the program:

```
cargo run --release
```

To load a map file, pass it as an argument:

```
cargo run --release maps/city.map
```

Several example maps are provided in the `/maps` directory (`city.map`, `house.map`, `nature.map`).

## Controls

### Player Movement

- **W / S** (or Arrow keys): Move forward / backward
- **A / D** (or Arrow keys): Strafe left / right
- **Mouse**: Look around
- **Escape**: Exit

### Brush Management

- **O**: Create a new Box brush in front of the player
- **T**: Cycle the selected brush's type (Box → Cylinder → Sphere → Cone)
- **Q**: Toggle the selected brush between Add and Subtract mode
- **C**: Subtract a cylinder aligned to the camera direction (quick tunnel tool)
- **Enter**: Deselect the current brush
- **Delete / Backspace**: Remove the selected brush
- **N / M**: Cycle to the previous / next material
- **Ctrl+C / Ctrl+V**: Copy / paste the selected brush

### Edit Modes

Switch modes with these keys when a brush is selected:
- **P**: Position mode (default)
- **X**: Scale/size mode
- **R**: Rotation mode (work in progress)

### Position and Scale Editing

In Position and Scale modes, holding a modifier key lets you move or resize
the selected brush with the mouse. The edit axes are snapped to world axes
based on your facing direction at the moment you press the modifier key.

- **Shift + Mouse**: left/right controls the horizontal axis most aligned with
  your view, up/down controls the vertical (Y) axis.
- **Alt + Mouse**: left/right controls the horizontal axis most aligned with
  your view, up/down controls the horizontal axis most aligned with your
  forward direction — keeping movement on the ground plane.

Positions and scales snap to a 0.1-unit grid.

## Architecture

The world is made up of **brushes** — simple shapes (box, cylinder, sphere,
cone) with a position, rotation, scale, material, and a CSG operation (add or
subtract). There are no polygons anywhere; every brush is defined purely by its
SDF.

To render efficiently on the GPU, the world is partitioned into a 3D spatial
grid where each cell stores a short list of brush indices that overlap it. On
every frame, a wgpu compute/render shader ray-marches through this grid: for
each pixel it steps along the ray cell by cell, evaluates only the brushes
in the cells it visits, and finds the surface by standard sphere-tracing. This
keeps per-pixel SDF evaluation cheap even with many brushes in the scene.

Brushes are stored in a GPU-side buffer and the grid is rebuilt on the CPU
whenever the world changes, then re-uploaded. Lighting is a simple diffuse +
ambient model computed in the fragment shader, with materials sampled from a
texture atlas.

## Contributing

All code is licensed under the [MIT License](LICENSE), but textures are
licensed as CC0 (public domain).

Contributions for algorithmic optimizations and new features are welcome.
However, note that smaller pull requests are more likely to get merged.
Please avoid opening pull requests with major design changes without discussing the changes
you would like to make first.

New textures welcome, we could particularly use more seamless materials, as long as they are
licensed under CC0 (public domain). Textures should be in 24-bit PNG format, and at a
resolution of 512 pixels per meter.
