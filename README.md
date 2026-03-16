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
- **O**: Add a new Box brush in front of the player.
  - If a brush is already selected, pressing **O** will cycle its type (Box, Cylinder, Sphere, Cone).
- **Enter**: "Stamp" the current brush (duplicate it and keep the new one selected).
- **Delete / Backspace**: Remove the selected brush.

### Brush Editing (Position Mode - 'P' key)
When a brush is selected and you are in Position mode:
- **I / K**: Move forward / backward relative to your view (aligned to the nearest world X or Z axis).
- **J / L**: Move left / right relative to your view (aligned to the nearest world X or Z axis).
- **Y / H**: Move vertically Up / Down (World Y axis).

### Edit Modes
These keys switch the editing mode for the currently selected brush:
- **P**: Switch to Position mode (default).
- **X**: Switch to Scale mode.
- **R**: Switch to Rotation mode (currently implemented in code but pending key bindings).

## Contributing

Contributions for algorithmic optimizations and new features welcome.
However, note that smaller pull requests are more likely to get merged.
Please avoid opening pull requests with major design changes without discussing the changes you want to make first.
New textures welcome, as long as they are licensed under CC0.
