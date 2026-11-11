# Recipe: translate a course and check it fits

1. Export the strings:

   ```sh
   coursecraft-cli export-xliff course.json --target-language fr -o course.fr.xlf
   ```

2. Translate each `<target>` in the file. Keep the ids, placeholders and tags exactly as they are.
3. Apply the translation to a copy:

   ```sh
   coursecraft-cli import-xliff course.json course.fr.xlf -o course.fr.json
   ```

   Exit 1 means some units were skipped (source changed, placeholder mismatch); the report names them. Fix those and import again.
4. Render every slide in the translation and look for text that no longer fits (translations are often longer):

   ```sh
   coursecraft-cli render course.fr.json --slide s-intro -o fr-s-intro.png
   ```

5. If a text box overflows, fix it with `object.set_autofit` (`"autofit": "shrink"`) or widen it with `object.update`, run the script, and render again.
