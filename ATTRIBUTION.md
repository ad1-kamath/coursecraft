# Attribution

Third-party assets shipped with CourseCraft. Every asset added must get a row here (path, title, author, source, license).

| Path | Title | Author | Source | License |
|---|---|---|---|---|
| `crates/render/fonts/Inter-*.ttf` | Inter, subset | The Inter Project Authors | https://github.com/rsms/inter | SIL Open Font License 1.1, in `crates/render/fonts/OFL.txt` |
| `crates/render/fonts/Phosphor*.ttf` | Phosphor Icons, subset | Phosphor Icons | https://github.com/phosphor-icons/homepage | MIT, in `crates/render/fonts/LICENSE-Phosphor.txt` |
| `crates/media/fonts/SymbolsFallback.ttf` | CourseCraft Symbols, a 39-glyph subset of DejaVu Sans (renamed, as its licence requires) | Bitstream, Inc., Tavmjong Bah and the DejaVu contributors | https://dejavu-fonts.github.io | Bitstream Vera licence, in `crates/media/fonts/LICENSE-DejaVu.txt` |
| `crates/spell/dictionaries/en_US.*` | English (US) Hunspell dictionary, version 2020.12.07 | Kevin Atkinson and the SCOWL contributors | https://github.com/wooorm/dictionaries (from http://wordlist.sourceforge.net) | SCOWL licence (MIT/BSD-style), in `crates/spell/dictionaries/LICENSE-en_US.txt` |

The images, sound and video under `examples/showcase/media/` were made for this project and carry the project's license.

## ArtCraft Crafting Apps libraries and forks

CourseCraft reuses open-source libraries, forks and widget code from the ArtCraft Crafting Apps family (`storytold`):

| Component | Author | Source | License |
|---|---|---|---|
| `craft-ui`, `craft-i18n`, `craft-update` | ArtCraft Team and the `craft-libs` contributors | https://github.com/storytold/craft-libs | MIT OR Apache-2.0 |
| Craft `egui` fork (`egui`, `eframe`, `epaint`, `emath`, `ecolor`, `egui-winit`, `egui-wgpu`, `epaint_default_fonts`) | Emil Ernerfeldt and the ArtCraft / `egui` contributors | https://github.com/storytold/egui | MIT OR Apache-2.0 |
| Craft `winit` fork (`winit`) | The `winit` contributors and the ArtCraft contributors | https://github.com/storytold/winit | Apache-2.0 |
| Ribbon widget patterns (`crates/ui-egui/src/ribbon/widgets.rs`) | ArtCraft Team (DeckCraft, commit `7fbdc085`) | https://github.com/storytold/deckcraft | MIT OR Apache-2.0 |

Other Rust crates are listed in `Cargo.lock`, under their own licenses.
