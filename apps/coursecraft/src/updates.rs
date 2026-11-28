//! CourseCraft's update wrapper around `craft_update`: one quiet check at start-up and one on
//! demand from Help, the prompt that offers the update, and the window that reports a requested
//! check.
//!
//! `craft_update::updater::Updater` runs the background check, reads the launcher's version pin
//! choice, and reports the outcome of a completed update on the next launch. CourseCraft owns the
//! `StartupPrompt` and self-update worker so that:
//! - Clicking **Later** / **Dismiss** or closing the prompt (Esc / ×) while an **Update and
//!   Restart** download is in flight cancels `craft_update::Stop` immediately: the app does not
//!   close, swap the binary/bundle, or install on exit, and a later explicit **Update and
//!   Restart** can start again.
//! - macOS and Linux self-updates select `.dmg`, `.AppImage` (when `$APPIMAGE` is set), or
//!   `.tar.gz`, verify `SHA256SUMS.txt`, and only start when the user clicks **Update and
//!   Restart** with no unsaved work.
//! - Windows releases ship the Inno Setup `-setup.exe` installer and a `-portable.zip`. Windows
//!   copies open the prompt in manual-download mode: installed copies point at `-setup.exe`;
//!   portable copies (`portable.txt` beside the executable) point at `-portable.zip` with a short
//!   note on how to update a portable folder.
//!
//! Desktop only; the web build has no updates.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use craft_update::release::{expected_download_url, safe_web_link, same_download_url};
use craft_update::self_update::Restart;
use craft_update::translate::Note;
use craft_update::updater::{Action, CheckSource, READY_TEXT, Status, Updater};
use craft_update::{
    Arch, Asset, CheckResultDialog, CheckResultResponse, Os, PackageKind, Platform, Release,
    StartupPrompt, StartupPromptResponse, Stop, pick_asset,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Notice shown in the update prompt when running a Windows portable copy (`portable.txt` beside
/// the executable).
pub const PORTABLE_NOTE: &str = "This is the portable version. To update, download the new portable .zip and unpack it in place of this folder.";

/// CourseCraft as craft-update knows it: its releases live in the upstream repository, and the
/// prompt shows its icon and teal accent.
pub const UPDATE_APP: craft_update::App = craft_update::App {
    icon_png: Some(include_bytes!("../icons/window-256.png")),
    accent: Some([0x0B, 0x7F, 0x72]),
    // The macOS bundle's identifier (scripts/package-macos.sh), kept for existing installs.
    bundle_id_override: Some("org.coursecraft.CourseCraft"),
    ..craft_update::App::new(
        "coursecraft",
        "CourseCraft",
        "eLearning authoring",
        "ad1-kamath/coursecraft",
    )
};

/// How this running copy handles discovered updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum InstallMode {
    /// macOS and Linux: self-updates via `craft_update::self_update::prepare_update` (`.dmg`,
    /// `.AppImage` when `$APPIMAGE` is set, `.tar.gz` otherwise) with explicit cancellation.
    CraftUpdate,
    /// Windows Inno Setup installation: manual download of `-setup.exe`.
    WindowsSetup,
    /// Windows portable copy (`portable.txt` beside the executable): manual download of
    /// `-portable.zip` with [`PORTABLE_NOTE`].
    WindowsPortable,
}

/// Whether `exe` belongs to a Windows portable copy (marked by `portable.txt` beside it).
#[must_use]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn is_portable_at(exe: &Path) -> bool {
    exe.parent()
        .is_some_and(|dir| dir.join("portable.txt").is_file())
}

/// Detects the update handling mode for the current OS and executable.
fn detect_install_mode() -> InstallMode {
    #[cfg(windows)]
    {
        if std::env::current_exe()
            .ok()
            .is_some_and(|exe| is_portable_at(&exe))
        {
            InstallMode::WindowsPortable
        } else {
            InstallMode::WindowsSetup
        }
    }
    #[cfg(not(windows))]
    {
        InstallMode::CraftUpdate
    }
}

/// The Linux package format matching the running process: `.AppImage` when `$APPIMAGE` is set
/// and non-empty, `.tar.gz` otherwise (matching `craft_update::self_update`'s Linux detection).
#[must_use]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn linux_package_kind(appimage_env: Option<&OsStr>) -> PackageKind {
    if appimage_env.is_some_and(|value| !value.is_empty()) {
        PackageKind::AppImage
    } else {
        PackageKind::TarGz
    }
}

/// Picks the matching Linux release asset (`.AppImage` when `$APPIMAGE` is set, `.tar.gz`
/// otherwise) using `craft-update`'s asset matcher.
#[must_use]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn select_linux_asset<'a>(
    release: &'a Release,
    arch: Arch,
    appimage_env: Option<&OsStr>,
) -> Option<(&'a Asset, PackageKind)> {
    let stems: Vec<&str> = UPDATE_APP.stems().collect();
    let kind = linux_package_kind(appimage_env);
    pick_asset(
        release,
        &stems,
        Platform {
            os: Os::Linux,
            arch,
        },
        &[kind],
    )
}

/// Preferred Windows asset platform tokens for `arch` (native first, then emulated).
fn windows_tokens(arch: Arch) -> &'static [&'static str] {
    match arch {
        Arch::X64 => &["windows-x64", "windows-x86"],
        Arch::Arm64 => &["windows-arm64", "windows-x64", "windows-x86"],
        Arch::X86 => &["windows-x86"],
        Arch::Riscv64 => &[],
    }
}

/// Parses a Windows asset name (`<stem>-<version>-windows-<arch><suffix>`, where `suffix` is
/// `-setup.exe` or `-portable.zip`), returning its `<os>-<arch>` token when it matches one of
/// `stems` and a valid version.
fn parse_windows_token(name: &str, suffix: &str, stems: &[&str]) -> Option<String> {
    let base = name.strip_suffix(suffix)?;
    for stem in stems {
        let Some(rest) = base.strip_prefix(stem).and_then(|r| r.strip_prefix('-')) else {
            continue;
        };
        let mut parts = rest.rsplitn(3, '-');
        let (Some(arch), Some(os), Some(version)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if os != "windows"
            || craft_update::parse_version(version).is_none()
            || !version.starts_with(|c: char| c.is_ascii_digit())
        {
            continue;
        }
        return Some(format!("{os}-{arch}"));
    }
    None
}

/// Picks the Windows asset ending in `suffix` from `release` for `arch` (native first).
fn select_windows_asset<'a>(
    release: &'a Release,
    stems: &[&str],
    arch: Arch,
    suffix: &str,
) -> Option<&'a Asset> {
    let parsed: Vec<(&Asset, String)> = release
        .assets
        .iter()
        .filter_map(|asset| {
            parse_windows_token(&asset.name, suffix, stems).map(|token| (asset, token))
        })
        .collect();
    for token in windows_tokens(arch) {
        if let Some((asset, _)) = parsed.iter().find(|(_, t)| t == token) {
            return Some(*asset);
        }
    }
    None
}

/// Picks the Windows `-setup.exe` installer asset from `release` for `arch`. Never selects a
/// portable zip or a non-Windows file.
#[must_use]
pub(crate) fn select_windows_setup_asset<'a>(
    release: &'a Release,
    stems: &[&str],
    arch: Arch,
) -> Option<&'a Asset> {
    select_windows_asset(release, stems, arch, "-setup.exe")
}

/// Picks the Windows `-portable.zip` asset from `release` for `arch`. Never selects the
/// installer or a non-Windows file.
#[must_use]
pub(crate) fn select_windows_portable_asset<'a>(
    release: &'a Release,
    stems: &[&str],
    arch: Arch,
) -> Option<&'a Asset> {
    select_windows_asset(release, stems, arch, "-portable.zip")
}

/// The safe download link for a Windows update: the release's `-setup.exe` asset (or, when
/// `portable`, its `-portable.zip`) when present and bound to `app`'s repository/tag, otherwise
/// the release page or the app's releases page.
#[must_use]
#[cfg(test)]
pub(crate) fn windows_download_url(
    app: &craft_update::App,
    release: &Release,
    arch: Arch,
) -> Option<String> {
    windows_download_url_for(app, release, arch, false)
}

/// Like [`windows_download_url`], choosing the portable zip when `portable` is true.
#[must_use]
pub(crate) fn windows_download_url_for(
    app: &craft_update::App,
    release: &Release,
    arch: Arch,
    portable: bool,
) -> Option<String> {
    let stems: Vec<&str> = app.stems().collect();
    let repo = if release.repo.is_empty() {
        app.repo
    } else {
        release.repo.as_str()
    };
    let asset = if portable {
        select_windows_portable_asset(release, &stems, arch)
    } else {
        select_windows_setup_asset(release, &stems, arch)
    };
    if let Some(asset) = asset
        && safe_web_link(&asset.url)
        && expected_download_url(repo, &release.tag, &asset.name)
            .is_some_and(|expected| same_download_url(&asset.url, &expected))
    {
        return Some(asset.url.clone());
    }
    release_download_url(app, release)
}

/// The safe release-page link for manual download on non-Windows platforms.
#[must_use]
fn release_download_url(app: &craft_update::App, release: &Release) -> Option<String> {
    if safe_web_link(&release.html_url) {
        return Some(release.html_url.clone());
    }
    let releases = app.releases_url();
    safe_web_link(&releases).then_some(releases)
}

fn current_windows_arch() -> Arch {
    Platform::current().map_or(Arch::X64, |p| p.arch)
}

fn running_version() -> String {
    craft_update::self_update::current_version(&UPDATE_APP, VERSION)
        .map_or_else(|| VERSION.to_owned(), |v| v.to_string())
}

fn can_install_itself(release: &Release) -> bool {
    craft_update::self_update::is_self_updating(&UPDATE_APP)
        && craft_update::self_update::release_installs(&UPDATE_APP, release)
}

fn save_first_note() -> Note {
    Note::new(
        "Save or close your open work first; the update restarts the app.",
        &[],
    )
}

fn installed_note() -> Note {
    Note::new(READY_TEXT, &[("app", UPDATE_APP.name)])
}

fn build_prompt(release: Release, mode: InstallMode, installs_itself: bool) -> StartupPrompt {
    let self_update = mode == InstallMode::CraftUpdate && installs_itself;
    let mut prompt =
        StartupPrompt::with_self_update(&UPDATE_APP, release, running_version(), self_update)
            .with_translator(translate);
    if mode == InstallMode::WindowsPortable {
        prompt.set_note(Note::new(PORTABLE_NOTE, &[]));
    }
    prompt
}

/// A prepared update waiting for process exit.
enum ReadyUpdate {
    Real(Restart),
    #[cfg(test)]
    Simulated(Arc<std::sync::atomic::AtomicBool>),
}

impl ReadyUpdate {
    fn start(self) -> Result<(), String> {
        match self {
            Self::Real(restart) => restart.start(),
            #[cfg(test)]
            Self::Simulated(ran) => {
                ran.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        }
    }
}

/// An in-flight self-update download/install worker with its cancellation handle.
struct ActiveInstall {
    rx: Receiver<Result<ReadyUpdate, String>>,
    stop: Arc<Stop>,
}

#[cfg(test)]
type InstallProbe = Arc<dyn Fn(&Release) -> bool + Send + Sync>;
#[cfg(test)]
type InstallRunner = Arc<dyn Fn(&Release, Arc<Stop>) -> Result<ReadyUpdate, String> + Send + Sync>;

/// Downloads, verifies, and prepares a self-update while checking `stop` throughout download and
/// immediately before swapping the installed binary/bundle on disk.
fn run_self_update(
    installed: &craft_update::self_update::Install,
    mut release: Release,
    version: &str,
    stop: &Stop,
) -> Result<ReadyUpdate, String> {
    stop.check()?;
    let http = craft_update::Http::new(&format!("{}/Update", UPDATE_APP.name))
        .map_err(|e| format!("could not initialize HTTP: {e}"))?;
    if release.assets.is_empty() {
        stop.check()?;
        if let Some(full) = craft_update::check_for_update(&UPDATE_APP, &http, version)
            .map_err(|e| format!("could not fetch release assets: {e}"))?
        {
            release = full;
        }
    }
    stop.check()?;
    let paths = craft_update::Paths::for_app(&UPDATE_APP)
        .map_err(|e| format!("could not find paths: {e}"))?;
    let platform = Platform::current().ok_or("unsupported platform for self update")?;
    let restart = craft_update::self_update::prepare_update(
        &UPDATE_APP,
        installed,
        &release,
        platform,
        &http,
        &paths,
        stop,
        &mut |_| {},
        &|| stop.check(),
        Some(version),
    )?;
    stop.check()?;
    Ok(ReadyUpdate::Real(restart))
}

/// The updater and whether this run checks at start.
pub struct Runtime {
    updater: Updater,
    mode: InstallMode,
    /// Release discovered by the background check thread (`(release, installs_itself)`),
    /// intercepted so CourseCraft owns the prompt and cancellable install worker.
    intercepted: Arc<Mutex<Option<(Release, bool)>>>,
    prompt: Option<StartupPrompt>,
    install: Option<ActiveInstall>,
    restart: Option<ReadyUpdate>,
    close_wanted: bool,
    local_status: Option<Status>,
    #[cfg(test)]
    install_runner: Option<InstallRunner>,
    /// The desktop host turns the check at start on (preference on, not CI); tests leave it off.
    enabled: bool,
    check_requested: bool,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::with_mode(detect_install_mode(), None)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.start_pending_update();
    }
}

/// The prompt's words in the interface language, through the editor's catalogs.
fn translate(text: &str, args: &[(&str, &str)]) -> String {
    coursecraft_ui_egui::i18n::translate_with(text, args)
}

impl Runtime {
    /// Builds a runtime for `mode`, optionally substituting `source` for GitHub API calls (tests).
    fn with_mode(mode: InstallMode, source: Option<CheckSource>) -> Self {
        Self::with_hooks(
            mode,
            source,
            #[cfg(test)]
            None,
            #[cfg(test)]
            None,
            #[cfg(test)]
            None,
        )
    }

    fn with_hooks(
        mode: InstallMode,
        source: Option<CheckSource>,
        #[cfg(test)] records_root: Option<&Path>,
        #[cfg(test)] install_probe: Option<InstallProbe>,
        #[cfg(test)] install_runner: Option<InstallRunner>,
    ) -> Self {
        let intercepted = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&intercepted);
        let wrapped: CheckSource = Arc::new(move || {
            let result = match &source {
                Some(ask) => ask(),
                None => craft_update::Http::new(&format!("{}/Update", UPDATE_APP.name))
                    .and_then(|http| craft_update::check_for_update(&UPDATE_APP, &http, VERSION)),
            };
            match result {
                Ok(Some(release)) => {
                    let installs_itself = if mode == InstallMode::CraftUpdate {
                        #[cfg(test)]
                        {
                            install_probe
                                .as_ref()
                                .map_or_else(|| can_install_itself(&release), |p| p(&release))
                        }
                        #[cfg(not(test))]
                        {
                            can_install_itself(&release)
                        }
                    } else {
                        false
                    };
                    *slot.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some((release, installs_itself));
                    Ok(None)
                }
                other => other,
            }
        });
        #[allow(unused_mut)]
        let mut updater = Updater::new(&UPDATE_APP, VERSION)
            .with_translator(translate)
            .with_source(wrapped);
        #[cfg(test)]
        if let Some(root) = records_root {
            updater = updater
                .with_paths(craft_update::Paths::under(root))
                .with_pins_path(root.join("pins.json"));
        }
        Self {
            updater,
            mode,
            intercepted,
            prompt: None,
            install: None,
            restart: None,
            close_wanted: false,
            local_status: None,
            #[cfg(test)]
            install_runner,
            enabled: false,
            check_requested: false,
        }
    }

    /// Turns the check at start on or off.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Help ▸ Check for Updates: runs a check and reports the outcome either way.
    pub fn request_check(&mut self) {
        if self.prompt.is_none() && !matches!(self.updater.status(), Status::Checking) {
            self.check_requested = true;
        }
    }

    /// Starts the helper of a ready, uncancelled update once process exit is certain.
    /// An in-flight download is cancelled instead of installing on exit. Later calls do nothing.
    pub fn start_pending_update(&mut self) {
        if let Some(active) = self.install.take() {
            active.stop.cancel();
        }
        if let Some(ready) = self.restart.take()
            && let Err(e) = ready.start()
        {
            eprintln!("CourseCraft self-update failed: {e}");
        }
        self.updater.start_pending_update();
    }

    /// Cancels any in-flight download/install so the app never closes, swaps binaries, or
    /// installs on exit.
    fn cancel_install(&mut self) {
        if let Some(active) = self.install.take() {
            active.stop.cancel();
        }
        self.restart = None;
        self.close_wanted = false;
        if matches!(self.local_status, Some(Status::Installing | Status::Ready)) {
            self.local_status = None;
        }
    }

    /// Runs the checks and polls the installation worker; call once a frame.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.check_requested) {
            if self.install.is_some() {
                self.local_status = Some(Status::Installing);
            } else if self.restart.is_some() {
                self.local_status = Some(Status::Ready);
            } else {
                self.local_status = None;
                self.updater.check_now(ctx);
            }
        } else if self.enabled {
            self.updater.check_at_start(ctx);
        }
        self.updater.tick(ctx);

        if self.updater.managed().is_some() {
            self.intercepted
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if self.install.is_none() && self.restart.is_none() {
                self.prompt = None;
            }
        } else if let Some((release, installs_itself)) = self
            .intercepted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            self.updater.dismiss_status();
            if self.install.is_none() && self.restart.is_none() && self.prompt.is_none() {
                self.prompt = Some(build_prompt(release, self.mode, installs_itself));
            }
        }

        self.poll_install(ctx);
    }

    fn poll_install(&mut self, ctx: &egui::Context) {
        let Some(active) = &self.install else {
            return;
        };
        let outcome = match active.rx.try_recv() {
            Ok(outcome) => Some(outcome),
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                None
            }
            Err(TryRecvError::Disconnected) => {
                Some(Err("The update stopped unexpectedly.".to_owned()))
            }
        };
        let Some(outcome) = outcome else {
            return;
        };
        let Some(active) = self.install.take() else {
            return;
        };
        // If cancelled while finishing, discard any result without closing or scheduling exit work.
        if active.stop.check().is_err() {
            return;
        }
        match outcome {
            Ok(ready) => {
                self.restart = Some(ready);
                self.close_wanted = true;
                if let Some(prompt) = &mut self.prompt {
                    prompt.is_installing = false;
                    prompt.is_ready = true;
                    prompt.set_note(installed_note());
                }
                if matches!(self.local_status, Some(Status::Installing)) {
                    self.local_status = Some(Status::Ready);
                }
            }
            Err(err) => {
                eprintln!("CourseCraft self-update failed: {err}");
                if matches!(self.local_status, Some(Status::Installing)) {
                    self.local_status = None;
                }
                if let Some(prompt) = &mut self.prompt {
                    prompt.is_installing = false;
                    prompt.set_message(err);
                }
            }
        }
    }

    /// Starts a cancellable self-update worker for the open prompt.
    fn start_install(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &mut self.prompt else {
            return;
        };
        if self.restart.is_some() {
            prompt.set_note(installed_note());
            self.close_wanted = true;
            return;
        }
        if self.install.is_some() {
            return;
        }
        let release = prompt.release.clone();
        let stop = Arc::new(Stop::default());
        let worker_stop = Arc::clone(&stop);
        let ctx_clone = ctx.clone();
        let (tx, rx) = channel();

        #[cfg(test)]
        let test_runner = self.install_runner.clone();

        #[cfg(test)]
        let installed = if test_runner.is_some() {
            None
        } else {
            craft_update::self_update::current(&UPDATE_APP)
        };
        #[cfg(not(test))]
        let installed = craft_update::self_update::current(&UPDATE_APP);

        #[cfg(test)]
        let has_runner = test_runner.is_some() || installed.is_some();
        #[cfg(not(test))]
        let has_runner = installed.is_some();

        if !has_runner {
            prompt.set_note(Note::new(
                "Cannot auto-update this installation type. Please download manually.",
                &[],
            ));
            prompt.is_installing = false;
            return;
        }

        prompt.is_installing = true;
        prompt.clear_message();
        let version = running_version();
        let spawned = std::thread::Builder::new()
            .name("self-update".into())
            .spawn(move || {
                let run = || {
                    #[cfg(test)]
                    if let Some(runner) = test_runner {
                        return runner(&release, Arc::clone(&worker_stop));
                    }
                    let Some(installed) = installed else {
                        return Err("Cannot auto-update this installation type.".to_owned());
                    };
                    run_self_update(&installed, release, &version, &worker_stop)
                };
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run))
                    .unwrap_or_else(|_| Err("The update failed unexpectedly.".to_owned()));
                // Only send when still active; if cancelled, drop any outcome silently.
                if worker_stop.check().is_ok() {
                    let _ = tx.send(outcome);
                }
                ctx_clone.request_repaint();
            });
        match spawned {
            Ok(_) => {
                self.install = Some(ActiveInstall { rx, stop });
            }
            Err(e) => {
                prompt.is_installing = false;
                prompt.set_note(Note::new(
                    "Couldn't start the update ({error})",
                    &[("error", &e.to_string())],
                ));
            }
        }
    }

    /// Draws the prompt and the result window. `clean`: nothing is unsaved, so an installation
    /// may start and a ready update may ask the app to close.
    pub fn show(&mut self, ctx: &egui::Context, clean: bool) {
        match self.show_action(ctx, clean) {
            Action::OpenUrl(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            // The app's close path asks about unsaved work; `on_exit` starts the helper.
            Action::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::None => {}
        }
    }

    /// Draws the prompt and returns the resulting action without executing OS side effects.
    fn show_action(&mut self, ctx: &egui::Context, clean: bool) -> Action {
        if clean && self.restart.is_some() && std::mem::take(&mut self.close_wanted) {
            return Action::Close;
        }
        let mut action = Action::None;
        if let Some(prompt) = &mut self.prompt {
            let response = prompt.show(ctx);
            match response {
                StartupPromptResponse::Pending => {}
                StartupPromptResponse::UpdateAndRestart => {
                    if self.mode == InstallMode::CraftUpdate {
                        if clean {
                            self.start_install(ctx);
                            if self.restart.is_some() && std::mem::take(&mut self.close_wanted) {
                                action = Action::Close;
                            }
                        } else {
                            prompt.set_note(save_first_note());
                        }
                    }
                }
                StartupPromptResponse::Download => {
                    let url = match self.mode {
                        InstallMode::CraftUpdate => {
                            release_download_url(&UPDATE_APP, &prompt.release)
                        }
                        InstallMode::WindowsSetup | InstallMode::WindowsPortable => {
                            windows_download_url_for(
                                &UPDATE_APP,
                                &prompt.release,
                                current_windows_arch(),
                                self.mode == InstallMode::WindowsPortable,
                            )
                        }
                    };
                    self.prompt = None;
                    action = url.map_or(Action::None, Action::OpenUrl);
                }
                StartupPromptResponse::Later => {
                    if self.install.is_some() {
                        self.cancel_install();
                    } else {
                        self.close_wanted = false;
                    }
                    self.prompt = None;
                }
            }
        }
        let status = self
            .local_status
            .as_ref()
            .unwrap_or_else(|| self.updater.status())
            .clone();
        if !matches!(status, Status::Idle) {
            let response = CheckResultDialog::new(&UPDATE_APP, VERSION)
                .with_translator(translate)
                .show(ctx, &status);
            self.answer(response);
        }
        action
    }

    /// Acts on the result window's answer.
    fn answer(&mut self, response: CheckResultResponse) {
        match response {
            CheckResultResponse::Close => {
                self.local_status = None;
                self.updater.dismiss_status();
            }
            CheckResultResponse::CheckAgain => {
                self.local_status = None;
                self.check_requested = true;
            }
            // The next frame's `show_action` asks the window to close once nothing is unsaved.
            CheckResultResponse::RestartToUpdate => {
                if self.restart.is_some() {
                    self.close_wanted = true;
                }
            }
            CheckResultResponse::None => {}
        }
    }

    #[cfg(test)]
    fn prompt_open(&self) -> bool {
        self.prompt.is_some()
    }

    #[cfg(test)]
    fn offer(&mut self, release: Release, installs_itself: bool) {
        if self.install.is_none() && self.restart.is_none() {
            self.prompt = Some(build_prompt(release, self.mode, installs_itself));
            self.local_status = None;
            self.updater.dismiss_status();
        }
    }
}

/// Whether `CI` or `COURSECRAFT_DISABLE_UPDATES` turn the check at start off.
#[must_use]
pub fn suppressed_by_env() -> bool {
    let set = |name: &str| {
        std::env::var(name).is_ok_and(|v| {
            let v = v.trim();
            !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
        })
    };
    set("CI") || set("COURSECRAFT_DISABLE_UPDATES")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    /// A runtime whose updater keeps its records under `root`, reads no launcher choice and asks
    /// no server, with the check at start on.
    fn runtime_with_records(root: &std::path::Path) -> Runtime {
        let mut rt = Runtime::with_hooks(
            InstallMode::CraftUpdate,
            Some(std::sync::Arc::new(|| Ok(None))),
            Some(root),
            None,
            None,
        );
        rt.enabled = true;
        rt
    }

    fn windows_runtime(
        root: &std::path::Path,
        mode: InstallMode,
        release: Option<Release>,
    ) -> Runtime {
        Runtime::with_hooks(
            mode,
            Some(Arc::new(move || Ok(release.clone()))),
            Some(root),
            None,
            None,
        )
    }

    fn records_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("coursecraft-updates-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn frame(rt: &mut Runtime, ctx: &egui::Context) -> Action {
        frame_with(rt, ctx, egui::RawInput::default(), true)
    }

    fn frame_with(
        rt: &mut Runtime,
        ctx: &egui::Context,
        input: egui::RawInput,
        clean: bool,
    ) -> Action {
        let mut action = Action::None;
        ctx.run_ui(input, |ui| {
            rt.tick(ui.ctx());
            action = rt.show_action(ui.ctx(), clean);
        })
        .textures_delta
        .clear();
        action
    }

    fn release_0_5_0() -> Release {
        let tag = "v0.5.0";
        let repo = UPDATE_APP.repo;
        let names = [
            "coursecraft-0.5.0-macos-arm64.dmg",
            "coursecraft-0.5.0-windows-x64-setup.exe",
            "coursecraft-0.5.0-linux-x86_64.tar.gz",
            "coursecraft-0.5.0-linux-x86_64.AppImage",
            "SHA256SUMS.txt",
        ];
        Release {
            tag: tag.into(),
            version: craft_update::parse_version("0.5.0"),
            name: "CourseCraft v0.5.0".into(),
            html_url: format!("https://github.com/{repo}/releases/tag/{tag}"),
            repo: repo.into(),
            assets: names
                .iter()
                .map(|n| Asset {
                    name: (*n).into(),
                    size: 1,
                    url: format!("https://github.com/{repo}/releases/download/{tag}/{n}"),
                    sha256: None,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn the_app_is_described_by_its_upstream_repository() {
        assert_eq!(UPDATE_APP.repo, "ad1-kamath/coursecraft");
        assert_eq!(
            UPDATE_APP.releases_url(),
            "https://github.com/ad1-kamath/coursecraft/releases"
        );
    }

    /// The file names release.yml and package-macos.sh make are the ones craft-update and
    /// CourseCraft's Windows selector pick, with no portable Windows package.
    #[test]
    fn the_release_file_names_are_the_ones_the_update_picks() {
        let release = release_0_5_0();
        let stems: Vec<&str> = UPDATE_APP.stems().collect();
        for (os, arch, kind, expected) in [
            (
                Os::MacOs,
                Arch::Arm64,
                PackageKind::Dmg,
                "coursecraft-0.5.0-macos-arm64.dmg",
            ),
            (
                Os::Linux,
                Arch::X64,
                PackageKind::TarGz,
                "coursecraft-0.5.0-linux-x86_64.tar.gz",
            ),
            (
                Os::Linux,
                Arch::X64,
                PackageKind::AppImage,
                "coursecraft-0.5.0-linux-x86_64.AppImage",
            ),
        ] {
            let picked = pick_asset(&release, &stems, Platform { os, arch }, &[kind]);
            assert_eq!(picked.map(|(a, _)| a.name.as_str()), Some(expected));
        }
        assert_eq!(
            select_windows_setup_asset(&release, &stems, Arch::X64).map(|a| a.name.as_str()),
            Some("coursecraft-0.5.0-windows-x64-setup.exe")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let yml = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
        for part in [
            "coursecraft-$ver-windows-x64-setup",
            "coursecraft-${GITHUB_REF_NAME#v}-linux-x86_64",
            "coursecraft-$ver-linux-x86_64.AppImage",
            "$root/bin/coursecraft",
            "packaging/linux/coursecraft.desktop",
            "apps/coursecraft/icons/coursecraft-1024.png",
            "sha256sum coursecraft-* > SHA256SUMS.txt",
        ] {
            assert!(yml.contains(part), "release.yml lacks {part}");
        }
        // The portable zip carries portable.txt and is named the way the portable picker reads.
        for part in [
            "portable.txt",
            "coursecraft-$ver-windows-x64-portable",
            "dist\\$name.zip",
        ] {
            assert!(yml.contains(part), "release.yml lacks {part}");
        }
        let mut with_zip = release.clone();
        let zip = "coursecraft-0.5.0-windows-x64-portable.zip";
        with_zip.assets.push(Asset {
            name: zip.into(),
            size: 1,
            url: String::new(),
            sha256: None,
        });
        assert_eq!(
            select_windows_portable_asset(&with_zip, &stems, Arch::X64).map(|a| a.name.as_str()),
            Some(zip)
        );
        assert_eq!(
            select_windows_setup_asset(&with_zip, &stems, Arch::X64).map(|a| a.name.as_str()),
            Some("coursecraft-0.5.0-windows-x64-setup.exe")
        );
        let mac = std::fs::read_to_string(root.join("scripts/package-macos.sh")).unwrap();
        assert!(mac.contains("coursecraft-$version-macos-$arch.dmg"));
    }

    #[test]
    fn linux_asset_selection_uses_appimage_when_env_set_and_tarball_otherwise() {
        let release = release_0_5_0();

        let (asset, kind) = select_linux_asset(
            &release,
            Arch::X64,
            Some(OsStr::new(
                "/home/user/Applications/coursecraft-0.4.6-linux-x86_64.AppImage",
            )),
        )
        .expect("AppImage selected when APPIMAGE is set");
        assert_eq!(kind, PackageKind::AppImage);
        assert_eq!(asset.name, "coursecraft-0.5.0-linux-x86_64.AppImage");

        let (asset, kind) =
            select_linux_asset(&release, Arch::X64, None).expect("tarball when APPIMAGE unset");
        assert_eq!(kind, PackageKind::TarGz);
        assert_eq!(asset.name, "coursecraft-0.5.0-linux-x86_64.tar.gz");

        // An empty APPIMAGE variable does not count as running from an AppImage.
        let (asset, kind) = select_linux_asset(&release, Arch::X64, Some(OsStr::new("")))
            .expect("tarball when APPIMAGE empty");
        assert_eq!(kind, PackageKind::TarGz);
        assert_eq!(asset.name, "coursecraft-0.5.0-linux-x86_64.tar.gz");

        // Wrong architecture never falls back to x86_64 on Linux.
        assert_eq!(select_linux_asset(&release, Arch::Arm64, None), None);
        assert_eq!(
            select_linux_asset(&release, Arch::Arm64, Some(OsStr::new("/tmp/a.AppImage"))),
            None
        );
    }

    #[test]
    fn windows_setup_selection_never_picks_portable_and_validates_urls() {
        let mut release = release_0_5_0();
        // Even if an older release lists a portable zip first, Windows selection picks -setup.exe.
        release.assets.insert(
            0,
            Asset {
                name: "coursecraft-0.5.0-windows-x64-portable.zip".into(),
                size: 1,
                url: "https://github.com/ad1-kamath/coursecraft/releases/download/v0.5.0/coursecraft-0.5.0-windows-x64-portable.zip".into(),
                sha256: None,
            },
        );
        let url = windows_download_url(&UPDATE_APP, &release, Arch::X64).unwrap();
        assert_eq!(
            url,
            "https://github.com/ad1-kamath/coursecraft/releases/download/v0.5.0/coursecraft-0.5.0-windows-x64-setup.exe"
        );
        // Windows ARM64 falls back to x64 installer cleanly.
        assert_eq!(
            windows_download_url(&UPDATE_APP, &release, Arch::Arm64).as_deref(),
            Some(url.as_str())
        );

        // If a release has a forged asset URL, refuse it and fall back to the release HTML page.
        let mut forged = release_0_5_0();
        for asset in &mut forged.assets {
            if asset.name.ends_with("-setup.exe") {
                asset.url = "https://evil.example/coursecraft-0.5.0-windows-x64-setup.exe".into();
            }
        }
        assert_eq!(
            windows_download_url(&UPDATE_APP, &forged, Arch::X64).as_deref(),
            Some("https://github.com/ad1-kamath/coursecraft/releases/tag/v0.5.0")
        );

        // If no -setup.exe asset exists at all, fall back to the release page without error.
        let mut no_setup = release_0_5_0();
        no_setup.assets.retain(|a| !a.name.ends_with("-setup.exe"));
        assert_eq!(
            windows_download_url(&UPDATE_APP, &no_setup, Arch::X64).as_deref(),
            Some("https://github.com/ad1-kamath/coursecraft/releases/tag/v0.5.0")
        );
    }

    #[test]
    fn portable_marker_detection_checks_sibling_portable_txt() {
        let dir = records_dir("portable-marker");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("coursecraft.exe");
        std::fs::write(&exe, b"exe").unwrap();
        assert!(!is_portable_at(&exe));
        std::fs::write(dir.join("portable.txt"), b"").unwrap();
        assert!(is_portable_at(&exe));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_portable_offers_portable_zip_with_notice_and_no_error() {
        let root = records_dir("win-portable");
        let mut release = release_0_5_0();
        let name = "coursecraft-0.5.0-windows-x64-portable.zip";
        release.assets.push(Asset {
            name: name.into(),
            size: 1,
            url: format!(
                "https://github.com/ad1-kamath/coursecraft/releases/download/v0.5.0/{name}"
            ),
            sha256: None,
        });
        let mut rt = windows_runtime(&root, InstallMode::WindowsPortable, Some(release));
        let ctx = egui::Context::default();
        rt.request_check();
        for _ in 0..200 {
            let action = frame(&mut rt, &ctx);
            assert_eq!(action, Action::None, "must never silently install or close");
            if rt.prompt_open() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let prompt = rt
            .prompt
            .as_ref()
            .expect("Windows portable copy should open the update prompt");
        assert!(
            !prompt.is_self_updating,
            "must not attempt PortableZip self-update"
        );
        assert_eq!(rt.updater.status(), &Status::Idle, "must not show an error");
        let msg = prompt
            .message()
            .expect("portable prompt must explain how to update a portable folder");
        assert!(msg.contains("portable"), "{msg}");
        assert!(msg.contains(".zip"), "{msg}");

        // Choosing Download points straight to the portable zip; installed copies still get
        // the installer.
        let url = windows_download_url_for(&UPDATE_APP, &prompt.release, Arch::X64, true).unwrap();
        assert!(url.ends_with(name), "{url}");
        let setup = windows_download_url(&UPDATE_APP, &prompt.release, Arch::X64).unwrap();
        assert!(setup.ends_with("-setup.exe"), "{setup}");

        // A release without a portable zip falls back to the release page.
        let fallback =
            windows_download_url_for(&UPDATE_APP, &release_0_5_0(), Arch::X64, true).unwrap();
        assert_eq!(
            fallback,
            "https://github.com/ad1-kamath/coursecraft/releases/tag/v0.5.0"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_setup_offers_setup_exe_without_portable_notice_or_silent_restart() {
        let root = records_dir("win-setup");
        let mut rt = windows_runtime(&root, InstallMode::WindowsSetup, Some(release_0_5_0()));
        let ctx = egui::Context::default();
        rt.request_check();
        for _ in 0..200 {
            let action = frame(&mut rt, &ctx);
            assert_eq!(action, Action::None, "must never silently install or close");
            if rt.prompt_open() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let prompt = rt
            .prompt
            .as_ref()
            .expect("Windows installer copy should open the update prompt");
        assert!(!prompt.is_self_updating);
        assert_eq!(rt.updater.status(), &Status::Idle);
        assert_eq!(prompt.message(), None);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_offered_release_opens_the_prompt() {
        let mut rt = Runtime::with_mode(InstallMode::CraftUpdate, None);
        let ctx = egui::Context::default();
        assert!(!rt.prompt_open());
        rt.offer(
            craft_update::Release {
                tag: "v99.0.0".into(),
                ..Default::default()
            },
            false,
        );
        ctx.run_ui(egui::RawInput::default(), |ui| rt.show(ui.ctx(), true))
            .textures_delta
            .clear();
        assert!(rt.prompt_open());
        // Offering an update never silently installs or closes the app.
        assert_eq!(rt.updater.status(), &Status::Idle);
    }

    fn wait_for_prompt(rt: &mut Runtime, ctx: &egui::Context) {
        for _ in 0..200 {
            assert_eq!(frame(rt, ctx), Action::None);
            if rt.prompt_open() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("prompt did not open in time");
    }

    fn wait_until(pred: impl Fn() -> bool, msg: &str) {
        for _ in 0..200 {
            if pred() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("{msg}");
    }

    #[test]
    fn closing_prompt_while_downloading_cancels_update_and_allows_retry() {
        let root = records_dir("cancel-inflight");
        let attempts = Arc::new(AtomicUsize::new(0));
        let swapped = Arc::new(AtomicUsize::new(0));
        let cancelled_seen = Arc::new(AtomicBool::new(false));
        let installed_on_exit = Arc::new(AtomicBool::new(false));
        let allow_finish = Arc::new(AtomicBool::new(false));

        let runner: InstallRunner = {
            let attempts = Arc::clone(&attempts);
            let swapped = Arc::clone(&swapped);
            let cancelled_seen = Arc::clone(&cancelled_seen);
            let installed_on_exit = Arc::clone(&installed_on_exit);
            let allow_finish = Arc::clone(&allow_finish);
            Arc::new(move |_release, stop| {
                attempts.fetch_add(1, Ordering::SeqCst);
                while !allow_finish.load(Ordering::SeqCst) {
                    if let Err(err) = stop.check() {
                        cancelled_seen.store(true, Ordering::SeqCst);
                        return Err(err);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                if let Err(err) = stop.check() {
                    cancelled_seen.store(true, Ordering::SeqCst);
                    return Err(err);
                }
                swapped.fetch_add(1, Ordering::SeqCst);
                Ok(ReadyUpdate::Simulated(Arc::clone(&installed_on_exit)))
            })
        };

        let mut rt = Runtime::with_hooks(
            InstallMode::CraftUpdate,
            Some(Arc::new(|| Ok(Some(release_0_5_0())))),
            Some(&root),
            Some(Arc::new(|_| true)),
            Some(runner),
        );
        let ctx = egui::Context::default();

        // 1. Discover update and start an explicit Update and Restart.
        rt.request_check();
        wait_for_prompt(&mut rt, &ctx);
        assert!(rt.prompt.as_ref().is_some_and(|p| p.is_self_updating));

        rt.start_install(&ctx);
        assert!(rt.prompt.as_ref().is_some_and(|p| p.is_installing));
        wait_until(
            || attempts.load(Ordering::SeqCst) == 1,
            "first install attempt did not start",
        );

        // 2. While download is in flight, close the prompt via Escape (same StartupPromptResponse::Later
        //    as clicking Later / Dismiss / ×).
        let esc = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        assert_eq!(frame_with(&mut rt, &ctx, esc, true), Action::None);
        assert!(!rt.prompt_open(), "prompt closes immediately");
        assert!(rt.install.is_none(), "in-flight install handle is dropped");
        wait_until(
            || cancelled_seen.load(Ordering::SeqCst),
            "in-flight worker must receive Stop::cancel()",
        );

        // Even after ticking and exiting, the app must NOT close, swap, or install on exit.
        for _ in 0..5 {
            assert_eq!(frame(&mut rt, &ctx), Action::None);
        }
        assert_eq!(swapped.load(Ordering::SeqCst), 0, "binary must not swap");
        rt.start_pending_update();
        assert!(
            !installed_on_exit.load(Ordering::SeqCst),
            "cancelled update must not install on exit"
        );

        // 3. A later explicit Update and Restart can start again and complete cleanly.
        allow_finish.store(true, Ordering::SeqCst);
        rt.request_check();
        wait_for_prompt(&mut rt, &ctx);
        rt.start_install(&ctx);

        let mut close_requested = false;
        for _ in 0..200 {
            if frame(&mut rt, &ctx) == Action::Close {
                close_requested = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(swapped.load(Ordering::SeqCst), 1);
        assert!(
            close_requested,
            "explicit retry must complete and ask to close"
        );
        rt.start_pending_update();
        assert!(
            installed_on_exit.load(Ordering::SeqCst),
            "explicit retry must start pending update on exit"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_start_after_an_update_reports_it_until_dismissed() {
        let root = records_dir("installed");
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("data").join("self-update-done.txt"), VERSION).unwrap();
        let mut rt = runtime_with_records(&root);
        let ctx = egui::Context::default();
        frame(&mut rt, &ctx);
        assert!(
            matches!(rt.updater.status(), Status::Updated(_)),
            "{:?}",
            rt.updater.status()
        );
        rt.answer(CheckResultResponse::Close);
        assert_eq!(rt.updater.status(), &Status::Idle);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_requested_check_with_nothing_newer_says_so() {
        let root = records_dir("uptodate");
        let mut rt = runtime_with_records(&root);
        rt.enabled = false;
        let ctx = egui::Context::default();
        rt.request_check();
        for _ in 0..200 {
            frame(&mut rt, &ctx);
            if !matches!(rt.updater.status(), Status::Checking) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(rt.updater.status(), &Status::UpToDate);
        rt.answer(CheckResultResponse::CheckAgain);
        assert!(rt.check_requested);
        let _ = std::fs::remove_dir_all(&root);
    }
}
