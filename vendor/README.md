# Vendored crates

## egui-baseview

[egui-baseview](https://github.com/BillyDM/egui-baseview) at revision
`ec70c3fe6b2f070dcacbc22924431edbe24bd1c0` (the one nih_plug_egui pins), MIT licensed, see
`egui-baseview/LICENSE`. It is patched in from the workspace `Cargo.toml`.

The only change: `default_fonts` is removed from its default features. nih_plug_egui depends on
egui-baseview with default features, which embeds egui's fonts (Ubuntu Light, Hack, Noto Emoji,
emoji-icon-font; 1.4 MB) in the plugin. ES 1 embeds its own typeface instead (Aileron, see
`crates/es1-plugin/assets/fonts/`), so egui's copies were unused weight.

To update, copy `Cargo.toml`, `LICENSE`, `README.md` and `src/` from the new revision and repeat
the change in `Cargo.toml`.
