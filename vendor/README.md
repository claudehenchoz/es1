# Vendored crates

## egui-baseview

[egui-baseview](https://github.com/BillyDM/egui-baseview) at revision
`ec70c3fe6b2f070dcacbc22924431edbe24bd1c0` (the one nih_plug_egui pins), MIT licensed, see
`egui-baseview/LICENSE`. It is patched in from the workspace `Cargo.toml`.

The only change: `default_fonts` is removed from its default features. nih_plug_egui depends on
egui-baseview with default features, which embeds egui's fonts (Ubuntu Light, Hack, Noto Emoji,
emoji-icon-font; 1.4 MB) in the plugin. ES 1 embeds its own typeface instead (Aileron, see
`crates/es1-plugin/assets/fonts/`), so egui's copies were unused weight.

Second change, in `src/window.rs`: the window honours egui's zoom factor. Pointer positions,
scroll deltas and the screen rectangle are divided by `zoom_factor()`, and the renderer uses the
`pixels_per_point` of the frame output (native scale times zoom). ES 1 sets the zoom to scale the
whole editor (the size menu) and resizes the window to match; egui still rasterises the text at
the final pixel size, so it stays sharp at every scale. Each change is marked with `ES 1:` or
uses `zoom_factor`.

To update, copy `Cargo.toml`, `LICENSE`, `README.md` and `src/` from the new revision and repeat
the change in `Cargo.toml`.
