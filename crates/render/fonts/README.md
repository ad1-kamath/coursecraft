# Bundled fonts

Inter (SIL Open Font License 1.1, see `OFL.txt`), by The Inter Project Authors (https://github.com/rsms/inter).

The four files are static instances of Inter 4 (optical size 14; weights 400 and 700, upright and italic), made with fontTools `varLib.instancer` and subset with `fontTools.subset` to Latin, Latin-1 Supplement, Latin Extended-A, general punctuation and a few symbols (€, ₹, ™, arrows, ✓). Kerning, ligatures, contextual alternates and tabular numbers are kept; hinting is dropped.

To regenerate, from the Inter variable fonts:

```sh
python3 -m fontTools.varLib.instancer Inter-VariableFont_opsz,wght.ttf wght=700 opsz=14 -o b.ttf
python3 -m fontTools.subset b.ttf --unicodes="U+0020-007E,U+00A0-017F,U+0192,U+02C6,U+02DA,U+02DC,U+2000-206F,U+20AC,U+20B9,U+2122,U+2190-2193,U+2212,U+2215,U+25CF,U+2713" \
  --layout-features='kern,liga,calt,tnum' --no-hinting --desubroutinize --output-file=Inter-Bold.ttf
```

Phosphor Icons 2.1.2 (MIT License, see `LICENSE-Phosphor.txt`), by Phosphor Icons (https://phosphoricons.com), from the npm package `@phosphor-icons/web`. `Phosphor.ttf` (regular) and `Phosphor-Fill.ttf` (fill) are subset to the icons named in `phosphor-icons.txt`; `phosphor-codepoints.txt` maps each name to its codepoint (the same in both weights).

To add an icon, add its name to `phosphor-icons.txt`, look up its codepoint in the package's `src/regular/style.css` (`.ph.ph-<name>:before { content: "\eXXX" }`), and regenerate both files with the full list of codepoints:

```sh
python3 -m fontTools.subset node_modules/@phosphor-icons/web/src/regular/Phosphor.ttf --unicodes="U+E2C2,..." --no-hinting --layout-features='' --output-file=Phosphor.ttf
python3 -m fontTools.subset node_modules/@phosphor-icons/web/src/fill/Phosphor-Fill.ttf --unicodes="U+E2C2,..." --no-hinting --layout-features='' --output-file=Phosphor-Fill.ttf
```
