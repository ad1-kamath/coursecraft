# Recipe: build a course from a PDF

You read the PDF yourself (your own file reading); CourseCraft never parses it. Then you turn what you read into an outline and let CourseCraft build the slides.

1. Read the PDF and decide the scenes, slide titles, a few body lines per slide, and any quiz questions.
2. Make a blank course:

   ```sh
   coursecraft-cli new course.json --title "Fire safety"
   ```

3. Write `build.script.json`: one `course.import_outline` step with your outline.

   ```json
   [
     {
       "command": "course.import_outline",
       "params": {
         "outline": {
           "title": "Fire safety",
           "scenes": [
             {
               "title": "Basics",
               "slides": [
                 { "title": "Why it matters", "body": ["Fires spread fast.", "Know two exits."] },
                 {
                   "title": "Check yourself",
                   "question": {
                     "prompt": "How many exits should you know?",
                     "choices": [{ "text": "Two", "correct": true }, { "text": "One", "correct": false }]
                   }
                 }
               ]
             }
           ]
         }
       }
     }
   ]
   ```

4. Run it and check the course:

   ```sh
   coursecraft-cli run course.json build.script.json -o built.json
   coursecraft-cli validate built.json
   ```

5. Look at your work. Render each slide and read the picture:

   ```sh
   coursecraft-cli render built.json --slide slide1 -o slide1.png
   ```

   If text overflows or a slide looks empty, fix it with `text.set` or `object.update` in a second script and render again.
