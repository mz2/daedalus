//! The GPUI render loop (only compiled with `--features gpui`), built on the
//! **gpui-component** widget library so the surface matches the locked design (`design/`):
//! a client-side title bar, a sidebar of navigation + backends, and nav-switched screens
//! (Tasks board, Sessions, Discover, Tools, Environments, Settings). It needs a GPU/display
//! (research R-UI); live state refreshes on a timer and commands run on the tokio runtime.

use gpui::{
    div, prelude::*, px, App as GpuiApp, Bounds, Context, FocusHandle, Focusable, Rgba,
    SharedString, Window, WindowBounds, WindowKind, WindowOptions,
};
use gpui_component::sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem};
use gpui_component::{button::Button, button::ButtonVariants};
use gpui_component::{h_flex, v_flex, Root, StyledExt, TitleBar};
use gpui_platform::application;
use tokio::runtime::Handle;

use daedalus_app::{App, AppQuery, Command};
use daedalus_proto::{
    ArtifactRef, BackendKind, Capabilities, InvocationSpec, Objective, ObjectiveId, Origin,
    SessionStatus, TaskStatus, ToolDef, ToolId,
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

/// Open the Daedalus window and run the GPUI event loop (blocks the main thread).
pub fn run(app: App, handle: Handle) {
    let tool = ensure_tool(&app);
    let tasks_path = std::env::temp_dir().join("daedalus-sample-tasks.md");
    let _ = std::fs::write(
        &tasks_path,
        "- [x] T001 Provision environment\n- [ ] T002 Implement feature\n- [ ] T003 Run tests\n",
    );
    let tasks_path = tasks_path.to_string_lossy().into_owned();

    let launched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        application().run(move |cx: &mut GpuiApp| {
            gpui_component::init(cx);
            // Dark by default (the locked design), with the platform-native accent tinted in
            // (Ubuntu orange on Linux / warm amber on macOS) so primary buttons + active nav
            // match `design/`.
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
                    width: px(760.0),
                    height: px(480.0),
                }),
                kind: WindowKind::Normal,
                #[cfg(target_os = "linux")]
                window_background: gpui::WindowBackgroundAppearance::Transparent,
                #[cfg(target_os = "linux")]
                window_decorations: Some(gpui::WindowDecorations::Client),
                ..Default::default()
            };

            cx.open_window(options, |window, cx| {
                let root = cx.new(|cx| {
                    AppRoot::new(app.clone(), handle.clone(), tool, tasks_path.clone(), cx)
                });
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

/// Register the demo tool (idempotent — reuses the persisted one by name).
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
            .expect("demo tool present"),
    }
}

struct AppRoot {
    app: App,
    handle: Handle,
    palette: Palette,
    active: NavItem,
    tool: ToolId,
    tasks_path: String,
    focus_handle: FocusHandle,
}

impl AppRoot {
    fn new(
        app: App,
        handle: Handle,
        tool: ToolId,
        tasks_path: String,
        cx: &mut Context<Self>,
    ) -> Self {
        // Refresh live state ~2×/sec so backend/task changes appear without polling.
        cx.spawn(async move |this, cx| loop {
            let _ = this.update(cx, |_, cx| cx.notify());
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
        })
        .detach();

        Self {
            app,
            handle,
            palette: Palette::host_default(),
            active: NavItem::Tasks,
            tool,
            tasks_path,
            focus_handle: cx.focus_handle(),
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

    fn start_request(&self) -> daedalus_proto::StartSessionRequest {
        daedalus_proto::StartSessionRequest {
            tool_id: self.tool,
            objective: Objective {
                id: ObjectiveId::new(),
                artifact_ref: ArtifactRef {
                    root: std::env::temp_dir().to_string_lossy().into_owned(),
                    tasks_file: self.tasks_path.clone(),
                },
                description: "Demo objective".to_string(),
            },
            origin: Origin::Fresh,
            worktree: None,
            backend: BackendKind::Fake,
        }
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
                            let req = this.start_request();
                            this.dispatch(Command::StartSession(req));
                            cx.notify();
                        })),
                ),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let nav = [
            (NavItem::Tasks, "Tasks"),
            (NavItem::Fleet, "Sessions"),
            (NavItem::Discover, "Discover"),
            (NavItem::Tools, "Tools"),
            (NavItem::Backends, "Environments"),
            (NavItem::Settings, "Settings"),
        ];
        let items: Vec<SidebarMenuItem> = nav
            .into_iter()
            .map(|(item, label)| {
                SidebarMenuItem::new(label)
                    .active(self.active == item)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = item;
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
        match self.active {
            NavItem::Tasks => self.render_tasks().into_any_element(),
            NavItem::Fleet => self.render_sessions(cx).into_any_element(),
            NavItem::Discover => self.render_discover().into_any_element(),
            NavItem::Tools => self.render_tools().into_any_element(),
            NavItem::Backends => self.render_environments().into_any_element(),
            NavItem::Settings => self.render_settings().into_any_element(),
        }
    }

    fn render_tasks(&self) -> impl IntoElement {
        let p = self.palette;
        // Aggregate every non-terminal session's tasks into kanban columns.
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

            list = list.child(card().child(row));
        }
        screen("Sessions", "Every session across backends").child(list)
    }

    fn render_discover(&self) -> impl IntoElement {
        screen(
            "Discover",
            "Sessions across local, mDNS, and tunneled hosts",
        )
        .child(empty(
            "No sessions discovered (the fake backend is local-only).",
        ))
    }

    fn render_tools(&self) -> impl IntoElement {
        let mut list = v_flex().gap_2().w_full();
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
        screen("Tools", "Registered agentic tools").child(list)
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

    fn render_settings(&self) -> impl IntoElement {
        screen("Settings", "Local-first: no open listener by default").child(
            card().child(
                v_flex()
                    .gap_1()
                    .child(div().child("Concurrency limit: unlimited"))
                    .child(div().child("Stall interval: 120s"))
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.6)
                            .child("Remote reach only over operator-established tunnels."),
                    ),
            ),
        )
    }
}

/// A screen scaffold: heading + subtitle + a scrollable content area.
fn screen(title: &str, subtitle: &str) -> gpui::Div {
    v_flex().size_full().gap_4().p_5().child(
        v_flex()
            .gap_0()
            .child(div().text_xl().font_semibold().child(title.to_string()))
            .child(div().text_sm().opacity(0.6).child(subtitle.to_string())),
    )
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
