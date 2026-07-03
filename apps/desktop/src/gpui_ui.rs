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
use gpui_component::{h_flex, v_flex, IconName, Root, StyledExt, TitleBar};
use gpui_platform::application;
use gpui_terminal::{ColorPalette, TerminalConfig, TerminalView};
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use tokio::runtime::Handle;

use daedalus_app::{App, AppQuery, Command};
use daedalus_proto::{
    ArtifactRef, Availability, BackendKind, Capabilities, DiscoveredSession, InvocationSpec,
    Objective, ObjectiveId, Origin, SessionId, SessionStatus, StartSessionRequest, TaskStatus,
    ToolDef, ToolId, WorktreeRef,
};

use crate::app::{NavItem, StatusCounts};
use crate::theme::{Color, StatusTone, Theme as Palette};

/// One kanban card: `(tool name, task id, description)`.
type TaskCard = (String, String, String);
/// A kanban column: `(status, heading, cards)`.
type Column = (TaskStatus, &'static str, Vec<TaskCard>);

/// Convert a design-system [`Color`] to a GPUI color.
fn col(c: Color) -> Rgba {
    gpui::rgb(((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32))
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
                // Dark by default (the locked design), with the platform accent tinted in.
                gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
                let accent: gpui::Hsla = col(crate::theme::Skin::from_host_os().accent()).into();
                let on_accent: gpui::Hsla = gpui::rgb(0xffffff).into();
                {
                    let theme = gpui_component::Theme::global_mut(cx);
                    theme.primary = accent;
                    theme.primary_foreground = on_accent;
                    theme.sidebar_primary = accent;
                    theme.sidebar_primary_foreground = on_accent;
                }

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

                    let root = cx.new(|cx| AppRoot::new(app.clone(), handle.clone(), inputs, cx));
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
fn ensure_tool(app: &App) -> ToolId {
    let def = ToolDef {
        name: "claude".to_string(),
        invocation: InvocationSpec {
            program: "bash".to_string(),
            args: vec!["-lc".to_string(), "echo working...; sleep 5".to_string()],
            env: Vec::new(),
        },
        capabilities: Capabilities {
            accepts_interactive_input: true,
        },
    };
    match app.core().register_tool(def) {
        Ok(id) => id,
        Err(_) => app
            .core()
            .list_tools()
            .unwrap_or_default()
            .into_iter()
            .find(|t| t.name == "claude")
            .map(|t| t.id)
            .unwrap_or_default(),
    }
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
    // Latest discovered sessions (refreshed on the timer via the tokio runtime).
    discovered: Vec<DiscoveredSession>,
    // The embedded terminal for the open session (a real PTY via a shell); the PTY handles
    // are held so the pty/child stay alive while the terminal is shown.
    terminal: Option<Entity<TerminalView>>,
    pty: Option<PtyPair>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    focus_handle: FocusHandle,
}

impl AppRoot {
    fn new(app: App, handle: Handle, inputs: Inputs, cx: &mut Context<Self>) -> Self {
        // Refresh live state ~2×/sec: re-poll discovery on the tokio runtime, then notify.
        let app_bg = app.clone();
        let handle_bg = handle.clone();
        cx.spawn(async move |this, cx| loop {
            let a = app_bg.clone();
            let discovered = handle_bg
                .spawn(async move { a.core().discovered().await })
                .await
                .unwrap_or_default();
            let _ = this.update(cx, |this, cx| {
                this.discovered = discovered;
                cx.notify();
            });
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
        })
        .detach();

        Self {
            app,
            handle,
            palette: Palette::host_default(),
            view: View::Nav(NavItem::Tasks),
            inputs,
            start_tool: None,
            start_origin: Origin::Fresh,
            start_backend: BackendKind::Fake,
            tool_accepts_input: true,
            discovered: Vec::new(),
            terminal: None,
            pty: None,
            child: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Open a session's detail and start an embedded terminal (a shell in a real PTY).
    fn open_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
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
        let counts = StatusCounts::from_app(&self.app);
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
                    .child(count_chip(p, StatusTone::Awaiting, counts.attention))
                    .child(count_chip(p, StatusTone::Completed, counts.completed))
                    .child(count_chip(p, StatusTone::Failed, counts.failed)),
            )
            .child(
                h_flex().flex_1().justify_end().items_center().pr_2().child(
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

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let nav = [
            (NavItem::Tasks, "Tasks", IconName::LayoutDashboard),
            (NavItem::Fleet, "Sessions", IconName::SquareTerminal),
            (NavItem::Discover, "Discover", IconName::Globe),
            (NavItem::Tools, "Tools", IconName::Bot),
            (NavItem::Backends, "Environments", IconName::Folder),
            (NavItem::Settings, "Settings", IconName::Settings),
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

        let backends: Vec<SidebarMenuItem> = self
            .app
            .core()
            .backend_registry()
            .all()
            .iter()
            .map(|b| SidebarMenuItem::new(format!("{:?}", b.kind())))
            .collect();

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
            .child(SidebarGroup::new("Backends").child(SidebarMenu::new().children(backends)))
    }

    fn render_content(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.view {
            View::Start => self.render_start(cx).into_any_element(),
            View::Session(id) if self.app.session(id).is_some() => {
                self.render_detail(id, cx).into_any_element()
            }
            View::Session(_) => self.render_sessions(cx).into_any_element(),
            View::Nav(NavItem::Tasks) => self.render_tasks().into_any_element(),
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

        let _ = p;
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
        };
        self.dispatch(Command::StartSession(req));
        self.view = View::Nav(NavItem::Fleet);
        cx.notify();
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
            if !s.status.is_terminal() {
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
        let Some(detail) = self.app.session(id) else {
            return screen("Session", "not found");
        };
        let s = &detail.session;
        let tone = StatusTone::from_session(s.status);

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
            .child(status_pill(p, tone))
            .child(div().font_semibold().child(detail.tool.name.clone()))
            .child(
                div()
                    .text_sm()
                    .opacity(0.6)
                    .child(detail.objective.description.clone()),
            );

        let mut controls = h_flex().gap_2();
        if s.status == SessionStatus::AwaitingConfirmation {
            controls = controls.child(
                Button::new("d-confirm")
                    .primary()
                    .label("Confirm")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dispatch(Command::ConfirmCompletion(id));
                        cx.notify();
                    })),
            );
        }
        if !s.status.is_terminal() {
            controls = controls.child(Button::new("d-stop").danger().label("Stop").on_click(
                cx.listener(move |this, _, _, cx| {
                    this.dispatch(Command::StopSession(id));
                    cx.notify();
                }),
            ));
        }
        controls = controls.child(Button::new("d-clean").ghost().label("Clean up").on_click(
            cx.listener(move |this, _, _, cx| {
                this.dispatch(Command::CleanUp(id));
                this.close_terminal();
                this.view = View::Nav(NavItem::Fleet);
                cx.notify();
            }),
        ));

        let accepts = s.accepts_input;
        let send_row = h_flex()
            .gap_2()
            .items_center()
            .child(div().flex_1().child(Input::new(&self.inputs.send)))
            .child(if accepts {
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
                    }))
            } else {
                Button::new("send").label("Send input (unsupported)")
            });

        let board = self.app.task_board(id);
        let mut board_el = v_flex().gap_1().child(section_label("Task board"));
        if board.is_empty() {
            board_el = board_el.child(empty("No tracked tasks."));
        }
        for t in board {
            let ttone = StatusTone::from_task(t.status);
            board_el = board_el.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(status_pill(p, ttone))
                    .child(div().font_semibold().text_sm().child(t.id.0.clone()))
                    .child(div().text_sm().child(t.description.clone())),
            );
        }

        let mut tele = h_flex().gap_4().child(section_label("Telemetry"));
        for m in self.app.resource_usage(id) {
            tele = tele.child(
                v_flex()
                    .gap_0()
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.6)
                            .child(format!("{:?}", m.metric)),
                    )
                    .child(div().font_semibold().child(format!("{:.0}", m.value))),
            );
        }

        // Terminal pane: a live embedded terminal when open, else the captured-output tail.
        let terminal_pane = if let Some(term) = &self.terminal {
            v_flex().gap_1().child(section_label("Terminal")).child(
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
            v_flex().gap_1().child(section_label("Output")).child(
                div()
                    .w_full()
                    .p_2()
                    .rounded_md()
                    .bg(gpui::rgba(0x00000040))
                    .font_family("monospace")
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
            .child(controls)
            .child(send_row)
            .child(board_el)
            .child(tele)
            .child(terminal_pane)
    }

    fn render_tasks(&self) -> impl IntoElement {
        let p = self.palette;
        let mut cols: [Column; 4] = [
            (TaskStatus::InProgress, "In progress", Vec::new()),
            (TaskStatus::Blocked, "Blocked", Vec::new()),
            (TaskStatus::Todo, "To do", Vec::new()),
            (TaskStatus::Done, "Done", Vec::new()),
        ];
        for s in self.app.fleet() {
            let _ = self.app.core().refresh_task_board(s.id);
            for t in self.app.task_board(s.id) {
                if let Some(c) = cols.iter_mut().find(|c| c.0 == t.status) {
                    c.2.push((s.tool_name.clone(), t.id.0.clone(), t.description.clone()));
                }
            }
        }

        let columns = h_flex()
            .gap_4()
            .items_start()
            .w_full()
            .children(cols.into_iter().map(|(status, label, tasks)| {
                let tone = StatusTone::from_task(status);
                v_flex()
                    .flex_1()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(status_dot(p, tone))
                            .child(div().font_semibold().child(label.to_string()))
                            .child(
                                div()
                                    .text_xs()
                                    .opacity(0.6)
                                    .child(format!("{}", tasks.len())),
                            ),
                    )
                    .children(tasks.into_iter().map(|(tool, id, desc)| {
                        card()
                            .child(div().text_xs().opacity(0.6).child(tool))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(div().font_semibold().text_sm().child(id))
                                    .child(div().text_sm().child(desc)),
                            )
                    }))
            }));

        screen("Tasks", "What every agent is working on, by task").child(columns)
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
                                .font_family("monospace")
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
        let mut list = v_flex().gap_2().w_full();
        for b in self.app.core().backend_registry().all() {
            list = list.child(
                card().child(
                    h_flex()
                        .items_center()
                        .gap_3()
                        .child(status_dot(p, StatusTone::Unknown))
                        .child(div().font_semibold().child(format!("{:?}", b.kind()))),
                ),
            );
        }
        screen("Environments", "Sandbox backends and availability").child(list)
    }

    fn render_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                .child(
                    Button::new("save-settings")
                        .primary()
                        .label("Save")
                        .on_click(cx.listener(|this, _, _, cx| {
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
                            cx.notify();
                        })),
                ),
        );
        screen("Settings", "Local-first: no open listener by default")
            .child(form)
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
        font_family: "JetBrains Mono".to_string(),
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

/// A small colored dot for a status tone.
fn status_dot(p: Palette, tone: StatusTone) -> impl IntoElement {
    div()
        .w(px(9.0))
        .h(px(9.0))
        .rounded_full()
        .bg(col(tone.color(p.skin)))
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
