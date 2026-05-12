This is a simple game engine / construction game based on signed
distance fields. To get more context, read @src.

Rules when editing the code:
- Do not remove comments
- Do not undo changes made by the user
- Do not make changes that were not requested or approved by the user
- Respect existing styling around curly braces and variable names
  Try to match the styling of the existing code.
- Be careful when making edits to avoid undoing changes made by the user
- Commenting
  - Each struct must have a comment about it describing its purpose
  - Any non-trivial function should have a comment above it describing its purpose
  - Add brief comments troughout the code, one or two lines, to explain the logic
- Try to avoid inserting unnecessary trailing whitespace
- Try to avoid producing deep levels of indentation/nesting
  - Use the early return, early escape pattern to avoid indentation where possible
- After making changes, test that your code builds using `cargo build`.
