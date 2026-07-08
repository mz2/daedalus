//! The GPUI render loop (only compiled with `--features gpui`), built on the
//! **gpui-component** widget library so the surface matches the locked design (`design/`).
//!
//! It wires the operator-facing capabilities into the window: a Start-session flow, session
//! monitoring + control + send-input, tool registration, fleet/tasks views, and settings —
//! all driven through the `daedalus_app::App` command/query API. Needs a GPU/display
//! (research R-UI); live state refreshes on a timer, commands run on the tokio runtime.

use gpui::{
    div, prelude::*, px, App as GpuiApp, Bounds, Context, Edges, Entity, FocusHandle, Focusable,
    Rgba, SharedString, Window, WindowBounds, WindowKind, WindowOptions,
};
use gpui_component::input::{Input, InputState};
use gpui_component::sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem};
use gpui_component::{button::Button, button::ButtonVariants};
use gpui_component::{h_flex, v_flex, Disableable, IconName, Root, StyledExt, TitleBar};
use gpui_platform::application;
use gpui_terminal::{ColorPalette, TerminalConfig, TerminalView};
use tokio::runtime::Handle;

use std::io::{self, Read, Write};

use bytes::Bytes;
use daedalus_app::{App, AppQuery, AppQueryAsync, Command};
use daedalus_proto::{
    ArtifactRef, Availability, BackendKind, Capabilities, DiscoveredSession, InvocationSpec,
    Objective, ObjectiveId, Origin, PromptConvention, SessionId, SessionStatus,
    StartSessionRequest, TaskStatus, ToolDef, ToolId, WorktreeRef,
};

use crate::app::{HostsIndicator, NavItem, StatusCounts};
use crate::components::{availability_text, ActionButton, ButtonIntent, StatusBadge};
use crate::screens::backends::BackendsView;
use crate::screens::needs::NeedsView;
use crate::screens::session::{BannerTone, SessionFocus, SessionView, TimelineKind};
use crate::screens::settings::SettingsView;
use crate::screens::tasks::{TaskFilters, TasksBoardView};
use crate::theme::{Color, OsAppearance, StatusTone, Theme as Palette, ThemePreference};

/// Convert a design-system [`Color`] to a GPUI color.
fn col(c: Color) -> Rgba {
    gpui::rgb(((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32))
}

/// Map the window's platform appearance onto the design system's [`OsAppearance`], so the
/// "System" theme preference can follow the OS (FR-009a).
fn os_appearance(window: &Window) -> OsAppearance {
    match window.appearance() {
        gpui::WindowAppearance::Light | gpui::WindowAppearance::VibrantLight => OsAppearance::Light,
        gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark => OsAppearance::Dark,
    }
}

/// A monospace family that is actually present on the host (so fixed-cell terminal/text
/// rendering doesn't fall back to a proportional font and mangle spacing). The design's
/// JetBrains Mono is preferred where installed, but we name a guaranteed system font.
fn mono_family() -> SharedString {
    if cfg!(target_os = "macos") {
        "Menlo".into()
    } else if cfg!(target_os = "windows") {
        "Consolas".into()
    } else {
        "DejaVu Sans Mono".into()
    }
}

/// What the content area is showing.
#[derive(Clone, Copy, PartialEq)]
enum View {
    Nav(NavItem),
    Start,
    Session(SessionId),
}

/// All the text-input entities the forms use (created once, with the window in scope).
struct Inputs {
    send: Entity<InputState>,
    objective: Entity<InputState>,
    tasks_path: Entity<InputState>,
    worktree: Entity<InputState>,
    tool_name: Entity<InputState>,
    tool_program: Entity<InputState>,
    tool_args: Entity<InputState>,
    concurrency: Entity<InputState>,
    stall: Entity<InputState>,
    idle_rate: Entity<InputState>,
    openshell_image: Entity<InputState>,
}

/// Open the Daedalus window and run the GPUI event loop (blocks the main thread).
pub fn run(app: App, handle: Handle) {
    ensure_tool(&app);
    let sample_tasks = std::env::temp_dir().join("daedalus-sample-tasks.md");
    let _ = std::fs::write(
        &sample_tasks,
        "- [x] T001 Provision environment\n- [ ] T002 Implement feature\n- [ ] T003 Run tests\n",
    );
    let sample_tasks = sample_tasks.to_string_lossy().into_owned();

    let launched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        application()
            .with_assets(gpui_component_assets::Assets)
            .run(move |cx: &mut GpuiApp| {
                gpui_component::init(cx);
                // Dark by default (the locked design); `AppRoot::new` → `apply_theme`
                // resolves the preference and tints the platform accent in before the
                // first frame.
                gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);

                let bounds = Bounds::centered(None, gpui::size(px(1240.0), px(820.0)), cx);
                let options = WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitleBar::title_bar_options()),
                    window_min_size: Some(gpui::Size {
                        width: px(820.0),
                        height: px(520.0),
                    }),
                    kind: WindowKind::Normal,
                    #[cfg(target_os = "linux")]
                    window_background: gpui::WindowBackgroundAppearance::Transparent,
                    #[cfg(target_os = "linux")]
                    window_decorations: Some(gpui::WindowDecorations::Client),
                    ..Default::default()
                };

                let app = app.clone();
                let handle = handle.clone();
                let sample_tasks = sample_tasks.clone();
                cx.open_window(options, move |window, cx| {
                    let inputs = Inputs {
                        send: cx.new(|cx| {
                            InputState::new(window, cx).placeholder("Send input to the agent…")
                        }),
                        objective: cx.new(|cx| {
                            InputState::new(window, cx).placeholder("Objective description")
                        }),
                        tasks_path: cx
                            .new(|cx| InputState::new(window, cx).placeholder("Path to tasks.md")),
                        worktree: cx.new(|cx| {
                            InputState::new(window, cx).placeholder("Worktree path (pre-existing)")
                        }),
                        tool_name: cx
                            .new(|cx| InputState::new(window, cx).placeholder("Tool name")),
                        tool_program: cx.new(|cx| {
                            InputState::new(window, cx).placeholder("Program (e.g. bash)")
                        }),
                        tool_args: cx.new(|cx| {
                            InputState::new(window, cx).placeholder("Args (space-separated)")
                        }),
                        concurrency: cx.new(|cx| {
                            InputState::new(window, cx)
                                .placeholder("Concurrency limit (blank = unlimited)")
                        }),
                        stall: cx.new(|cx| {
                            InputState::new(window, cx).placeholder("Stall interval (seconds)")
                        }),
                        idle_rate: cx.new(|cx| {
                            InputState::new(window, cx)
                                .placeholder("Idle rate $/hr (blank = no estimate)")
                        }),
                        openshell_image: cx.new(|cx| {
                            InputState::new(window, cx)
                                .placeholder("e.g. daedalus/e2e-openshell:latest")
                        }),
                    };
                    // Pre-fill sensible defaults.
                    inputs
                        .objective
                        .update(cx, |s, cx| s.set_value("Demo objective", window, cx));
                    inputs
                        .tasks_path
                        .update(cx, |s, cx| s.set_value(sample_tasks.clone(), window, cx));
                    let cfg = app.core().config();
                    inputs.stall.update(cx, |s, cx| {
                        s.set_value(cfg.stall_interval_secs.to_string(), window, cx)
                    });
                    // Seed the persisted OpenShell image so Settings shows what is in
                    // force (store value, else the env fallback the backend would use).
                    let image = app
                        .core()
                        .backend_image(daedalus_proto::BackendKind::OpenShell)
                        .or_else(|| std::env::var("DAEDALUS_OPENSHELL_FROM").ok());
                    if let Some(image) = image.filter(|s| !s.is_empty()) {
                        inputs
                            .openshell_image
                            .update(cx, |s, cx| s.set_value(image, window, cx));
                    }
                    if let Some(limit) = cfg.concurrency_limit {
                        inputs
                            .concurrency
                            .update(cx, |s, cx| s.set_value(limit.to_string(), window, cx));
                    }
                    // Pre-fill the persisted idle rate for the default backend (FR-021b).
                    if let Some(rate) = app
                        .core()
                        .backend_registry()
                        .idle_rate(crate::default_backend_kind())
                    {
                        inputs
                            .idle_rate
                            .update(cx, |s, cx| s.set_value(format!("{rate}"), window, cx));
                    }

                    let os = os_appearance(window);
                    let root =
                        cx.new(|cx| AppRoot::new(app.clone(), handle.clone(), inputs, os, cx));
                    // Follow the OS appearance while the theme preference is System
                    // (FR-009a): re-resolve whenever the platform appearance changes.
                    window
                        .observe_window_appearance({
                            let root = root.clone();
                            move |window, cx| {
                                let os = os_appearance(window);
                                root.update(cx, |this, cx| {
                                    this.os_appearance = os;
                                    if this.theme_pref == ThemePreference::System {
                                        this.apply_theme(cx);
                                    }
                                });
                            }
                        })
                        .detach();
                    cx.new(|cx| Root::new(root, window, cx))
                })
                .expect("open window");
                cx.activate(true);
            });
    }));

    if launched.is_err() {
        eprintln!(
            "\nDaedalus: could not initialize the native GPUI window — its runtime libraries \
             (libwayland/libvulkan/libxkbcommon/libGL) were not found.\n\n\
             Fix: run inside the project's Nix dev shell, which exposes them:\n    \
             nix develop\n    DAEDALUS_BACKEND=fake cargo run -p daedalus-desktop --features gpui\n\n\
             Or, without Nix flakes:  source scripts/gpui-env.sh  (then re-run).\n"
        );
        std::process::exit(1);
    }
}

/// Register the demo tool (idempotent) so the tool picker is never empty.
fn ensure_tool(app: &App) {
    let def = ToolDef {
        name: "claude".to_string(),
        invocation: InvocationSpec {
            program: "bash".to_string(),
            args: vec!["-lc".to_string(), "echo working...; sleep 5".to_string()],
            env: Vec::new(),
        },
        capabilities: Capabilities {
            accepts_interactive_input: true,
            // Recognise the scripted live terminal's trailing prompt so a blocked agent is
            // surfaced as waiting-for-input (FR-015b) on the fake local-testing path.
            prompt_convention: Some(PromptConvention::PromptPattern(
                r"(?i)(Proceed with .*\?)".to_string(),
            )),
        },
    };
    // An Err means the tool already exists from a previous launch — fine either way.
    let _ = app.core().register_tool(def);
}

struct AppRoot {
    app: App,
    handle: Handle,
    palette: Palette,
    view: View,
    inputs: Inputs,
    // Start-form selections.
    start_tool: Option<ToolId>,
    start_origin: Origin,
    start_backend: BackendKind,
    /// Whether a launch was attempted with invalid fields (drives the footer-hint error
    /// copy, prototype `start-foot-hint`).
    start_attempted: bool,
    // Tool-form toggle.
    tool_accepts_input: bool,
    // Theme preference (System follows the OS appearance — FR-009a, default).
    theme_pref: ThemePreference,
    os_appearance: OsAppearance,
    // Latest discovered sessions (refreshed on the timer via the tokio runtime).
    discovered: Vec<DiscoveredSession>,
    // Shell hosts indicator + backends screen data (refreshed on the same timer; FR-028).
    hosts: HostsIndicator,
    backends_view: BackendsView,
    // Cached per refresh tick (and on push events), NOT per frame: the ~30fps terminal
    // pump repaints the whole window, so the titlebar counts and Needs-you queue must
    // not re-scan the fleet on every paint.
    counts: StatusCounts,
    needs: NeedsView,
    // The embedded terminal for the open session: renders the core's **redacted** capture
    // stream (via `App::attach`), and routes operator keystrokes back as `SendInput`.
    terminal: Option<Entity<TerminalView>>,
    // A stated not-attachable reason when the attach failed (contract C-T2 — never a silent
    // blank / a local shell fallback).
    terminal_error: Option<SharedString>,
    // Deep-link focus for the open session (answer mode, T078); cleared on plain opens.
    session_focus: Option<SessionFocus>,
    // The last dispatched-command failure, shown as a dismissable banner in the shell
    // chrome (G1) — set when a command is rejected, cleared on dismiss / next success.
    last_error: Option<SharedString>,
    focus_handle: FocusHandle,
}

impl AppRoot {
    fn new(
        app: App,
        handle: Handle,
        inputs: Inputs,
        os: OsAppearance,
        cx: &mut Context<Self>,
    ) -> Self {
        // Refresh live state ~2×/sec: re-poll discovery + host/backend availability on
        // the tokio runtime, then notify. Discovery and backend statuses are fetched
        // ONCE per tick (concurrently) and fed to every consumer — the previous shape
        // re-fetched both inside `HostsIndicator::build`/`BackendsView::build`.
        let app_bg = app.clone();
        let handle_bg = handle.clone();
        cx.spawn(async move |this, cx| loop {
            let a = app_bg.clone();
            let snapshot = handle_bg
                .spawn(async move {
                    let (discovered, backend_statuses) = tokio::join!(a.discovered(), a.backends());
                    // Keep task boards live for sessions whose run is still on — moved
                    // off the render path (re-parsing tasks.md per frame was wasted).
                    for s in a.fleet() {
                        if !s.status.has_ended() {
                            let _ = a.core().refresh_task_board(s.id);
                        }
                    }
                    let hosts = HostsIndicator::from_data(&backend_statuses, &discovered);
                    let backends = BackendsView::from_data(
                        backend_statuses,
                        a.core().environments().unwrap_or_default(),
                        &Palette::host_default(),
                    );
                    let counts = StatusCounts::from_app(&a);
                    let needs = NeedsView::build(&a);
                    (discovered, hosts, backends, counts, needs)
                })
                .await;
            if let Ok((discovered, hosts, backends, counts, needs)) = snapshot {
                let _ = this.update(cx, |this, cx| {
                    this.discovered = discovered;
                    this.hosts = hosts;
                    this.backends_view = backends;
                    this.counts = counts;
                    this.needs = needs;
                    cx.notify();
                });
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
        })
        .detach();

        // Push events refresh the cached shell state immediately, so operator actions
        // (Confirm/Stop/…) reflect in the titlebar counts and Needs-you queue without
        // waiting for the next tick. Output chunks are skipped — they stream constantly
        // and the terminal pump already repaints for them.
        let mut events = app.subscribe();
        cx.spawn(async move |this, cx| loop {
            match events.recv().await {
                Ok(daedalus_app::AppEvent::Output { .. }) => continue,
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
            if this
                .update(cx, |this, cx| {
                    this.counts = StatusCounts::from_app(&this.app);
                    this.needs = NeedsView::build(&this.app);
                    cx.notify();
                })
                .is_err()
            {
                break;
            }
        })
        .detach();

        // While a terminal is open, pump repaints at ~30fps so its background PTY-reader
        // task is drained promptly; otherwise it would only redraw on other window events.
        cx.spawn(async move |this, cx| loop {
            let has_term = match this.update(cx, |this, cx| {
                let open = this.terminal.is_some();
                if open {
                    cx.notify();
                }
                open
            }) {
                Ok(open) => open,
                Err(_) => break,
            };
            cx.background_executor()
                .timer(std::time::Duration::from_millis(if has_term {
                    33
                } else {
                    200
                }))
                .await;
        })
        .detach();

        let counts = StatusCounts::from_app(&app);
        let needs = NeedsView::build(&app);
        let mut root = Self {
            app,
            handle,
            palette: Palette::host_default(),
            // The aggregate Tasks board is the landing view (FR-025a).
            view: View::Nav(NavItem::home()),
            inputs,
            start_tool: None,
            start_origin: Origin::Fresh,
            start_backend: BackendKind::Fake,
            start_attempted: false,
            tool_accepts_input: true,
            theme_pref: ThemePreference::default(),
            os_appearance: os,
            discovered: Vec::new(),
            hosts: HostsIndicator { rows: Vec::new() },
            backends_view: BackendsView {
                rows: Vec::new(),
                reassurance: None,
                environments: Vec::new(),
            },
            counts,
            needs,
            terminal: None,
            terminal_error: None,
            session_focus: None,
            last_error: None,
            focus_handle: cx.focus_handle(),
        };
        // System is the default theme preference: adopt the OS appearance at launch
        // (FR-009a, T084).
        root.apply_theme(cx);
        root
    }

    /// Resolve the theme preference against the current OS appearance and re-apply the
    /// gpui-component theme (keeping the platform accent tint).
    fn apply_theme(&mut self, cx: &mut Context<Self>) {
        let mode = match self.theme_pref.resolve(self.os_appearance) {
            crate::theme::ThemeMode::Light => gpui_component::ThemeMode::Light,
            crate::theme::ThemeMode::Dark => gpui_component::ThemeMode::Dark,
        };
        gpui_component::Theme::change(mode, None, cx);
        let accent: gpui::Hsla = col(crate::theme::Skin::from_host_os().accent()).into();
        let on_accent: gpui::Hsla = gpui::rgb(0xffffff).into();
        let theme = gpui_component::Theme::global_mut(cx);
        theme.primary = accent;
        theme.primary_foreground = on_accent;
        theme.sidebar_primary = accent;
        theme.sidebar_primary_foreground = on_accent;
        cx.notify();
    }

    /// Open a session's detail and attach its embedded terminal to the core capture stream.
    fn open_session(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        self.open_session_focused(id, None, window, cx);
    }

    /// Open a session's detail via a deep link, landing with answer-mode focus (T078).
    ///
    /// Attaching drives the real core capture path (`App::attach` → redact → persist →
    /// observe → stream). The returned **redacted** output is bridged onto the terminal
    /// view's reader; operator keystrokes are routed back through `Command::SendInput`
    /// (never a local shell, never a raw PTY). A non-attachable session shows its stated
    /// reason instead of a blank pane (contract C-T2).
    fn open_session_focused(
        &mut self,
        id: SessionId,
        focus: Option<SessionFocus>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_focus = focus;
        self.view = View::Session(id);
        self.sample_usage(id);
        self.close_terminal();

        // Answer-mode deep link (T078, Part D / FR-021a): when the session is genuinely
        // waiting for input, honor the computed `SessionView.with_focus(...)` by retitling
        // the send field's placeholder to the answer-mode text and focusing it.
        if self.session_focus == Some(SessionFocus::AnswerPrompt) {
            if let Some(vm) = SessionView::build(&self.app, id, &self.palette)
                .map(|vm| vm.with_focus(self.session_focus))
            {
                if vm.focus_target == Some(SessionFocus::AnswerPrompt) {
                    let placeholder = vm.input_placeholder.clone();
                    self.inputs.send.update(cx, |st, cx| {
                        st.set_placeholder(placeholder, window, cx);
                        st.focus(window, cx);
                    });
                }
            }
        }

        self.attach_terminal(id, cx);

        cx.notify();
    }

    /// Attach the embedded terminal for `id` through the core capture path — used by both
    /// the open-session path and the start-flow's jump into a freshly started session.
    fn attach_terminal(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let app = self.app.clone();
        let handle = self.handle.clone();
        cx.spawn(async move |this, cx| {
            // Attach off the render thread, then bridge the async redacted output onto a
            // blocking reader the terminal view drains in its own thread.
            let attached = handle
                .spawn(async move {
                    match app.attach(id).await {
                        Ok(mut channel) => {
                            let resize = channel.resize.clone();
                            let (tx, rx) = std::sync::mpsc::channel::<Bytes>();
                            tokio::spawn(async move {
                                while let Some(chunk) = channel.output.recv().await {
                                    if tx.send(chunk).is_err() {
                                        break;
                                    }
                                }
                            });
                            Ok((rx, resize))
                        }
                        Err(e) => Err(e.to_string()),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                // A newer open (or a navigation away) supersedes this attach.
                if this.view != View::Session(id) {
                    return;
                }
                match attached {
                    Ok(Ok((rx, resize))) => {
                        let reader = ChannelReader::new(rx);
                        let writer = CommandWriter::new(this.app.clone(), this.handle.clone(), id);
                        let config = terminal_config();
                        let view = cx.new(|cx| {
                            // Keep the attach's PTY in step with the rendered grid — the
                            // agent sees real winsize changes (SIGWINCH), not a fixed
                            // 120x40 (issue #15 follow-up). `try_send` from the UI
                            // thread: stale geometry is superseded, never blocks.
                            TerminalView::new(writer, reader, config, cx).with_resize_callback(
                                move |cols, rows| {
                                    let _ = resize.try_send(daedalus_zellij::TerminalSize {
                                        cols: cols.min(u16::MAX as usize) as u16,
                                        rows: rows.min(u16::MAX as usize) as u16,
                                    });
                                },
                            )
                        });
                        this.terminal = Some(view);
                        this.terminal_error = None;
                    }
                    Ok(Err(reason)) => {
                        this.terminal = None;
                        this.terminal_error = Some(SharedString::from(reason));
                    }
                    Err(join) => {
                        this.terminal = None;
                        this.terminal_error =
                            Some(SharedString::from(format!("attach task failed: {join}")));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Tear down the embedded terminal (dropping the view drops its reader/writer bridge,
    /// which ends the forwarding task when the core stream next yields).
    fn close_terminal(&mut self) {
        self.terminal = None;
        self.terminal_error = None;
    }

    /// Dispatch a command, awaiting the result so a rejection is surfaced as a dismissable
    /// error banner (G1) instead of being swallowed by a fire-and-forget log line.
    fn dispatch(&self, command: Command, cx: &mut Context<Self>) {
        let app = self.app.clone();
        let handle = self.handle.clone();
        cx.spawn(async move |this, cx| {
            let outcome = handle
                .spawn(async move { app.execute(command).await })
                .await;
            let message = match outcome {
                Ok(Ok(_)) => None,
                Ok(Err(e)) => Some(e.to_string()),
                Err(e) => Some(format!("command task failed: {e}")),
            };
            if let Some(message) = message {
                tracing::warn!("command failed: {message}");
                let _ = this.update(cx, |this, cx| {
                    this.last_error = Some(SharedString::from(message));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn sample_usage(&self, id: SessionId) {
        let core = self.app.core().clone();
        self.handle.spawn(async move {
            let _ = core.record_resource_usage(id).await;
        });
    }

    fn input_value(&self, e: &Entity<InputState>, cx: &Context<Self>) -> String {
        e.read(cx).value().trim().to_string()
    }
}

impl Focusable for AppRoot {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        let body = h_flex().flex_1().child(self.render_sidebar(cx)).child(
            div()
                .flex_1()
                .h_full()
                .overflow_hidden()
                .track_focus(&self.focus_handle)
                .child(self.render_content(cx)),
        );

        div().id("daedalus-root").size_full().child(
            v_flex()
                .size_full()
                .child(self.render_title_bar(cx))
                .children(self.render_error_banner(cx))
                .child(body)
                .children(sheet_layer)
                .children(dialog_layer)
                .children(notification_layer),
        )
    }
}

impl AppRoot {
    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Cached per refresh tick / push event — never recomputed per frame.
        let counts = self.counts;
        let p = self.palette;
        TitleBar::new()
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .pl_2()
                    .child(
                        div()
                            .font_semibold()
                            .text_color(col(p.skin.accent()))
                            .child("Daedalus"),
                    )
                    .child(count_chip(p, StatusTone::Running, counts.running))
                    // The purple segment counts sessions blocked on the operator:
                    // questions + confirmations (prototype GlobalStatus).
                    .child(count_chip(p, StatusTone::Awaiting, counts.awaiting))
                    .child(count_chip(p, StatusTone::Completed, counts.completed))
                    .child(count_chip(p, StatusTone::Failed, counts.failed)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .justify_end()
                    .items_center()
                    .gap_2()
                    .pr_2()
                    .child(self.render_hosts_pill())
                    .child(
                        Button::new("start-session")
                            .primary()
                            .label("Start session")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.view = View::Start;
                                cx.notify();
                            })),
                    ),
            )
    }

    /// A dismissable error banner shown in the shell chrome when the last dispatched
    /// command failed (G1). Absent when there is no pending error.
    fn render_error_banner(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let message = self.last_error.clone()?;
        let p = self.palette;
        let accent = col(StatusTone::Failed.color(p.skin));
        Some(
            h_flex()
                .w_full()
                .items_center()
                .gap_3()
                .px_4()
                .py_2()
                .bg(accent.opacity(0.12))
                .border_b_1()
                .border_color(accent)
                .child(div().text_color(accent).child("✕"))
                .child(div().flex_1().text_sm().child(message))
                .child(
                    Button::new("dismiss-error")
                        .ghost()
                        .label("Dismiss")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.last_error = None;
                            cx.notify();
                        })),
                ),
        )
    }

    /// The titlebar hosts pill (prototype `HostsIndicator`): one availability dot per
    /// host, worst state first, with the host count. The full rows (kind + name +
    /// availability + reason) live in the sidebar Hosts list.
    fn render_hosts_pill(&self) -> impl IntoElement {
        let p = self.palette;
        h_flex()
            .id("hosts-pill")
            .items_center()
            .gap_1()
            .px_2()
            .py(px(2.0))
            .rounded_full()
            .bg(gpui::rgba(0xffffff10))
            .children(self.hosts.rows.iter().map(|h| avail_dot(p, h.availability)))
            .child(
                div()
                    .text_xs()
                    .child(format!("{}", self.hosts.rows.len().max(1))),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // "Needs you" leads and carries the queue count as its badge (purple in the
        // design; the count travels in the label here). Read from the per-tick cache.
        let needs = self.needs.rows.len();
        let needs_label = if needs > 0 {
            format!("Needs you ({needs})")
        } else {
            "Needs you".to_string()
        };
        let nav = [
            (NavItem::Needs, needs_label, IconName::Inbox),
            (NavItem::Tasks, "Tasks".into(), IconName::LayoutDashboard),
            (NavItem::Fleet, "Sessions".into(), IconName::SquareTerminal),
            (NavItem::Discover, "Discover".into(), IconName::Globe),
            (NavItem::Tools, "Tools".into(), IconName::Bot),
            (NavItem::Backends, "Environments".into(), IconName::Folder),
            (NavItem::Settings, "Settings".into(), IconName::Settings),
        ];
        let active = matches!(self.view, View::Nav(_));
        let items: Vec<SidebarMenuItem> = nav
            .into_iter()
            .map(|(item, label, icon)| {
                SidebarMenuItem::new(label)
                    .icon(icon)
                    .active(active && self.view == View::Nav(item))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.view = View::Nav(item);
                        cx.notify();
                    }))
            })
            .collect();

        // Hosts list (prototype sidebar `side-backends` / HostsIndicator popover rows,
        // anchored at the bottom like the prototype): dot + name, plus the availability
        // text and stated reason when impaired (FR-028).
        let p = self.palette;
        let hosts_rows = v_flex().gap_1().children(self.hosts.rows.iter().map(|h| {
            let mut row = h_flex()
                .items_center()
                .gap_2()
                .child(avail_dot(p, h.availability))
                .child(div().text_sm().child(h.name.clone()));
            if h.availability != Availability::Available {
                row = row.child(
                    div()
                        .text_xs()
                        .opacity(0.6)
                        .child(h.availability_text().to_string()),
                );
            }
            let mut entry = v_flex().gap_0().child(row);
            if let Some(reason) = &h.reason {
                entry = entry.child(div().text_xs().opacity(0.5).child(reason.clone()));
            }
            entry
        }));

        Sidebar::new("nav")
            .w(px(232.0))
            .header(
                SidebarHeader::new().child(
                    div()
                        .font_semibold()
                        .text_color(col(self.palette.skin.accent()))
                        .child("Daedalus"),
                ),
            )
            .child(SidebarGroup::new("Navigate").child(SidebarMenu::new().children(items)))
            .footer(
                v_flex()
                    .gap_1()
                    .p_2()
                    .w_full()
                    .child(div().text_xs().opacity(0.6).child("Hosts"))
                    .child(hosts_rows),
            )
    }

    fn render_content(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.view {
            View::Start => self.render_start(cx).into_any_element(),
            View::Session(id) if self.app.session(id).is_some() => {
                self.render_detail(id, cx).into_any_element()
            }
            View::Session(_) => self.render_sessions(cx).into_any_element(),
            View::Nav(NavItem::Needs) => self.render_needs(cx).into_any_element(),
            View::Nav(NavItem::Tasks) => self.render_tasks(cx).into_any_element(),
            View::Nav(NavItem::Fleet) => self.render_sessions(cx).into_any_element(),
            View::Nav(NavItem::Discover) => self.render_discover(cx).into_any_element(),
            View::Nav(NavItem::Tools) => self.render_tools(cx).into_any_element(),
            View::Nav(NavItem::Backends) => self.render_environments().into_any_element(),
            View::Nav(NavItem::Settings) => self.render_settings(cx).into_any_element(),
        }
    }

    fn render_start(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let tools = self.app.core().list_tools().unwrap_or_default();
        let selected_tool = self.start_tool.or_else(|| tools.first().map(|t| t.id));
        let accent = col(p.skin.accent());

        // Step 1 — Agentic tool (prototype `FormSection` + tool card grid): icon tile,
        // name, capability chip; the selected card carries the accent border.
        let mut tool_grid = h_flex().gap_2().flex_wrap().w_full();
        for t in &tools {
            let tid = t.id;
            let chosen = selected_tool == Some(tid);
            let initial = t.name.chars().next().unwrap_or('?').to_ascii_uppercase();
            let mut tile_bg = accent;
            tile_bg.a = 0.25;
            let interactive = t.capabilities.accepts_interactive_input;
            let cap_chip = if interactive {
                let good = col(p.status_color(StatusTone::Completed));
                let mut tint = good;
                tint.a = 0.18;
                div()
                    .px_2()
                    .py_0p5()
                    .rounded_full()
                    .text_xs()
                    .bg(tint)
                    .text_color(good)
                    .child("accepts input")
            } else {
                div()
                    .px_2()
                    .py_0p5()
                    .rounded_full()
                    .text_xs()
                    .bg(gpui::rgba(0xffffff10))
                    .opacity(0.8)
                    .child("headless")
            };
            let mut card_el = v_flex()
                .id(SharedString::from(format!("tool-{tid}")))
                .gap_2()
                .p_3()
                .rounded_lg()
                .w(px(260.0))
                .bg(gpui::rgba(0xffffff08))
                .border_1()
                .border_color(gpui::rgba(0xffffff14))
                .cursor_pointer()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(28.0))
                                .h(px(28.0))
                                .rounded_md()
                                .bg(tile_bg)
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_semibold()
                                .child(String::from(initial)),
                        )
                        .child(div().font_semibold().child(t.name.clone()))
                        .child(div().flex_1())
                        .children(chosen.then(|| div().text_color(accent).child("✓"))),
                )
                .child(cap_chip)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.start_tool = Some(tid);
                    cx.notify();
                }));
            if chosen {
                card_el = card_el.border_color(accent);
            }
            tool_grid = tool_grid.child(card_el);
        }
        if tools.is_empty() {
            tool_grid = tool_grid.child(empty("No tools registered — add one under Tools."));
        }

        // Step 2 — Objective (SpecKit convention: an objective decomposing into tracked
        // tasks from a tasks.md).
        let objective_body = v_flex()
            .gap_3()
            .child(field(
                "Objective",
                div().child(Input::new(&self.inputs.objective)),
            ))
            .child(field(
                "Tasks file (tasks.md)",
                div().child(Input::new(&self.inputs.tasks_path)),
            ));

        // Step 3 — Environment & backend: fresh vs pre-existing (worktree required for
        // pre-existing, FR-002a), and the hosting backend.
        let origin_row = h_flex()
            .gap_2()
            .child(origin_btn(self, cx, Origin::Fresh, "Fresh"))
            .child(origin_btn(self, cx, Origin::PreExisting, "Pre-existing"));
        let mut kinds: Vec<BackendKind> = self
            .app
            .core()
            .backend_registry()
            .all()
            .iter()
            .map(|b| b.kind())
            .collect();
        kinds.dedup();
        let backend_row = h_flex().gap_2().children(kinds.into_iter().map(|k| {
            let chosen = self.start_backend == k;
            let btn = Button::new(SharedString::from(format!("bk-{k:?}"))).label(format!("{k:?}"));
            let btn = if chosen { btn.primary() } else { btn.outline() };
            btn.on_click(cx.listener(move |this, _, _, cx| {
                this.start_backend = k;
                cx.notify();
            }))
        }));
        let mut env_body = v_flex().gap_3().child(field("Environment", origin_row));
        if self.start_origin == Origin::PreExisting {
            env_body = env_body
                .child(field(
                    "Worktree",
                    div().child(Input::new(&self.inputs.worktree)),
                ))
                .child(
                    div()
                        .text_xs()
                        .opacity(0.6)
                        .child(crate::screens::start::WORKTREE_REASSURANCE),
                );
        }
        env_body = env_body.child(field("Backend", backend_row));

        // Launch footer (prototype `start-foot`): the tested hint copy left, the primary
        // action right.
        let valid = selected_tool.is_some()
            && !self.input_value(&self.inputs.objective, cx).is_empty()
            && !self.input_value(&self.inputs.tasks_path, cx).is_empty();
        let hint =
            crate::screens::start::StartScreen::footer_hint(self.start_attempted, valid, false);
        let hint_color: gpui::Hsla = match hint.tone {
            crate::screens::start::FooterTone::Error => {
                col(p.status_color(StatusTone::Failed)).into()
            }
            crate::screens::start::FooterTone::Warn => {
                col(p.status_color(StatusTone::Stalled)).into()
            }
            crate::screens::start::FooterTone::Neutral => gpui::rgba(0xffffff90).into(),
        };
        let footer = h_flex()
            .items_center()
            .gap_2()
            .pt_2()
            .border_t_1()
            .border_color(gpui::rgba(0xffffff14))
            .child(div().text_sm().text_color(hint_color).child(hint.text))
            .child(div().flex_1())
            .child(
                Button::new("do-start")
                    .primary()
                    .label("Start session →")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.submit_start(cx);
                    })),
            );

        let header = h_flex()
            .items_center()
            .w_full()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xl().font_semibold().child("Start a session"))
                    .child(div().text_sm().opacity(0.6).child(
                        "Launch an agentic tool against an objective inside an environment",
                    )),
            )
            .child(div().flex_1())
            .child(
                Button::new("cancel-start")
                    .ghost()
                    .label("× Cancel")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.view = View::Nav(NavItem::Tasks);
                        cx.notify();
                    })),
            );

        v_flex()
            .gap_4()
            .size_full()
            .p_4()
            .child(header)
            .child(step_card(1, "Agentic tool", tool_grid))
            .child(step_card(2, "Objective", objective_body))
            .child(step_card(3, "Environment & backend", env_body))
            .child(footer)
    }

    fn submit_start(&mut self, cx: &mut Context<Self>) {
        let tools = self.app.core().list_tools().unwrap_or_default();
        let Some(tool_id) = self.start_tool.or_else(|| tools.first().map(|t| t.id)) else {
            // An attempted-but-invalid launch flips the footer hint to its error copy
            // rather than failing silently (prototype `start-foot-hint`).
            self.start_attempted = true;
            cx.notify();
            return;
        };
        let tasks_file = self.input_value(&self.inputs.tasks_path, cx);
        if tasks_file.is_empty() {
            self.start_attempted = true;
            cx.notify();
            return;
        }
        let worktree = if self.start_origin == Origin::PreExisting {
            let path = self.input_value(&self.inputs.worktree, cx);
            if path.is_empty() {
                self.start_attempted = true;
                cx.notify();
                return;
            }
            Some(WorktreeRef {
                path,
                branch: "daedalus".to_string(),
            })
        } else {
            None
        };
        let req = StartSessionRequest {
            tool_id,
            objective: Objective {
                id: ObjectiveId::new(),
                artifact_ref: ArtifactRef {
                    root: std::env::temp_dir().to_string_lossy().into_owned(),
                    tasks_file,
                },
                description: {
                    let d = self.input_value(&self.inputs.objective, cx);
                    if d.is_empty() {
                        "Objective".to_string()
                    } else {
                        d
                    }
                },
            },
            origin: self.start_origin,
            worktree,
            backend: self.start_backend,
            limits: daedalus_proto::ResourceLimits::default(),
        };
        // Await the start (G1): open the NEW SESSION on success (the operator's next act
        // is watching it, not scanning the fleet); on failure surface the stated reason
        // and STAY on the Start form rather than optimistically leaving so a rejected
        // start looks like it worked.
        let app = self.app.clone();
        let handle = self.handle.clone();
        cx.spawn(async move |this, cx| {
            let outcome = handle
                .spawn(async move { app.execute(Command::StartSession(req)).await })
                .await;
            let _ = this.update(cx, |this, cx| {
                match outcome {
                    Ok(Ok(daedalus_app::CommandResult::SessionStarted(id))) => {
                        this.last_error = None;
                        this.view = View::Session(id);
                        this.session_focus = None;
                        this.attach_terminal(id, cx);
                    }
                    Ok(Ok(_)) => {
                        this.last_error = None;
                        this.view = View::Nav(NavItem::Fleet);
                    }
                    Ok(Err(e)) => this.last_error = Some(SharedString::from(e.to_string())),
                    Err(e) => {
                        this.last_error = Some(SharedString::from(format!("start failed: {e}")))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The Needs-you queue (US6): the dedicated screen, driven by the headless
    /// [`NeedsView`] view-model so copy stays identical to the tested design strings —
    /// read from the per-tick cache rather than rebuilt per frame.
    fn render_needs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let view = &self.needs;
        let mut list = v_flex().gap_2().w_full();
        if let Some(caught_up) = view.empty {
            list = list.child(
                card()
                    .child(div().font_semibold().child(caught_up.title))
                    .child(div().text_sm().opacity(0.6).child(caught_up.subtitle)),
            );
        }
        for row in &view.rows {
            let link = row.activate();
            let reason = if row.quoted {
                format!("“{}”", row.reason)
            } else {
                row.reason.clone()
            };
            let mut right = h_flex()
                .gap_3()
                .items_center()
                .child(div().text_xs().opacity(0.6).child(row.wait_label.clone()));
            if let Some(cost) = &row.cost {
                right = right.child(div().text_xs().opacity(0.6).child(cost.label()));
            }
            right = right.child(
                Button::new(SharedString::from(format!("needs-{}", row.session)))
                    .primary()
                    .label(row.action.label.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_session_focused(link.session, link.focus, window, cx);
                    })),
            );
            list = list.child(
                card().child(
                    h_flex()
                        .items_center()
                        .gap_3()
                        .child(status_pill(p, row.tone))
                        .child(
                            v_flex()
                                .flex_1()
                                .gap_0()
                                .child(
                                    div()
                                        .font_semibold()
                                        .text_sm()
                                        .child(format!("{} · {}", row.cue, row.tool)),
                                )
                                .child(div().text_sm().child(row.objective.clone()))
                                .child(div().text_xs().opacity(0.6).child(reason)),
                        )
                        .child(right),
                ),
            );
        }
        let mut scaffold = screen(view.title, view.subtitle);
        if !view.stats.is_empty() {
            scaffold = scaffold.child(div().text_sm().opacity(0.75).child(view.stats_line()));
        }
        scaffold.child(list)
    }

    fn render_sessions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let fleet = self.app.fleet();
        let mut list = v_flex().gap_2().w_full();
        if fleet.is_empty() {
            list = list.child(empty("No sessions yet — click \"Start session\"."));
        }
        for s in fleet {
            let tone = StatusTone::from_session(s.status);
            let id = s.id;
            let board = self.app.task_board(id);
            let done = board
                .iter()
                .filter(|x| x.status == TaskStatus::Done)
                .count();

            let mut row = h_flex()
                .items_center()
                .gap_3()
                .child(status_pill(p, tone))
                .child(
                    v_flex()
                        .flex_1()
                        .gap_0()
                        .child(div().font_semibold().child(s.tool_name.clone()))
                        .child(div().text_xs().opacity(0.6).child(format!(
                            "{} · tasks {}/{}",
                            s.objective,
                            done,
                            board.len()
                        ))),
                );

            if s.status == SessionStatus::AwaitingConfirmation {
                row = row.child(
                    Button::new(SharedString::from(format!("confirm-{id}")))
                        .primary()
                        .label("Confirm")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dispatch(Command::ConfirmCompletion(id), cx);
                            cx.notify();
                        })),
                );
            }
            // Stop only while the run is on (matches the tested SessionView control
            // rule: an awaiting-confirmation run has already exited).
            if !s.status.has_ended() {
                row = row.child(
                    Button::new(SharedString::from(format!("stop-{id}")))
                        .danger()
                        .label("Stop")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dispatch(Command::StopSession(id), cx);
                            cx.notify();
                        })),
                );
            }
            row = row.child(
                Button::new(SharedString::from(format!("clean-{id}")))
                    .ghost()
                    .label("Clean up")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dispatch(Command::CleanUp(id), cx);
                        cx.notify();
                    })),
            );
            row = row.child(
                Button::new(SharedString::from(format!("open-{id}")))
                    .outline()
                    .label("Open")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_session(id, window, cx);
                    })),
            );

            list = list.child(card().child(row));
        }
        screen("Sessions", "Every session across backends").child(list)
    }

    fn render_detail(&self, id: SessionId, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        // The whole screen renders from the tested headless [`SessionView`] — banner
        // copy, control gating (disabled-with-reason), board rows, rail — so the native
        // surface can't drift from the design strings. A pending deep-link focus lands
        // as answer mode via `with_focus` (T078).
        let Some(vm) = SessionView::build(&self.app, id, &self.palette)
            .map(|vm| vm.with_focus(self.session_focus))
        else {
            return screen("Session", "not found");
        };

        // Header row (prototype `sess-head`): back · tool · status pill · [controls].
        let mut controls = h_flex().gap_2().items_center();
        for c in &vm.controls {
            controls = controls.child(self.control_button(id, c, cx));
        }
        let header = h_flex()
            .items_center()
            .gap_3()
            .child(
                Button::new("back")
                    .ghost()
                    .label("← Back")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_terminal();
                        this.view = View::Nav(NavItem::Fleet);
                        cx.notify();
                    })),
            )
            .child(div().font_semibold().child(vm.tool.clone()))
            .child(badge_pill(&vm.badge))
            .child(div().flex_1())
            .child(controls);

        // Session title + meta chips (prototype `sess-title` / `sess-chips`): the
        // objective as the headline; spec ref, backend, isolation posture as chips —
        // `worktree-isolated` carries the positive tint (FR-002a stated, not implied).
        let title = div().text_xl().font_semibold().child(vm.objective.clone());
        let mut chips_row = h_flex().gap_2().items_center().flex_wrap();
        for chip in &vm.chips {
            let mut el = div()
                .px_2()
                .py_0p5()
                .rounded_full()
                .text_xs()
                .bg(gpui::rgba(0xffffff10))
                .child(chip.label.clone());
            if chip.positive {
                let good = col(p.status_color(StatusTone::Completed));
                let mut tint = good;
                tint.a = 0.18;
                el = el.bg(tint).text_color(good);
            }
            chips_row = chips_row.child(el);
        }

        // The per-status state banner (prototype `StateBanner`), copy verbatim from the
        // view-model; its actions are carried by the header controls below.
        let banner = vm.banner.as_ref().map(|b| {
            let mut tint = col(banner_color(p, b.tone));
            tint.a = 0.15;
            let mut el = v_flex()
                .gap_1()
                .p_2()
                .rounded_md()
                .bg(tint)
                .child(div().font_semibold().text_sm().child(b.lead.clone()));
            if let Some(body) = &b.body {
                el = el.child(div().text_sm().opacity(0.8).child(body.clone()));
            }
            if let Some(quote) = &b.quote {
                el = el.child(div().text_sm().opacity(0.8).child(format!("“{quote}”")));
            }
            if let Some(meta) = &b.meta {
                el = el.child(div().text_xs().opacity(0.6).child(meta.clone()));
            }
            el
        });

        // Send-input row: live when the view-model raises no notice; otherwise disabled
        // with the tested review-mode/no-input explanation shown alongside.
        let mut send_row = h_flex()
            .gap_2()
            .items_center()
            .child(div().flex_1().child(Input::new(&self.inputs.send)));
        send_row = if vm.input_notice.is_none() {
            send_row.child(
                Button::new("send")
                    .label("Send input")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let text = this.input_value(&this.inputs.send, cx);
                        if !text.is_empty() {
                            this.dispatch(
                                Command::SendInput {
                                    session: id,
                                    data: format!("{text}\n").into_bytes().into(),
                                },
                                cx,
                            );
                            this.inputs
                                .send
                                .update(cx, |st, cx| st.set_value("", window, cx));
                            cx.notify();
                        }
                    })),
            )
        } else {
            send_row.child(Button::new("send").label("Send input").disabled(true))
        };
        let input_notice = vm
            .input_notice
            .clone()
            .map(|n| div().text_xs().opacity(0.6).child(n));

        // Tasks panel (prototype `TaskBoard`): "Tasks — from SpecKit tasks.md", done/total
        // progress, and the four status columns with per-card id + status dot + title;
        // in-progress cards carry the accent border.
        let (done, total) = vm.progress;
        let progress_bar = div()
            .h(px(5.0))
            .w_full()
            .rounded_full()
            .bg(gpui::rgba(0xffffff14))
            .child(
                div()
                    .h_full()
                    .rounded_full()
                    .bg(col(p.skin.accent()))
                    .w(gpui::relative(if total == 0 {
                        0.0
                    } else {
                        done as f32 / total as f32
                    })),
            );
        let mut columns = h_flex().gap_2().items_start().w_full();
        for column in vm.board_columns() {
            let tone = StatusTone::from_task(column.status);
            let mut col_el = v_flex().gap_1().flex_1().min_w_0().child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(status_dot(p, tone))
                    .child(div().text_xs().font_semibold().child(column.title))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.5)
                            .child(column.cards.len().to_string()),
                    ),
            );
            for t in &column.cards {
                let mut card_el = v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(gpui::rgba(0xffffff08))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .opacity(0.6)
                                    .font_family(mono_family())
                                    .child(t.id.clone()),
                            )
                            .child(div().flex_1())
                            .child(status_dot(p, t.badge.tone)),
                    )
                    .child(div().text_sm().child(t.description.clone()));
                if column.status == daedalus_proto::TaskStatus::InProgress {
                    card_el = card_el.border_1().border_color(col(p.skin.accent()));
                }
                col_el = col_el.child(card_el);
            }
            columns = columns.child(col_el);
        }
        let mut board_el = v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().font_semibold().child("Tasks"))
                    .child(div().text_xs().opacity(0.5).child("from SpecKit tasks.md"))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.8)
                            .child(format!("{done}/{total}")),
                    ),
            )
            .child(progress_bar);
        board_el = if vm.board.is_empty() {
            board_el.child(empty("No tracked tasks."))
        } else {
            board_el.child(columns)
        };

        // The telemetry rail (FR-019/019a) + trimmed-output notice (FR-016a).
        let rail = self.render_rail(&vm.rail);
        let trim_notice = vm.trim_notice.clone().map(|n| {
            div()
                .text_xs()
                .opacity(0.75)
                .p_1()
                .rounded_sm()
                .bg(gpui::rgba(0xffffff10))
                .child(n)
        });

        // Terminal panel (prototype `term-bar` + body): header states liveness — a green
        // LIVE pill while the run is on, "persisted output" for ended sessions,
        // "last-known output" when unreachable — then the live embedded terminal
        // (REDACTED core stream), a stated not-attachable reason (C-T2 — never a silent
        // blank), or the captured-output tail. The send-input row sits under the pane.
        let live = !vm.status.is_terminal() && vm.status != SessionStatus::Unknown;
        let liveness = if live {
            let good = col(p.status_color(StatusTone::Running));
            let mut tint = good;
            tint.a = 0.18;
            div()
                .px_2()
                .py_0p5()
                .rounded_full()
                .text_xs()
                .bg(tint)
                .text_color(good)
                .child("● LIVE")
        } else {
            div()
                .px_2()
                .py_0p5()
                .rounded_full()
                .text_xs()
                .bg(gpui::rgba(0xffffff10))
                .opacity(0.8)
                .child(if vm.status == SessionStatus::Unknown {
                    "last-known output"
                } else {
                    "persisted output"
                })
        };
        let term_bar = h_flex()
            .items_center()
            .gap_2()
            .child(div().font_semibold().text_sm().child("Terminal"))
            .child(liveness);
        let term_body = if let Some(term) = &self.terminal {
            div()
                .w_full()
                .h(px(380.0))
                .rounded_md()
                .overflow_hidden()
                .bg(gpui::rgba(0x000000a0))
                .child(term.clone())
                .into_any_element()
        } else if let Some(reason) = &self.terminal_error {
            div()
                .w_full()
                .p_3()
                .rounded_md()
                .bg(gpui::rgba(0x00000040))
                .text_sm()
                .child(format!("Not attachable: {reason}"))
                .into_any_element()
        } else {
            let output = self.app.core().session_output(id, 8192);
            div()
                .w_full()
                .p_2()
                .rounded_md()
                .bg(gpui::rgba(0x00000040))
                .font_family(mono_family())
                .text_xs()
                .child(if output.is_empty() {
                    "— no captured output —".to_string()
                } else {
                    output
                })
                .into_any_element()
        };
        // With a live terminal attached, keystrokes go straight into the pane (the
        // CommandWriter routes them through SendInput) — the separate field would be
        // redundant, so it only renders when there is no terminal to type into.
        let mut terminal_pane = v_flex()
            .gap_1()
            .child(term_bar)
            .children(trim_notice)
            .child(term_body);
        if self.terminal.is_none() {
            terminal_pane = terminal_pane.child(send_row);
        }
        terminal_pane = terminal_pane.children(input_notice);

        // Two-column main area (prototype `sess-grid`): terminal left, Tasks + rail right.
        let main = h_flex()
            .gap_4()
            .items_start()
            .w_full()
            .child(v_flex().gap_2().flex_1().min_w_0().child(terminal_pane))
            .child(
                v_flex()
                    .gap_4()
                    .w(px(380.0))
                    .flex_none()
                    .child(board_el)
                    .child(rail),
            );

        card()
            .gap_4()
            .child(header)
            .child(title)
            .child(chips_row)
            .children(banner)
            .child(main)
    }

    /// Render one view-model [`ActionButton`] control, wiring the tested labels back to
    /// the same dispatched commands; a disabled control carries its stated reason as a
    /// tooltip (never silently inert).
    fn control_button(
        &self,
        session: SessionId,
        control: &ActionButton,
        cx: &mut Context<Self>,
    ) -> Button {
        let mut btn = Button::new(SharedString::from(format!("ctl-{}", control.label)))
            .label(control.label.clone());
        btn = match control.intent {
            ButtonIntent::Primary => btn.primary(),
            ButtonIntent::Danger => btn.danger(),
            ButtonIntent::Ghost => btn.ghost(),
            ButtonIntent::Tinted => btn.outline(),
        };
        if !control.enabled {
            btn = btn.disabled(true);
            if let Some(reason) = &control.disabled_reason {
                btn = btn.tooltip(reason.clone());
            }
            return btn;
        }
        match control.label.as_str() {
            "Confirm completion" => btn.on_click(cx.listener(move |this, _, _, cx| {
                this.dispatch(Command::ConfirmCompletion(session), cx);
                cx.notify();
            })),
            "Stop" => btn.on_click(cx.listener(move |this, _, _, cx| {
                this.dispatch(Command::StopSession(session), cx);
                cx.notify();
            })),
            "Clean up" => btn.on_click(cx.listener(move |this, _, _, cx| {
                this.dispatch(Command::CleanUp(session), cx);
                this.close_terminal();
                this.view = View::Nav(NavItem::Fleet);
                cx.notify();
            })),
            "Start similar" => btn.on_click(cx.listener(|this, _, _, cx| {
                this.view = View::Start;
                cx.notify();
            })),
            // The header Send-input control focuses the send row's input.
            "Send input" => btn.on_click(cx.listener(|this, _, window, cx| {
                this.inputs.send.update(cx, |s, cx| s.focus(window, cx));
            })),
            _ => btn,
        }
    }

    /// The telemetry rail sections (Resources / Timeline / Outcome) from the view-model.
    fn render_rail(&self, rail: &crate::screens::session::TelemetryRail) -> gpui::Div {
        let p = self.palette;
        let mut el = v_flex().gap_2().w_full();

        // Resource meters are deliberately NOT drawn (recorded deviation, 2026-07-08):
        // until real FR-019 metrics land (#9) the numbers are placeholders and the
        // operator called them noise. The unknown-state notice and runtime still show —
        // those carry real signal.
        if let Some(notice) = rail.unavailable_notice {
            el = el.child(div().text_xs().opacity(0.7).child(notice));
        }
        if let Some(res) = &rail.resources {
            el = el.child(
                div()
                    .text_xs()
                    .opacity(0.6)
                    .child(format!("{} {}", res.runtime_label, res.runtime)),
            );
        }

        // Timeline (FR-019a): lifecycle + operator actions + notes, oldest → newest.
        el = el.child(section_label("Timeline"));
        for entry in &rail.timeline {
            let tone = match entry.kind {
                TimelineKind::Status(status) => Some(StatusTone::from_session(status)),
                TimelineKind::Action(_) | TimelineKind::Note => None,
            };
            let node = match tone {
                Some(tone) => status_dot(p, tone).into_any_element(),
                None => div()
                    .w(px(9.0))
                    .h(px(9.0))
                    .rounded_full()
                    .bg(col(p.skin.accent()))
                    .into_any_element(),
            };
            // When it happened, relative ("8m ago") — absolute times mean re-deriving
            // "how long ago" in your head; the wait durations are what the operator acts on.
            let age_secs = ((daedalus_core::clock::now().millis() - entry.timestamp.millis()).max(0)
                as u64)
                / 1000;
            let when = format!(
                "{} ago",
                crate::screens::needs::wait_label(std::time::Duration::from_secs(age_secs))
            );
            el = el.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(node)
                    .child(div().text_xs().child(entry.label.clone()))
                    .child(div().flex_1())
                    .child(div().text_xs().opacity(0.45).child(when)),
            );
        }

        // Outcome block for ended/unknown sessions.
        if let Some(outcome) = &rail.outcome {
            let mut block = v_flex().gap_1().child(section_label("Outcome")).child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_xs().opacity(0.6).w(px(64.0)).child("State"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(col(StatusTone::from_session(outcome.state).color(p.skin)))
                            .child(outcome.state_label),
                    ),
            );
            if let Some(exit) = &outcome.exit {
                block = block.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().text_xs().opacity(0.6).w(px(64.0)).child("Exit"))
                        .child(
                            div()
                                .font_family(mono_family())
                                .text_xs()
                                .child(exit.clone()),
                        ),
                );
            }
            if let Some(reason) = &outcome.reason {
                block = block.child(div().text_xs().opacity(0.8).child(reason.clone()));
            }
            block = block.child(div().text_xs().opacity(0.5).child(outcome.note));
            el = el.child(block);
        }

        card().child(el)
    }

    /// The aggregate tasks board (FR-025a) — the landing view, driven by the headless
    /// [`TasksBoardView`] so grouping/counts/copy stay identical to the tested design.
    fn render_tasks(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        // The boards stay live via the 500ms refresh task, which re-reads tasks.md for
        // sessions whose run is still on — never on the render path.
        let view = TasksBoardView::build(&self.app, &TaskFilters::default());

        let mut scaffold = screen(view.title, &view.stats_line());

        // The pinned Needs-you strip (Phase 9) rides on top of the board.
        if let Some(strip) = &view.needs_strip {
            scaffold = scaffold.child(
                card()
                    .child(div().font_semibold().child(strip.headline.clone()))
                    .child(div().text_xs().opacity(0.6).child(strip.summary.clone())),
            );
        }

        let mut list = v_flex().gap_4().w_full();
        if let Some(state) = view.empty {
            list = list.child(
                card()
                    .child(div().font_semibold().child(state.title))
                    .child(div().text_sm().opacity(0.6).child(state.body)),
            );
        }
        for group in &view.groups {
            let header = h_flex()
                .items_center()
                .gap_2()
                .child(status_dot(p, StatusTone::from_task(group.status)))
                .child(div().font_semibold().child(group.label))
                .child(
                    div()
                        .text_xs()
                        .opacity(0.6)
                        .child(format!("{}", group.count)),
                );
            let mut rows = v_flex().gap_1().w_full().child(header);
            for row in &group.rows {
                let link = row.open();
                let mut line = h_flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_xs()
                            .opacity(0.7)
                            .child(row.task_id.clone()),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_0()
                            .child(div().font_semibold().text_sm().child(row.title.clone()))
                            .child(div().text_xs().opacity(0.6).child(row.sub_line.clone())),
                    );
                if let Some(kind) = row.needs {
                    line = line.child(
                        div()
                            .text_xs()
                            .text_color(col(crate::screens::needs::kind_tone(kind).color(p.skin)))
                            .child(crate::screens::needs::kind_tag(kind)),
                    );
                }
                line = line.child(
                    // The session-ref chip: dot + objective, opens the session.
                    Button::new(SharedString::from(format!(
                        "task-{}-{}",
                        row.session, row.task_id
                    )))
                    .outline()
                    .label(row.session_ref.objective.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_session(link.session, window, cx);
                    })),
                );
                rows = rows.child(card().child(line));
            }
            list = list.child(rows);
        }

        scaffold.child(list)
    }

    fn render_discover(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let mut list = v_flex().gap_2().w_full();
        if self.discovered.is_empty() {
            list = list.child(empty("No sessions discovered on local / mDNS sources."));
        }
        for d in &self.discovered {
            let tone = if d.source_availability == Availability::Unavailable {
                StatusTone::Unreachable
            } else {
                StatusTone::Running
            };
            let mut row = h_flex()
                .items_center()
                .gap_3()
                .child(status_pill(p, tone))
                .child(
                    v_flex()
                        .flex_1()
                        .gap_0()
                        .child(div().font_semibold().child(d.host_label.clone()))
                        .child(
                            div()
                                .text_xs()
                                .opacity(0.6)
                                .child(format!("{:?} · {}", d.kind, d.zellij_session)),
                        ),
                );
            if d.attachable {
                let did = d.id.clone();
                row = row.child(
                    Button::new(SharedString::from(format!("conn-{}", d.id.identity.0)))
                        .primary()
                        .label("Connect")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dispatch(Command::ConnectDiscovered(did.clone()), cx);
                            cx.notify();
                        })),
                );
            } else {
                row = row.child(
                    div()
                        .text_xs()
                        .opacity(0.6)
                        .child(d.attach_reason.clone().unwrap_or_default()),
                );
            }
            list = list.child(card().child(row));
        }
        screen(
            "Discover",
            "Sessions across local, mDNS, and tunneled hosts",
        )
        .child(list)
    }

    fn render_tools(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Registration form.
        let accepts = self.tool_accepts_input;
        let form = card()
            .gap_2()
            .child(section_label("Register a tool"))
            .child(
                v_flex()
                    .gap_2()
                    .child(field(
                        "Name",
                        div().child(Input::new(&self.inputs.tool_name)),
                    ))
                    .child(field(
                        "Program",
                        div().child(Input::new(&self.inputs.tool_program)),
                    ))
                    .child(field(
                        "Args",
                        div().child(Input::new(&self.inputs.tool_args)),
                    ))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child({
                                let b = Button::new("accepts").label("Accepts input");
                                let b = if accepts { b.primary() } else { b.outline() };
                                b.on_click(cx.listener(|this, _, _, cx| {
                                    this.tool_accepts_input = !this.tool_accepts_input;
                                    cx.notify();
                                }))
                            })
                            .child(
                                Button::new("do-register")
                                    .primary()
                                    .label("Register")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.submit_tool(window, cx);
                                    })),
                            ),
                    ),
            );

        let mut list = v_flex()
            .gap_2()
            .w_full()
            .child(section_label("Registered tools"));
        for t in self.app.core().list_tools().unwrap_or_default() {
            let mut cmd = t.invocation.program.clone();
            for a in &t.invocation.args {
                cmd.push(' ');
                cmd.push_str(a);
            }
            list = list.child(
                card().child(
                    v_flex()
                        .gap_0()
                        .child(div().font_semibold().child(t.name))
                        .child(
                            div()
                                .text_xs()
                                .opacity(0.6)
                                .font_family(mono_family())
                                .child(cmd),
                        )
                        .child(div().text_xs().opacity(0.6).child(
                            if t.capabilities.accepts_interactive_input {
                                "accepts input"
                            } else {
                                "batch (no input)"
                            },
                        )),
                ),
            );
        }

        screen("Tools", "Registered agentic tools")
            .child(form)
            .child(list)
    }

    fn submit_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.input_value(&self.inputs.tool_name, cx);
        let program = self.input_value(&self.inputs.tool_program, cx);
        if name.is_empty() || program.is_empty() {
            return;
        }
        let args = self
            .input_value(&self.inputs.tool_args, cx)
            .split_whitespace()
            .map(str::to_string)
            .collect();
        self.dispatch(
            Command::RegisterTool(ToolDef {
                name,
                invocation: InvocationSpec {
                    program,
                    args,
                    env: Vec::new(),
                },
                capabilities: Capabilities {
                    accepts_interactive_input: self.tool_accepts_input,
                    prompt_convention: None,
                },
            }),
            cx,
        );
        for e in [
            &self.inputs.tool_name,
            &self.inputs.tool_program,
            &self.inputs.tool_args,
        ] {
            e.update(cx, |s, cx| s.set_value("", window, cx));
        }
        cx.notify();
    }

    fn render_environments(&self) -> impl IntoElement {
        let p = self.palette;
        let view = &self.backends_view;
        let mut list = v_flex().gap_2().w_full();
        // The FR-028 continuity banner when a host is down (prototype `host-reassure`).
        if let Some(reassure) = view.reassurance {
            list = list.child(card().child(div().text_sm().child(reassure)));
        }
        for row in &view.rows {
            let mut line = h_flex()
                .items_center()
                .gap_3()
                .child(avail_dot(p, row.availability))
                .child(div().font_semibold().child(row.chip.label))
                .child(
                    div()
                        .text_xs()
                        .opacity(0.6)
                        .child(availability_text(row.availability).to_string()),
                )
                .child(div().flex_1());
            if let Some(action) = row.action {
                // Reconnect re-checks availability on the next refresh tick.
                line = line.child(
                    Button::new(SharedString::from(format!("reconnect-{:?}", row.kind)))
                        .outline()
                        .label(action),
                );
            }
            let mut entry = v_flex().gap_1().child(line);
            // The stated degraded/unavailable reason (prototype `brow-note`, FR-028).
            if let Some(reason) = &row.reason {
                entry = entry.child(div().text_xs().opacity(0.6).child(reason.clone()));
            }
            list = list.child(card().child(entry));
        }
        screen("Environments", "Sandbox backends and availability").child(list)
    }

    fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let idle_backend = crate::default_backend_kind();
        let form = card().gap_2().child(section_label("Configuration")).child(
            v_flex()
                .gap_2()
                .child(field(
                    "Concurrency limit",
                    div().child(Input::new(&self.inputs.concurrency)),
                ))
                .child(field(
                    "Stall interval (s)",
                    div().child(Input::new(&self.inputs.stall)),
                ))
                .child(field(
                    // FR-021b: the optional per-backend idle rate feeding the Needs-you
                    // waiting-cost estimate; blank clears it.
                    &format!("Idle rate for {idle_backend:?} ($/hr, blank = none)"),
                    div().child(Input::new(&self.inputs.idle_rate)),
                ))
                .child(field(
                    // The image OpenShell sandboxes are created from (`--from`) —
                    // operator configuration owned by the app, persisted in the store;
                    // blank restores the default base image.
                    "OpenShell sandbox image (blank = default base image)",
                    div().child(Input::new(&self.inputs.openshell_image)),
                ))
                .child(
                    Button::new("save-settings")
                        .primary()
                        .label("Save")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            // Concurrency: validate first (G2) — reject a non-numeric or
                            // out-of-range value rather than silently disabling the limit.
                            let raw = this.input_value(&this.inputs.concurrency, cx);
                            let limit = match crate::screens::settings::parse_concurrency(&raw) {
                                Ok(limit) => limit,
                                Err(msg) => {
                                    this.last_error = Some(SharedString::from(msg));
                                    cx.notify();
                                    return; // leave the existing limit untouched
                                }
                            };
                            this.last_error = None;
                            // Persist via the command path (G6/G15) so both survive restart.
                            this.dispatch(Command::SetConcurrencyLimit(limit), cx);
                            if let Ok(secs) =
                                this.input_value(&this.inputs.stall, cx).parse::<u64>()
                            {
                                this.dispatch(Command::SetStallInterval(secs), cx);
                            }
                            // Idle rate: persisted via the command path (FR-021b).
                            let rate = this.input_value(&this.inputs.idle_rate, cx);
                            let rate = if rate.is_empty() {
                                Some(None)
                            } else {
                                rate.parse::<f64>().ok().map(Some)
                            };
                            if let Some(rate) = rate {
                                this.dispatch(
                                    Command::SetIdleRate {
                                        backend: idle_backend,
                                        rate,
                                    },
                                    cx,
                                );
                            }
                            // OpenShell sandbox image: persisted in the app, no env
                            // vars required; blank clears back to the default image.
                            let image = this.input_value(&this.inputs.openshell_image, cx);
                            this.dispatch(
                                Command::SetBackendImage {
                                    backend: daedalus_proto::BackendKind::OpenShell,
                                    image: (!image.is_empty()).then_some(image),
                                },
                                cx,
                            );
                            cx.notify();
                        })),
                ),
        );

        // Appearance: the three-way theme segment (System | Light | Dark) with System
        // following the OS appearance — the default (FR-009a, T084).
        let mut segment = h_flex().gap_1();
        for pref in ThemePreference::all() {
            let chosen = self.theme_pref == pref;
            let b = Button::new(SharedString::from(format!("theme-{}", pref.label())))
                .label(pref.label());
            let b = if chosen { b.primary() } else { b.outline() };
            segment = segment.child(b.on_click(cx.listener(move |this, _, window, cx| {
                this.theme_pref = pref;
                this.os_appearance = os_appearance(window);
                this.apply_theme(cx);
            })));
        }
        let mut appearance = card()
            .gap_2()
            .child(section_label("Appearance"))
            .child(field("Theme", segment));
        let settings_view = SettingsView {
            theme: self.theme_pref,
            ..SettingsView::default()
        };
        if let Some(note) = settings_view.theme_note() {
            appearance = appearance.child(div().text_xs().opacity(0.6).child(note));
        }

        screen("Settings", "Local-first: no open listener by default")
            .child(form)
            .child(appearance)
            .child(
                card().child(
                    div()
                        .text_xs()
                        .opacity(0.6)
                        .child("Remote reach only over operator-established tunnels."),
                ),
            )
    }
}

/// The embedded terminal view configuration (fixed cell grid + present monospace font).
fn terminal_config() -> TerminalConfig {
    TerminalConfig {
        cols: 100,
        rows: 30,
        font_family: mono_family().to_string(),
        font_size: px(13.0),
        line_height_multiplier: 1.0,
        scrollback: 5000,
        padding: Edges::all(px(6.0)),
        colors: ColorPalette::default(),
    }
}

/// A blocking [`Read`] bridge from the core's async redacted-output channel onto the
/// terminal view's reader thread. The `gpui-terminal` reader runs in a dedicated `std`
/// thread and blocks on `read`, so a blocking `std::sync::mpsc::Receiver` is the correct
/// primitive: a tokio task drains the `App::attach` output and forwards each redacted chunk
/// here. Never carries a raw PTY — only the already-redacted stream from the core.
struct ChannelReader {
    rx: std::sync::mpsc::Receiver<Bytes>,
    buf: Bytes,
    pos: usize,
}

impl ChannelReader {
    fn new(rx: std::sync::mpsc::Receiver<Bytes>) -> Self {
        Self {
            rx,
            buf: Bytes::new(),
            pos: 0,
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        // Block until the current buffer has bytes, refilling from the channel. An empty
        // chunk is skipped (never treated as EOF); a closed channel is EOF (Ok(0)).
        while self.pos >= self.buf.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.buf = chunk;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = (self.buf.len() - self.pos).min(out.len());
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// A [`Write`] bridge that routes operator keystrokes from the terminal view back through
/// the surface command path as [`Command::SendInput`] (FR-023) — keeping the surface
/// boundary: the surface issues a command; the core decides delivery / acceptance.
struct CommandWriter {
    app: App,
    handle: Handle,
    session: SessionId,
}

impl CommandWriter {
    fn new(app: App, handle: Handle, session: SessionId) -> Self {
        Self {
            app,
            handle,
            session,
        }
    }
}

impl Write for CommandWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let bytes = Bytes::copy_from_slice(data);
        let app = self.app.clone();
        let session = self.session;
        self.handle.spawn(async move {
            let _ = app
                .execute(Command::SendInput {
                    session,
                    data: bytes,
                })
                .await;
        });
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A labelled form field: label above, control below.
fn field(label: &str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(div().text_xs().opacity(0.6).child(label.to_string()))
        .child(control)
}

/// An origin toggle button for the Start form.
fn origin_btn(
    root: &AppRoot,
    cx: &mut Context<AppRoot>,
    origin: Origin,
    label: &str,
) -> impl IntoElement {
    let chosen = root.start_origin == origin;
    let b = Button::new(SharedString::from(format!("origin-{label}"))).label(label.to_string());
    let b = if chosen { b.primary() } else { b.outline() };
    b.on_click(cx.listener(move |this, _, _, cx| {
        this.start_origin = origin;
        cx.notify();
    }))
}

/// A screen scaffold: heading + subtitle + content area.
fn screen(title: &str, subtitle: &str) -> gpui::Div {
    v_flex().size_full().gap_4().p_5().child(
        v_flex()
            .gap_0()
            .child(div().text_xl().font_semibold().child(title.to_string()))
            .child(div().text_sm().opacity(0.6).child(subtitle.to_string())),
    )
}

/// A small section heading.
/// A numbered step section (prototype `FormSection`): circled step number + heading,
/// body beneath.
fn step_card(step: usize, title: &'static str, body: impl IntoElement) -> gpui::Div {
    card()
        .gap_3()
        .child(
            h_flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(22.0))
                        .h(px(22.0))
                        .rounded_full()
                        .bg(gpui::rgba(0xffffff14))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .font_semibold()
                        .child(step.to_string()),
                )
                .child(div().font_semibold().child(title)),
        )
        .child(body)
}

fn section_label(text: &str) -> impl IntoElement {
    div().font_semibold().text_sm().child(text.to_string())
}

/// A card surface.
fn card() -> gpui::Div {
    v_flex()
        .gap_1()
        .w_full()
        .p_3()
        .rounded_lg()
        .bg(gpui::rgba(0xffffff08))
}

/// Empty-state text.
fn empty(text: &str) -> impl IntoElement {
    div().text_sm().opacity(0.6).child(text.to_string())
}

/// A host/backend availability dot (prototype `AvailDot`): running hue when available,
/// stalled when degraded, failed when unavailable — always paired with a text label.
fn avail_dot(p: Palette, availability: Availability) -> impl IntoElement {
    let tone = match availability {
        Availability::Available => StatusTone::Running,
        Availability::Degraded => StatusTone::Stalled,
        Availability::Unavailable => StatusTone::Failed,
    };
    status_dot(p, tone)
}

/// A small colored dot for a status tone.
fn status_dot(p: Palette, tone: StatusTone) -> impl IntoElement {
    div()
        .w(px(9.0))
        .h(px(9.0))
        .rounded_full()
        .bg(col(tone.color(p.skin)))
}

/// A filled status pill from a resolved view-model [`StatusBadge`] — carries the
/// status-specific label (e.g. "Waiting for input" vs "Awaiting confirmation" on the
/// shared purple tone), never color alone.
fn badge_pill(badge: &StatusBadge) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(2.0))
        .rounded_full()
        .bg(col(badge.color))
        .text_color(col(badge.foreground))
        .text_xs()
        .child(badge.glyph)
        .child(badge.label)
}

/// The status hue behind a session state banner (prototype `banner-*` tint classes).
fn banner_color(p: Palette, tone: BannerTone) -> Color {
    let status_tone = match tone {
        BannerTone::Info => StatusTone::Starting,
        BannerTone::Ok => StatusTone::Completed,
        BannerTone::Warn => StatusTone::Stalled,
        BannerTone::Error => StatusTone::Failed,
        BannerTone::Neutral => StatusTone::Stopped,
        BannerTone::Await | BannerTone::Confirm => StatusTone::Awaiting,
        BannerTone::Unknown => StatusTone::Unknown,
    };
    status_tone.color(p.skin)
}

/// A filled status pill: color + glyph + label (never color alone).
fn status_pill(p: Palette, tone: StatusTone) -> impl IntoElement {
    let color = tone.color(p.skin);
    h_flex()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(2.0))
        .rounded_full()
        .bg(col(color))
        .text_color(col(color.best_foreground()))
        .text_xs()
        .child(tone.glyph())
        .child(tone.label())
}

/// A "dot Label N" count chip for the title bar.
fn count_chip(p: Palette, tone: StatusTone, n: usize) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_1()
        .text_xs()
        .child(status_dot(p, tone))
        .child(div().child(format!("{} {}", tone.label(), n)))
}
