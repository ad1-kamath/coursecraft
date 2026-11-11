# Recipe: rebrand many courses

One script, many courses, one command.

1. Write the script (see `examples/batch/rebrand.json`): recolour the old brand colour everywhere, then set the font.

   ```json
   [
     { "command": "course.recolor", "params": { "from": "#2F6FEB", "to": "#0B7A75" } },
     { "command": "course.set_theme", "params": { "font": "Inter" } }
   ]
   ```

2. See what would change, writing nothing:

   ```sh
   coursecraft-cli batch examples/batch/rebrand.json --dir courses --out-dir branded --dry-run
   ```

3. Run it. `--out-dir` writes copies; `--in-place` saves over each course safely:

   ```sh
   coursecraft-cli batch examples/batch/rebrand.json --dir courses --out-dir branded
   ```

4. Swap the logo file in every course (same path, new bytes; it must be the same kind of file):

   ```sh
   coursecraft-cli batch examples/batch/set-logo.json --dir courses --in-place --replace-media logo.png=new-logo.png
   ```

5. Read the table. Any `failed: reason` course was not written; fix it and run again. Then render one slide from a few courses to check the result.
