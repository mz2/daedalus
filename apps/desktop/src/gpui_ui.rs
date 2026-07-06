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
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use tokio::runtime::Handle;

use daedalus_app::{App, AppQuery, AppQueryAsync, Command};
use daedalus_proto::{
    ArtifactRef, Availability, BackendKind, Capabilities, DiscoveredSession, InvocationSpec,
    Objective, ObjectiveId, Origin, SessionId, SessionStatus, StartSessionRequest, TaskStatus,
    ToolDef, ToolId, WorktreeRef,
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
            prompt_convention: None,
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
    // The embedded terminal for the open session (a real PTY via a shell); the PTY handles
    // are held so the pty/child stay alive while the terminal is shown.
    terminal: Option<Entity<TerminalView>>,
    pty: Option<PtyPair>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    // Deep-link focus for the open session (answer mode, T078); cleared on plain opens.
    session_focus: Option<SessionFocus>,
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
            pty: None,
            child: None,
            session_focus: None,
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

    /// Open a session's detail and start an embedded terminal (a shell in a real PTY).
    fn open_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        self.open_session_focused(id, None, cx);
    }

    /// Open a session's detail via a deep link, landing with answer-mode focus (T078).
    fn open_session_focused(
        &mut self,
        id: SessionId,
        focus: Option<SessionFocus>,
        cx: &mut Context<Self>,
    ) {
        self.session_focus = focus;
        self.view = View::Session(id);
        self.sample_usage(id);
        self.close_terminal();
        if let Some((view, pair, child)) = spawn_terminal(cx) {
            self.terminal = Some(view);
            self.pty = Some(pair);
            self.child = Some(child);
        }
        cx.notify();
    }

    /// Tear down the embedded terminal (closing the PTY exits its shell).
    fn close_terminal(&mut self) {
        self.terminal = None;
        self.pty = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }

    fn dispatch(&self, command: Command) {
        let app = self.app.clone();
        self.handle.spawn(async move {
            if let Err(e) = app.execute(command).await {
                tracing::warn!("command failed: {e}");
            }
        });
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
        let tools = self.app.core().list_tools().unwrap_or_default();
        let selected_tool = self.start_tool.or_else(|| tools.first().map(|t| t.id));

        // Tool picker.
        let tool_row = h_flex().gap_2().children(tools.into_iter().map(|t| {
            let id = t.id;
            let chosen = selected_tool == Some(id);
            let btn = Button::new(SharedString::from(format!("pick-{id}"))).label(t.name);
            let btn = if chosen { btn.primary() } else { btn.outline() };
            btn.on_click(cx.listener(move |this, _, _, cx| {
                this.start_tool = Some(id);
                cx.notify();
            }))
        }));

        // Origin toggle.
        let origin_row = h_flex()
            .gap_2()
            .child(origin_btn(self, cx, Origin::Fresh, "Fresh"))
            .child(origin_btn(self, cx, Origin::PreExisting, "Pre-existing"));

        // Backend picker.
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

        let mut form = v_flex()
            .gap_3()
            .child(field("Tool", tool_row))
            .child(field(
                "Objective",
                div().child(Input::new(&self.inputs.objective)),
            ))
            .child(field(
                "Tasks file (tasks.md)",
                div().child(Input::new(&self.inputs.tasks_path)),
            ))
            .child(field("Environment", origin_row));
        if self.start_origin == Origin::PreExisting {
            form = form.child(field(
                "Worktree",
                div().child(Input::new(&self.inputs.worktree)),
            ));
        }
        form = form.child(field("Backend", backend_row)).child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("do-start")
                        .primary()
                        .label("Start session")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.submit_start(cx);
                        })),
                )
                .child(
                    Button::new("cancel-start")
                        .ghost()
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.view = View::Nav(NavItem::Tasks);
                            cx.notify();
                        })),
                ),
        );

        screen(
            "Start a session",
            "Select a tool + objective, choose an environment, launch",
        )
        .child(card().child(form))
    }

    fn submit_start(&mut self, cx: &mut Context<Self>) {
        let tools = self.app.core().list_tools().unwrap_or_default();
        let Some(tool_id) = self.start_tool.or_else(|| tools.first().map(|t| t.id)) else {
            return;
        };
        let tasks_file = self.input_value(&self.inputs.tasks_path, cx);
        if tasks_file.is_empty() {
            return;
        }
        let worktree = if self.start_origin == Origin::PreExisting {
            let path = self.input_value(&self.inputs.worktree, cx);
            if path.is_empty() {
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
        self.dispatch(Command::StartSession(req));
        self.view = View::Nav(NavItem::Fleet);
        cx.notify();
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
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_session_focused(link.session, link.focus, cx);
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
                            this.dispatch(Command::ConfirmCompletion(id));
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
                            this.dispatch(Command::StopSession(id));
                            cx.notify();
                        })),
                );
            }
            row = row.child(
                Button::new(SharedString::from(format!("clean-{id}")))
                    .ghost()
                    .label("Clean up")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dispatch(Command::CleanUp(id));
                        cx.notify();
                    })),
            );
            row = row.child(
                Button::new(SharedString::from(format!("open-{id}")))
                    .outline()
                    .label("Open")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_session(id, cx);
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
            .child(badge_pill(&vm.badge))
            .child(div().font_semibold().child(vm.tool.clone()))
            .child(div().text_sm().opacity(0.6).child(vm.objective.clone()));

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

        // Header lifecycle controls in prototype order, from the view-model: Send input
        // and Clean up gated with a stated reason, Stop only while the run is on,
        // Confirm completion / Start similar per state.
        let mut controls = h_flex().gap_2().items_center();
        for c in &vm.controls {
            controls = controls.child(self.control_button(id, c, cx));
        }

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
                            this.dispatch(Command::SendInput {
                                session: id,
                                data: format!("{text}\n").into_bytes().into(),
                            });
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

        // Task board from the view-model rows (badge label/color already resolved).
        let mut board_el = v_flex().gap_1().child(section_label("Task board"));
        if vm.board.is_empty() {
            board_el = board_el.child(empty("No tracked tasks."));
        }
        for t in &vm.board {
            board_el = board_el.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(badge_pill(&t.badge))
                    .child(div().font_semibold().text_sm().child(t.id.clone()))
                    .child(div().text_sm().child(t.description.clone())),
            );
        }

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

        // Terminal pane: a live embedded terminal when open, else the captured-output tail.
        let terminal_pane = if let Some(term) = &self.terminal {
            v_flex()
                .gap_1()
                .child(section_label("Terminal"))
                .children(trim_notice)
                .child(
                    div()
                        .w_full()
                        .h(px(380.0))
                        .rounded_md()
                        .overflow_hidden()
                        .bg(gpui::rgba(0x000000a0))
                        .child(term.clone()),
                )
        } else {
            let output = self.app.core().session_output(id, 8192);
            v_flex()
                .gap_1()
                .child(section_label("Output"))
                .children(trim_notice)
                .child(
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
                        }),
                )
        };

        card()
            .gap_4()
            .child(header)
            .children(banner)
            .child(controls)
            .child(send_row)
            .children(input_notice)
            .child(board_el)
            .child(rail)
            .child(terminal_pane)
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
                this.dispatch(Command::ConfirmCompletion(session));
                cx.notify();
            })),
            "Stop" => btn.on_click(cx.listener(move |this, _, _, cx| {
                this.dispatch(Command::StopSession(session));
                cx.notify();
            })),
            "Clean up" => btn.on_click(cx.listener(move |this, _, _, cx| {
                this.dispatch(Command::CleanUp(session));
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

        // Resources — or the unknown-state notice (never shown as healthy, s-908).
        el = el.child(section_label("Resources"));
        if let Some(notice) = rail.unavailable_notice {
            el = el.child(div().text_xs().opacity(0.7).child(notice));
        }
        if let Some(res) = &rail.resources {
            let spark = |s: &crate::screens::session::ResourceSpark| {
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_xs().opacity(0.6).w(px(64.0)).child(s.label))
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_xs()
                            .child(s.value_text.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.4)
                            .child(format!("{} samples", s.history.len())),
                    )
            };
            el = el
                .child(spark(&res.cpu))
                .child(spark(&res.memory))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().text_xs().opacity(0.6).w(px(64.0)).child("Disk"))
                        .child(
                            div()
                                .font_family(mono_family())
                                .text_xs()
                                .child(res.disk.value_text.clone()),
                        ),
                )
                .child(
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
            el = el.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(node)
                    .child(div().text_xs().child(entry.label.clone())),
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
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_session(link.session, cx);
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
                            this.dispatch(Command::ConnectDiscovered(did.clone()));
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
        self.dispatch(Command::RegisterTool(ToolDef {
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
        }));
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
                .child(
                    Button::new("save-settings")
                        .primary()
                        .label("Save")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let limit = this.input_value(&this.inputs.concurrency, cx);
                            let limit = if limit.is_empty() {
                                None
                            } else {
                                limit.parse::<usize>().ok()
                            };
                            this.app.core().set_concurrency_limit(limit);
                            if let Ok(secs) =
                                this.input_value(&this.inputs.stall, cx).parse::<u64>()
                            {
                                this.app.core().set_stall_interval(secs);
                            }
                            // Idle rate: persisted via the command path (FR-021b).
                            let rate = this.input_value(&this.inputs.idle_rate, cx);
                            let rate = if rate.is_empty() {
                                Some(None)
                            } else {
                                rate.parse::<f64>().ok().map(Some)
                            };
                            if let Some(rate) = rate {
                                this.dispatch(Command::SetIdleRate {
                                    backend: idle_backend,
                                    rate,
                                });
                            }
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

/// Start a real PTY running a shell and wrap it in a terminal view.
fn spawn_terminal(
    cx: &mut Context<AppRoot>,
) -> Option<(
    Entity<TerminalView>,
    PtyPair,
    Box<dyn portable_pty::Child + Send + Sync>,
)> {
    let (cols, rows) = (100usize, 30usize);
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        })
        .ok()?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
    let child = pair.slave.spawn_command(CommandBuilder::new(shell)).ok()?;
    let reader = pair.master.try_clone_reader().ok()?;
    let writer = pair.master.take_writer().ok()?;
    let config = TerminalConfig {
        cols,
        rows,
        font_family: mono_family().to_string(),
        font_size: px(13.0),
        line_height_multiplier: 1.0,
        scrollback: 5000,
        padding: Edges::all(px(6.0)),
        colors: ColorPalette::default(),
    };
    let view = cx.new(|cx| TerminalView::new(writer, reader, config, cx));
    Some((view, pair, child))
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
