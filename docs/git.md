# Keeping a course in git

A CourseCraft project can be a **folder of plain files** instead of one `.ccraft` zip. A team can then keep the course in git, branch it, review a change as text, and merge.

## The folder

```text
my-course.ccraft-folder/
  course.json            the whole course, pretty JSON
  media/                 pictures, audio, video, captions, web folders
    fonts/               custom fonts
```

It holds exactly what the zip holds, entry for entry. `course.json` is written with 2-space indents, `\n` line endings, keys in a stable order and a trailing newline, so a small edit to the course is a small diff.

CourseCraft writes nothing else into the folder and deletes nothing except media it wrote itself. The rule, in full:

- It only ever deletes a file under `media/` that it recorded when it opened or last saved the folder, and only when the course no longer uses that file.
- It never deletes anything outside `media/`: not `.git`, not `.gitattributes`, not a `README`, not your notes. A file you add under `media/` after opening is not touched either.
- A save writes only the files whose content changed, each through a temporary file and a rename, with `course.json` last. If the computer stops half way, the folder holds either the old or the new `course.json`, and every media file the surviving one needs.

A temporary `*.ccraft-tmp` file left by a crash is ignored when opening and is overwritten by the next save. Add `*.ccraft-tmp` to `.gitignore`.

## Start

In the app, choose **File ▸ Save As Project Folder (for git)…** and name the folder. To open one, choose **File ▸ Open Folder…** (the folder also appears under **Open Recent**).

Or from a terminal, turn an existing file into a folder and back:

```sh
coursecraft-cli unpack my-course.ccraft -o my-course.ccraft-folder
coursecraft-cli pack my-course.ccraft-folder -o my-course.ccraft
```

`pack` then `unpack` gives back byte-identical `course.json` and media. Every `coursecraft-cli` command that takes a `<course>` accepts the folder too.

Then, as usual:

```sh
cd my-course.ccraft-folder
git init
git add .
git commit -m "First version"
```

## A good `.gitattributes`

Put this at the top of the folder:

```gitattributes
# Pictures, audio, video, fonts and archives are not text: never merge or diff them as text.
media/** binary

# Optional: keep big media out of the repository history with Git LFS.
# media/**/*.mp4 filter=lfs diff=lfs merge=lfs -text
# media/**/*.mov filter=lfs diff=lfs merge=lfs -text
# media/**/*.wav filter=lfs diff=lfs merge=lfs -text
```

Keep `course.json` as text so git can diff and merge it line by line. Add `*.ccraft-tmp` and, if you like, `*.autosave.ccraft` to `.gitignore`. CourseCraft's autosave of a folder project is written next to the folder, not in it.

## Read a change

`git diff` shows the JSON. For a description in plain words, compare two versions with `coursecraft-cli diff`. Each side can be a folder, a `course.json` or a `.ccraft`:

```sh
git worktree add ../before HEAD~1
coursecraft-cli diff ../before .
```

```text
Slide "Welcome": text of "title" changed from "Hello" to "Hi there"
Slide "Results": moved to position 1 in scene "Main"
Media: media file "banner.png" has different content
```

Slides, objects, layers and variables are matched by id, so moving a slide reads as a move, not a removal and an addition. Add `--json` for a machine-readable list. The exit code is 0 when the courses are the same and 1 when they differ.

## Teammates and the open course

If a teammate's `git pull` replaces `media/logo.png` while you have the course open, CourseCraft notices before it saves. It names the files that changed and asks whether to **Overwrite** them with what you have, **Save a Copy**, or **Cancel**. It compares only size and modified time, so this check is cheap even for a large course.

CourseCraft keeps no version history of a folder project: git is the history. The Version History window says so.

## A merge conflict in `course.json`

`course.json` is plain JSON, so a conflict is resolved like one in any text file:

1. Open `course.json`, resolve the `<<<<<<<` markers, and save. Keep both sides' changes when they touch different slides or objects; those sit in separate parts of the file.
2. Check it is still a valid course:

   ```sh
   coursecraft-cli validate .
   ```

   Fix anything it reports. A JSON syntax error is reported with its line and column.
3. Look at what the merge did: `coursecraft-cli diff <the version before> .`
4. `git add course.json` and finish the merge.

Two people changing the same object's text conflict on that one line, as in any text file. Media files are binary, so git cannot merge them: pick one side with `git checkout --ours` or `--theirs`.
