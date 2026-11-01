# GLB Export Parity

## Goal

The bundled GLB exporter is a one-shot, software-agnostic consumer of the same
source material and scene data used by the decomposed Blender workflow. The GLB
must not require the StarBreaker Blender add-on or any sidecar files at runtime.

The decomposed material contract remains the high-fidelity reference. GLB maps
that contract to core glTF metallic-roughness PBR, standard Khronos extensions,
embedded PNGs, and diagnostic `extras` for source semantics that glTF cannot
represent directly.

## Phase 1: Materials And Textures

Status: implemented for the portable PBR surface and texture path.

The bundled exporter now:

- resolves the equipped paint/material override before decoding root textures;
- bakes the selected concrete palette into layered source textures;
- embeds diffuse, normal, DDNA-derived metallic-roughness, emissive, occlusion,
  decal, and generated UI images in the GLB;
- preserves authored `TexMod` scale and offset with `KHR_texture_transform`,
  including normal and occlusion texture infos;
- preserves secondary UV selection;
- maps palette finish glossiness to PBR roughness when no roughness map exists;
- maps authored/palette specular response through `KHR_materials_specular`;
- maps authored glass values through `KHR_materials_transmission`,
  `KHR_materials_ior`, and `KHR_materials_volume`;
- maps high emission through `KHR_materials_emissive_strength`;
- renders source-backed UI bindings once per unique binding, then specializes
  shared screen materials by NMC helper so different screens can carry different
  embedded images;
- preserves shader family, palette routing, layers, public parameters, authored
  XML fields, activation state, and source identity in material `extras`;
- omits invented screen images and unauthored glass volume values.

`--materials textures` is the normal portable export mode. `all` remains for
compatibility with callers that explicitly request every available reconstruction
path.

### Portable Representation Limits

One static GLB cannot retain editable palettes or arbitrary Blender/Cycles node
graphs. It contains the concrete equipped paint and baked portable maps. POM,
multi-layer wear, stencil composition, and other Star Engine shader behavior are
flattened where source data permits and retained as semantic `extras` otherwise.

Animated `TexMod` oscillator blocks remain metadata-only; static scale and offset
are exported. The legacy metallic classifier in `mtl.rs` also remains an evidence
gap: it derives conductor state from authored response values because no explicit
Star Engine metallic flag has yet been identified. Replace that classifier when
the engine rule or an explicit source field is verified; do not add asset-name
exceptions or more thresholds.

## Phase 2: Animation Parity

The glTF writer already carries the rig: `Mesh::skin` and `Mesh::morph_targets`
are packed by `pack_rig`, skins are bound per mesh node by `bind_skins`
(joints by name, inverse bind matrices), and `gltf/animation.rs` turns
`clip::Clip` values into glTF animations (bone TRS channels, morph weights,
material-parameter tracks in `extras`).

What remains is wiring the entity export to it: `assemble_glb_with_loadout`
still passes `animations: Vec::new()`. The remaining work reuses the existing
`.chrparams` / DBA / CAF / Mannequin discovery used by decomposed animation
sidecars:

| Work item | Deliverable |
|---|---|
| 2A Clip discovery | Feed the entity's decoded clips (root, attachments, landing gear, animated interior CGA/CHR entities) into `GlbInput::animations`. |
| 2B Joint binding | Resolve every channel to the entity-local joint by source skeleton identity and bone hash/name. Report unresolved and ambiguous channels; never bind by a guessed global node name, and never cross-bind duplicate bone names between entity instances. |
| 2C Semantic metadata | Store Mannequin fragments, tags, scopes, source paths, and event markers in animation `extras`. |
| 2D Validation | Compare first/final sampled GLB poses against decomposed sidecars, run glTF validation, and import in Blender without the add-on. |

## Validation Commands

```bash
cargo test -p starbreaker-3d --lib
cargo test --workspace
scripts/build-release-cli.sh
./target/release/starbreaker entity export <entity> <output>.glb --materials textures
```
