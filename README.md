# TinkerField

Dependencies:
- The [Rust toolchain](https://www.rust-lang.org/tools/install)
- wgpu and winit crates

Running the program:

```
cargo run --release
```

## Controls

### Player Movement
- **W / S** (or Arrows): Move Forward / Backward
- **A / D** (or Arrows): Strafe Left / Right
- **Mouse**: Look around
- **Escape**: Exit

### Brush Management
- **O**: Add a new Box brush at a distance. If a brush is already selected, cycle its shape (Box, Cylinder, Sphere, Cone).
- **Enter**: "Stamp" the current brush (duplicate it and keep the new one selected).
- **Delete / Backspace**: Remove the selected brush.

### Brush Editing (Position Mode - 'P' key)
When a brush is selected and you are in Position mode:
- **I / K**: Move forward / backward relative to your view (aligned to the nearest world X or Z axis).
- **J / L**: Move left / right relative to your view (aligned to the nearest world X or Z axis).
- **Y / H**: Move vertically Up / Down (World Y axis).

### Edit Modes
- **P**: Switch to Position mode (default).
- **S**: Switch to Scale mode (currently implemented in code but pending key bindings).
- **R**: Switch to Rotation mode (currently implemented in code but pending key bindings).

## Contributing
...
Contributions for algorithmic optimizations and new features welcome.
However, note that smaller pull requests are more likely to get merged.
New textures welcome, as long as they are licensed under CC0.
