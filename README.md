# spi3derr

A native terminal 3D editor and MCP modeling server written in Rust. One executable runs the editor, renders Markdown, exports images and serves MCP over stdio. It uses the Rust standard library and the terminal interfaces of macOS and Linux.

This is the GPT-6 Round III port of [Spider-Paint 26.11.26003](https://github.com/Pengeszikra/Spider-Paint/tree/6f29a123f92e50aaa044d0f6c272b387ad0e697f). The [benchmark and compatibility matrix](BENCHMARK.md) record which workflows have been exercised and where parity is incomplete.

## Open the globe

~~~sh
cargo build --release
./target/release/spi3derr globe
~~~

Press **T**, then tap **D** once. The globe keeps turning. Further taps increase orbital speed. **Space** stops motion and keeps the target. **Esc** releases it. **Enter**, type **/load**, then **Enter** opens the catalogue. Type part of a name, use ↑ or ↓ and press Enter. To fly among the 5,149 Deep Sky objects, load **deepsky**, press **Enter**, type a star name and confirm. **Tab** and **Shift+Tab** cycle targets.

**Q** enters mesh editing. **1**, **2** and **3** switch between vertices, edges and faces. Select with the pointer. **G**, **S**, **R** and **E** start a live transform. Move the pointer or enter an axis and a number. Click or press **Enter** to accept, **Esc** to cancel, **U** to undo. **P** opens the material palette. **?** opens the original help pages. **Ctrl+C** exits.

## Shared workshop

From the repository root, build once and choose a fresh path for an empty workshop.

~~~sh
export SPI3DERR_SESSION="$PWD/my-workshop.3draft"
./target/release/spi3derr --session "$SPI3DERR_SESSION"
~~~

Start GitHub Copilot CLI in the same repository with the same exported variable. The project's [.mcp.json](.mcp.json) registers the release executable. Use an absolute executable path if installing this configuration elsewhere. To inspect registration, run:

~~~sh
copilot mcp get spi3derr --json
~~~

Ask the client to read the scene revision, add the tabletop and four legs with **add_primitives**, turn a hollow mug profile with **lathe_profile** and attach a handle with **add_mesh**. Pass the revision when applying an edit based on a prior read. Changes appear in the live editor. Q edits publish back to MCP. An independent protocol client exercises this sequence in [tests/workshop.py](tests/workshop.py).

The default shared document is ~/.spi3derr/workshop.3draft. Explicit **--scene FILE --watch** also shares a file. **/save name** writes to ~/.spi3derr/store; SPI3DER_STORE overrides the directory. The catalogue searches the store and models/. **/dl path.3draft** exports the current document, and **/merge name** imports objects. Keep the models/ directory in the working directory or beside an installed binary to use the supplied catalogue.

## Other entry points

~~~sh
./target/release/spi3derr --version
./target/release/spi3derr --md README.md
./target/release/spi3derr --md models/table.scene.json
./target/release/spi3derr --scene lark-4 --once --cols 120 --rows 40
./target/release/spi3derr --mcp
cargo test
python3 tests/measure.py
~~~

The [original help index](help/index.md) links to the flight, modeling, palette, command and story documentation. It also describes features of Spider-Paint that are listed as partial or missing in the compatibility matrix.
