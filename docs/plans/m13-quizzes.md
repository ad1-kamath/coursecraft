# M13 Deeper quizzes

Goal: quizzes that match what Storyline authors expect: flexible scoring, surveys, question banks, results slides, retry, review and certificates. Every runtime change lives in `coursecraft-logic` and is written into `docs/spec/triggers.md` in the same change.

## Step 1: Points per answer and partial credit

- `Choice.points` (whole number, -1000 to 1000, default 0).
- `Question.scoring`: `all_or_nothing` (default), `per_answer` (multiple choice, multiple response, true/false) or `partial` (multiple response, sequence, matching, hotspot, drag-and-drop).
- `Question::max_points()` is the one answer to "what is this question worth": `per_answer` is the best choice's points (multiple choice, true/false) or the sum of positive points (multiple response); otherwise `points`.
- Grading still decides `correct` exactly as before, so feedback layers and locking do not change. Only the points awarded differ. `per_answer` adds up the chosen answers' points and clamps to `0..=max`. `partial` awards `floor(points * right / total)`.
- Inspector: a Scoring picker, and a points field on each answer when scoring is per answer.

## Step 2: Survey questions

Likert scale, short answer and essay. Not graded, reported to the LMS as survey interactions.

## Step 3: Question banks and random draw

- `Course.banks`: named banks of question slides, kept outside play order like masters. Every bank slide has a question.
- A slide in a scene may carry `draw: { bank, count }`. At play time it is replaced by `count` questions picked at random from that bank. Two draws from one bank never pick the same question, and their counts may not add up to more than the bank holds.
- The picks come from a seed chosen when the learner starts. The seed is saved with their progress, so resuming shows the same questions. `simulate` uses a fixed seed unless told otherwise.
- A jump to a draw slide lands on its first drawn question. Only drawn questions count toward the score.
- Editor: banks listed under the slides in the left panel, bank slides edited like any slide, and a draw slide with a bank picker and a count.

## Step 4: Results slides, retry and answer review

Three pieces: 4a the runtime, 4b the renderer and player, 4c the editor.

- **4a Runtime.** Actions `retry_quiz`, `review_quiz` and `exit_review`, and built-in variables `Quiz.Passed`, `Quiz.Attempts` and `Quiz.Reviewing`. Retry clears the answers and, with banks, draws new questions, then lands on the first question. Review walks the answered questions in play order with the learner's answers restored from a runtime-only snapshot, every question locked, no slide-entry triggers, autoplay or feedback layers; `Next` after the last answered question exits review. `Effect::ReviewChanged { reviewing }` tells hosts. `quiz_attempts` joins suspend data only for courses with `quiz` or banks.
- **4b Renderer and player.** While reviewing, each choice, sequence row, matching row, fill-in field, hotspot region and drag item carries a review mark: a tick for a right answer the learner gave, a cross for a wrong one, and an outlined tick on the right answer they missed, with the accepted answer written under a wrong fill-in or matching row. A review banner above the slide (the notice banner's look, the accent colour instead of the warning colour) reads "Reviewing answers · Question 2 of 5 · Correct · 10 of 10 points" with an **Exit review** button that sends the new `Input::ExitReview`. The access tree announces the banner and reads each mark. Survey questions show the answer without a mark. All strings are chrome strings, so they translate.
- **4c Editor.** `results.insert` builds an ordinary slide after the selected one: a title, "Your score: %Course.ScorePercent%%" and "Points: %Course.ScorePoints%" text, the passing score, a **Passed** layer (message, Review quiz, Finish) and a **Failed** layer (message, Retry quiz, Review quiz), and three base triggers: `submit_results` when the timeline starts, then show Passed when `Quiz.Passed` is true, else Failed. It sets `quiz.passing_percent` to 80 when the course has none. Insert ▸ Results slide runs it; the trigger form offers Retry quiz, Review quiz and Exit review; `docs/cli.md` lists the command. Everything on the slide stays editable like any other slide.
- Deferred: a results slide that lists every question with its result (the review walk covers it); "retry only incorrect questions"; a per-question review layer (authors can add one by hand).

## Step 5: Certificates and any slide as a graded question

Three pieces: 5a freeform grading, 5b the certificate runtime, 5c the certificate in the player and editor.

- **5a Any slide as a graded question.** `QuestionKind::Freeform`: a question with no choices whose `correct_when` is a trigger-style `Condition` over variables and object states on the slide. Submit evaluates the condition, awards `points` or 0, shows the feedback layer and locks, exactly like other kinds; it never emits `answer_required`. The prompt is drawn only when it is non-empty, so the author's own slide design is the question. The question form shows the kind and the existing condition builder. The LMS reports it as an `other` interaction with the response `correct` or `incorrect`.
- **5b Certificate runtime.** Built-in text variables `Learner.Name`, `Course.Title` and `Course.Date`. The host sets the learner name and today's date through `Player::set_host_info`: the SCORM bridge reads `cmi.core.student_name` or `cmi.learner_name`, the xAPI bridge the actor's name, standalone players leave the name empty. `Action::PrintSlide` emits `Effect::Print { slide, exclude }`, where `exclude` is the clicked object that fired it, so the Print button itself is never printed.
- **5c Certificate hosts and editor.** `render_player_rgba` draws the live player's slide (variables substituted, states applied) without the excluded objects. The web player turns that into a PNG and opens the browser's print dialog; the editor preview saves it as a PNG through a save dialog; the bare desktop player logs that printing works in the published course. `certificate.insert` builds a certificate slide (border, "Certificate of Completion", `%Learner.Name%`, `%Course.Title%`, score, `%Course.Date%`, a Print button) after the selected slide; Insert ▸ Certificate slide, the trigger form's Print slide action and `docs/cli.md` follow.
- Deferred: PDF output on the desktop (PNG is what the OS print dialog needs anyway); a certificate template gallery; a learner-typed name field on the certificate (an author can bind a text entry to a variable and reference it); signatures and images on the certificate beyond what the author places by hand.

## Done bar

Each step: model and validation tests, logic tests with `simulate` scripts, render and UI harness tests, `docs/spec/triggers.md` updated, `simulate` on the examples unchanged, `cargo xtask ci` green, and the feature played in a published course.
