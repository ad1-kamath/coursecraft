# Performance

Measured on the maintainer's Mac (Apple M5, 10 cores, 24 GB, macOS), release build, 2026-11-11.

## The big-course test

`apps/coursecraft-cli/tests/big_course.rs` (ignored; writes about 5 GB under `target/big-course/`, which a guard deletes at the end). It lives in `coursecraft-cli` because the editor app has no library target and this crate already links format, publish, logic and course.

The course: 300 slides in 10 scenes; each slide has a title, 2 text boxes, a picture, a shape and a button with a jump trigger; every 10th slide also has a graded question and a layer. Media: 300 distinct valid PNGs (about 1 MB each, random pixels so nothing compresses away) and 10 distinct 100 MB video blobs: 1362 MB in all.

```sh
cargo test --release -p coursecraft-cli --test big_course -- --ignored --nocapture
# per-stage peak memory (macOS keeps freed pages in the resident count, so run stages apart):
BIG_STAGE=save BIG_KEEP=1 /usr/bin/time -l cargo test --release -p coursecraft-cli --test big_course -- --ignored --nocapture
BIG_STAGE=publish BIG_KEEP=1 /usr/bin/time -l cargo test ... (same)
BIG_STAGE=open /usr/bin/time -l cargo test ... (same; run after `save`)
```

There is no separate HTML5 target; the second publish row is the manifest-free LTI zip, which is the same file set as SCORM 1.2 minus the manifest.

| Stage | Budget | Before | After |
|---|---|---|---|
| validate | 200 ms | 2 ms | 2 ms |
| save `.ccraft` (1362 MB written) | 5 s | 33.8 s | 0.35 s |
| publish SCORM 1.2 | 60 s | 34.9 s | 0.5 s |
| publish LTI zip | 60 s | 34.3 s | 0.45 s |
| open `.ccraft` | 5 s | failed: "archive too large" | 0.5 s |
| simulate all 300 slides | none | 4 ms | 4 ms |
| peak memory, whole run | 3224 MB (2 x media + 500 MB) | 5472 MB | see stages |
| peak memory, save | 3224 MB | - | 1390 MB |
| peak memory, publish | 3224 MB | - | 2756 MB |
| peak memory, open | 3224 MB | - | 2735 MB |

The "after" whole-run figure is not comparable, so it is not given: when the stages share a process, macOS reports pages the program has already freed as still resident. The three stage figures each run alone.

### After M18 (with verify and versions)

A desktop save now also checks the file it wrote and keeps a copy of the file it replaced. Measured on the same course (2026-11-11), save stage run alone:

| Stage | Budget | After M18 |
|---|---|---|
| save `.ccraft` (1362 MB) | 5 s | 0.40 s |
| verify the saved file (streams every entry, CRCs checked) | part of the save | 0.38 s |
| keep a version (copy of the replaced file) | part of the save | under 1 ms here |
| the whole save | 5 s | about 0.8 s |
| peak memory, save | 3224 MB | 1393 MB (the desktop app builds the archive in memory first, about 2.7 GB for this course) |

The version copy is instant on this Mac because the file system (APFS) clones files; on a file system that cannot, it costs a plain disk copy of the project (about as long as writing it). `big_course.rs` prints the verify and copy rows with the save stage. Verification reads the file back, so on a slow or network disk it roughly doubles the time spent reading and writing.

### Project folder (for git)

The same course saved as a folder (`big.ccraft-folder`: `course.json` plus 310 media files), measured with the big-course test on 2026-11-11, release build. The first save writes every file, several at a time, each through a temporary file, a flush and a rename. The second save finds from its record that nothing changed and writes nothing (a hash of each file in memory, no read from disk). Open reads every file and records its size, hash and time.

| Stage | Budget | Folder |
|---|---|---|
| save folder, first (1362 MB, 311 files written) | 5 s | 2.8 s |
| save folder, unchanged (0 files written) | 5 s | 0.27 s |
| open folder | 5 s | 1.4 s |

The first save is slower than the `.ccraft` (0.5 s) because it flushes each of 311 files to disk before renaming it, so a crash leaves a whole file or the old one. Before the files were written in parallel, the first save took 5.3 s and the unchanged save 1.3 s. The check that runs before an app save (is any file different on disk from the record?) compares size and modified time only and reads nothing.

### Editor frame time (300 slides, pictures stubbed)

`crates/ui-egui/src/perf_tests.rs` (ignored), 60 frames each, layout plus tessellation:

```sh
cargo test --release -p coursecraft-ui-egui perf_tests -- --ignored --nocapture
```

| Case | Budget | Average | Worst |
|---|---|---|---|
| idle | 16 ms | 3.07 ms | 3.31 ms |
| select slide 150 | 33 ms | 3.14 ms | 3.43 ms |
| scroll the slide list | 33 ms | 3.07 ms | 3.11 ms |
| type in a text field | 33 ms | 3.18 ms | 3.22 ms |

All within budget with room to spare; nothing needed fixing. Frame times use stubbed media, so real thumbnail decoding cost is not in these numbers.

## What was slow, and the fixes

1. **Deflating already-compressed media.** `save_project` and `zip_package` deflated every file. PNG, JPEG, MP4 and similar do not shrink, so the time was wasted (about 34 s for 1.3 GB). Now pictures, video, audio, fonts and archives are stored (`coursecraft_format::is_precompressed`); text (`course.json`, HTML, JS, captions) is still deflated. Old files, which are all deflated, still open.
2. **Opening a big project failed.** The 512 MiB total and 256 MiB per-entry limits made any course over 512 MB unopenable. Stored entries cannot expand, so they now use the larger limits (4 GiB total, 1 GiB per entry); deflated entries keep the old 512 MiB / 256 MiB caps, so the zip-bomb guard is as strict as before.
3. **Memory copies.** The save and zip buffers are sized once instead of doubling; entries are read into an exactly-sized buffer; `save_project_to` and `zip_package_to` write straight into a file, and the editor's publish now streams the zip to a temporary file and renames it. That removes one whole-package copy from publish (it was about 4.1 GB at peak, over budget).

Still copied once: publish resolves each media file with a copy (`PackageInput::media` returns an owned `Vec`), so publish holds the media twice (2756 MB here, within budget). Removing that needs a borrowed-bytes API and was left alone.
