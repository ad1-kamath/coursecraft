# Recipe: find and fix accessibility problems

1. Check. The JSON report lists every finding with its slide and object:

   ```sh
   coursecraft-cli a11y-check course.json --json > findings.json
   ```

2. Fix each finding with a command. Missing alt text on a picture, or marking a picture as decoration:

   ```json
   [
     { "command": "object.update", "params": { "slide": "s-intro", "object": "logo", "description": "Company logo" } },
     { "command": "object.update", "params": { "slide": "s-intro", "object": "divider", "decorative": true } }
   ]
   ```

   Low contrast or small text: change the colour with `course.recolor`, or the text style with `text.style_range`.

3. Apply and check again. Repeat until the report has no errors:

   ```sh
   coursecraft-cli run course.json fixes.json -o fixed.json
   coursecraft-cli a11y-check fixed.json --json
   ```

4. Render the slides you changed and look at them:

   ```sh
   coursecraft-cli render fixed.json --slide s-intro -o s-intro.png
   ```
