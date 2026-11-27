# spi3derr — native Rust prototype

**Q** enters/leaves edit mode. **T** cycles objects. Click a visible face to select it.
**Enter** opens the command line. **?** shows help. Ctrl+C exits.
W/S zoom, A/D orbit, R/F pitch, arrows orbit, wheel zoom, 0 resets view.
In edit mode G/S/R/E open numeric transform commands; A selects all vertices.
Outside Q, uppercase R opens object rotation.

## Commands
- /add box — add cube
- /face 0 — select face by index
- /g x 2 — move 2 metres
- /scl all 2 — scale
- /r y 45 — rotate around selected geometry centre
- /e 1 — extrude selected face (Q only)
- /undo — restore previous geometry
- /save name, /store, /load name, /merge name, /forget name
- /dl file.3draft — export geometry
- /view front, /view iso, /fit, /zoom 2
- /new, /ls, /ver, /quit
