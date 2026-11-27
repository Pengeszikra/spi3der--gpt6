# spi3derr — GPT-6 competition entry

Release versions use `YY.MM.DDNNN` (daily sequence, e.g. `26.11.27002`). The CLI and Cargo share the package version. Previously published `0.1.0` remains the original competition baseline.

Reference: Spider-Paint 26.11.26003, commit 6f29a123f92e50aaa044d0f6c272b387ad0e697f.

**Initial prototype, not feature parity.** Zero Cargo dependencies, no browser or JS runtime. Unix interactive terminal uses the OS stty utility. No Windows support yet.

```sh
cargo run --release
cargo run --release -- --scene model.3draft
cargo run --release -- --once --cols 80 --rows 24
cargo run --release -- --bench 1000 --cols 120 --rows 40
cargo run --release -- --markdown HELP.md
cargo test
```

Native orthographic triangle rasterizer, depth buffer, truecolor half blocks, changed-row output, face picking, cube creation, numeric transforms, face extrusion, undo, geometry-only scene import/export, local atomic store, basic Markdown highlighting, object rotation outside Q. Benchmark measures render plus checksum, excludes terminal I/O.

Store defaults to ~/.spi3derr/store; SPI3DER_STORE overrides it. Save replaces the named scene atomically; no cloud sync. Import flattens face materials to RGB. Export does not preserve unknown scene fields, camera, animation, assets, wires or material parameters: use a new filename.

Known gaps: full reference keyboard mapping, vertex/edge selection, concave polygons, perspective/flight, sky/instances, full Markdown layout, live resize while idle, mouse drag, Unicode interactive input, robust escape decoding, signal restoration, boolean, mirror, bevel, loop cut, saw, lathe, palette, story playback and exports. Do not treat benchmark as parity comparison yet.

See HELP.md for implemented commands. Rust compiler was unavailable in authoring environment; CI is configured to compile and test this prototype.
