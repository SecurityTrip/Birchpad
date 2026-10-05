# Birchpad's icon

A strip of birch bark, the notepad of medieval Novgorod, on a green tile; its dark marks read as
lines of writing.

- `birchpad.svg`: the icon, the source of every size from 48 pixels up;
- `birchpad-small.svg`: 32 pixels and less, without the tilt and the fine marks, with three
  lines on the pixel grid;
- `birchpad.ico` (Windows: the executable, installers, shortcuts), `Birchpad.icns` (macOS) and
  `birchpad.png` (Linux, 512 pixels): rendered from them, and committed.

After changing an SVG, render them again from the repository root:

```bash
cargo run -p birchpad-release-tool --example icons
```
