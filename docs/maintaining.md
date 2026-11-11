# Maintaining CourseCraft on GitHub

For the maintainer: how the repository is set up, how changes come in, and how a release goes out. Contributors want [contributing.md](contributing.md) instead.

## Before the first push

1. **Links.** The repository lives at `github.com/ad1-kamath/coursecraft`. If it moves (a new name, or an organisation), change it in `crates/ui-egui/src/project_links.rs`, `.github/ISSUE_TEMPLATE/config.yml` and `.github/FUNDING.yml`.
2. **Hide your email.** GitHub shows the author email of every commit. In GitHub → Settings → Emails, turn on "Keep my email addresses private" and "Block command line pushes that expose my email". This repository's commits use the `…@users.noreply.github.com` address (`git config user.email`).
3. **History.** The public repository starts from one fresh commit. The earlier history is kept only on this computer, on the branch `history-private`. Never push that branch; push `main` only.
4. **Create the repository** on GitHub, public, named `coursecraft`, with no README or licence (the repo already has them), then push `main`:
   ```sh
   git remote add origin https://github.com/ad1-kamath/coursecraft.git
   git push -u origin main
   ```

## Repository settings

Settings → General:
- Features: turn on **Issues** and **Discussions**. Leave Wiki off (docs live in the repo).
- Pull requests: allow **squash merging** only, so each pull request becomes one commit on `main`. Turn on "Automatically delete head branches".

Settings → Branches → add a rule (or ruleset) for `main`:
- Require a pull request before merging.
- Require status checks to pass: choose the **CI** check (it appears after the first CI run).
- Do not allow force pushes or deletion.
- As the owner you can still push directly in an emergency; leave "Do not allow bypassing" off until there are other maintainers.

Settings → Code security:
- Turn on **Private vulnerability reporting** (`SECURITY.md` points people to it).
- Turn on **Dependabot alerts** and **Secret scanning**.

Settings → Actions → General: allow GitHub Actions. For pull requests from forks, keep "Require approval for first-time contributors".

## How changes come in

1. Someone forks the repository, works on a branch, and opens a pull request.
2. CI runs `cargo xtask ci` on it. A red cross means it doesn't build or a test fails; ask them to fix it before reviewing.
3. Review it, or ask Claude Code to: `gh pr checkout <number>`, then review and run it. Check the clean-room box in the template and the [patent watch list](contributing.md#patent-watch-list).
4. Ask for changes in comments, or **Squash and merge** when it's good.
5. Small fixes from you can still go through a pull request, so CI checks them the same way.

Issues: label them `bug` or `idea` (the templates do this), and close duplicates with a link to the first one.

## Releasing a version

1. Update `CHANGELOG.md`: move the "Unreleased" notes under a new version heading.
2. Set the version in the root `Cargo.toml` (`[workspace.package] version`).
3. Commit, then tag and push the tag:
   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```
4. The **Release** workflow builds the macOS `.dmg`, the Windows `.exe` and the Linux `.tar.gz` (each with the player inside) and attaches them to a **draft** release. Nothing is public yet.
5. Download them and try the macOS and Windows ones (Linux is built but not tried by the maintainer; say so in the notes), paste the changelog section into the release notes, and press **Publish release**.

The app's Help → Check for Updates opens the Releases page; there is no automatic update.

## What people will see on first launch

- **macOS:** the app is signed ad hoc, not by an Apple developer account, so macOS says it can't check the app. People right-click → Open the first time, or go to System Settings → Privacy & Security → Open Anyway. An Apple Developer account ($99 a year) would let the release workflow sign and notarise it.
- **Windows:** SmartScreen warns about an unknown publisher. A code-signing certificate removes the warning; until then people choose More info → Run anyway.

Say this in the release notes so people aren't alarmed.

## Donations

`.github/FUNDING.yml` adds a Sponsor button for GitHub Sponsors. Apply at github.com/sponsors (it asks for bank and tax details, and GitHub reviews it); the button shows once you're approved. To add Ko-fi or Buy Me a Coffee later, add a `ko_fi:` or `custom:` line to the same file.
