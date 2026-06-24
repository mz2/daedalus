//! The GPUI render loop (only compiled with `--features gpui`).
//!
//! Renders the app shell — title bar with global status counts, a sidebar of backends, and
//! the live fleet with per-session controls — from the `daedalus_app::App` query API, and
//! drives commands back through it. This is the native surface; it needs a GPU/display
//! (research R-UI). Live state is refreshed on a timer; commands run on the tokio runtime.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, rgb, App as GpuiApp, Bounds, ClickEvent, Context, ElementId, Rgba, Role,
    SharedString, Window, WindowBounds, WindowOptions,
};
use gpui_platform::application;
use tokio::runtime::Handle;

use daedalus_app::{App, AppQuery, Command};
use daedalus_proto::{
    ArtifactRef, BackendKind, Capabilities, InvocationSpec, Objective, ObjectiveId, Origin,
    SessionStatus, SessionSummary, StartSessionRequest, TaskStatus, ToolDef, ToolId,
};

use crate::app::StatusCounts;
use crate::components::{ActionButton, ButtonIntent};
use crate::theme::{Color, StatusTone, Theme};

/// Convert a design-system [`Color`] to a GPUI color.
fn col(c: Color) -> Rgba {
    rgb(((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32))
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

    application().run(move |cx: &mut GpuiApp| {
        let bounds = Bounds::centered(None, gpui::size(px(1180.0), px(760.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|cx| {
                    // Refresh live state ~2×/sec so backend/task changes appear without polling.
                    cx.spawn(async move |this, cx| loop {
                        let _ = this.update(cx, |_, cx| cx.notify());
                        cx.background_executor()
                            .timer(Duration::from_millis(500))
                            .await;
                    })
                    .detach();

                    RootView {
                        app: app.clone(),
                        handle: handle.clone(),
                        theme: Theme::host_default(),
                        tool,
                        tasks_path: tasks_path.clone(),
                    }
                })
            },
        )
        .expect("open window");
        cx.activate(true);
    });
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

struct RootView {
    app: App,
    handle: Handle,
    theme: Theme,
    tool: ToolId,
    tasks_path: String,
}

impl RootView {
    /// Fire-and-forget a command on the tokio runtime; the refresh timer reflects the result.
    fn dispatch(&self, command: Command) {
        let app = self.app.clone();
        self.handle.spawn(async move {
            if let Err(e) = app.execute(command).await {
                tracing::warn!("command failed: {e}");
            }
        });
    }

    fn start_request(&self) -> StartSessionRequest {
        StartSessionRequest {
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

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let counts = StatusCounts::from_app(&self.app);
        let fleet = self.app.fleet();
        // Refresh each running session's board so task progress shows live.
        for s in &fleet {
            if !s.status.is_terminal() {
                let _ = self.app.core().refresh_task_board(s.id);
            }
        }

        let title_bar = div()
            .flex()
            .items_center()
            .gap_4()
            .w_full()
            .h(px(48.0))
            .px_4()
            .bg(col(t.surfaces.titlebar))
            .child(div().text_color(col(t.skin.accent())).child("Daedalus"))
            .child(count_chip(t, StatusTone::Running, counts.running))
            .child(count_chip(t, StatusTone::Awaiting, counts.attention))
            .child(count_chip(t, StatusTone::Completed, counts.completed))
            .child(count_chip(t, StatusTone::Failed, counts.failed))
            .child(div().flex_1())
            .child(action_button(
                t,
                "start-session",
                &ActionButton::enabled("Start session", ButtonIntent::Primary),
                cx.listener(|this, _, _, cx| {
                    let req = this.start_request();
                    this.dispatch(Command::StartSession(req));
                    cx.notify();
                }),
            ));

        let mut sidebar = div()
            .flex()
            .flex_col()
            .gap_2()
            .w(px(220.0))
            .h_full()
            .p_3()
            .bg(col(t.surfaces.sidebar))
            .child(section_label(t, "Backends"));
        for backend in self.app.core().backend_registry().all() {
            sidebar = sidebar.child(
                div()
                    .text_color(col(t.surfaces.text_primary))
                    .child(format!("{:?}", backend.kind())),
            );
        }

        let mut content = div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .h_full()
            .p_4()
            .bg(col(t.surfaces.content))
            .child(section_label(t, "Fleet"));
        if fleet.is_empty() {
            content = content.child(
                div()
                    .text_color(col(t.status_color(StatusTone::Unknown)))
                    .child("No sessions yet — click \"Start session\"."),
            );
        }
        for s in fleet {
            content = content.child(self.session_row(t, s, cx));
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(col(t.surfaces.app))
            .text_color(col(t.surfaces.text_primary))
            .font_family(t.skin.ui_font())
            .child(title_bar)
            .child(div().flex().flex_1().w_full().child(sidebar).child(content))
    }
}

impl RootView {
    fn session_row(&self, t: Theme, s: SessionSummary, cx: &mut Context<Self>) -> impl IntoElement {
        let tone = StatusTone::from_session(s.status);
        let id = s.id;
        let board = self.app.task_board(id);
        let done = board
            .iter()
            .filter(|x| x.status == TaskStatus::Done)
            .count();

        let mut row = div()
            .flex()
            .items_center()
            .gap_3()
            .py_1()
            .child(status_dot(t, tone))
            .child(
                div()
                    .w(px(96.0))
                    .text_color(col(t.status_color(tone)))
                    .child(tone.label()),
            )
            .child(
                div()
                    .flex_1()
                    .text_color(col(t.surfaces.text_primary))
                    .child(format!(
                        "{}  ·  {}  ·  tasks {}/{}",
                        s.tool_name,
                        s.objective,
                        done,
                        board.len()
                    )),
            );

        if s.status == SessionStatus::AwaitingConfirmation {
            row = row.child(action_button(
                t,
                SharedString::from(format!("confirm-{id}")),
                &ActionButton::enabled("Confirm", ButtonIntent::Primary),
                cx.listener(move |this, _, _, cx| {
                    this.dispatch(Command::ConfirmCompletion(id));
                    cx.notify();
                }),
            ));
        }
        if !s.status.is_terminal() {
            row = row.child(action_button(
                t,
                SharedString::from(format!("stop-{id}")),
                &ActionButton::enabled("Stop", ButtonIntent::Danger),
                cx.listener(move |this, _, _, cx| {
                    this.dispatch(Command::StopSession(id));
                    cx.notify();
                }),
            ));
        }
        row.child(action_button(
            t,
            SharedString::from(format!("clean-{id}")),
            &ActionButton::enabled("Clean up", ButtonIntent::Ghost),
            cx.listener(move |this, _, _, cx| {
                this.dispatch(Command::CleanUp(id));
                cx.notify();
            }),
        ))
    }
}

/// A small colored dot for a status tone.
fn status_dot(t: Theme, tone: StatusTone) -> impl IntoElement {
    div()
        .w(px(10.0))
        .h(px(10.0))
        .rounded_full()
        .bg(col(t.status_color(tone)))
}

/// A section heading.
fn section_label(t: Theme, text: &str) -> impl IntoElement {
    div()
        .text_color(col(t.surfaces.text_primary))
        .child(text.to_string())
}

/// A "glyph N" count chip in the title bar.
fn count_chip(t: Theme, tone: StatusTone, n: usize) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(status_dot(t, tone))
        .child(
            div()
                .text_color(col(t.surfaces.text_primary))
                .child(format!("{} {}", tone.label(), n)),
        )
}

/// A clickable, accessibility-labelled button rendered from an [`ActionButton`] descriptor.
fn action_button<F>(
    t: Theme,
    id: impl Into<ElementId>,
    spec: &ActionButton,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&ClickEvent, &mut Window, &mut GpuiApp) + 'static,
{
    let (bg, fg) = match spec.intent {
        ButtonIntent::Primary => (t.skin.accent(), Color::WHITE),
        ButtonIntent::Tinted => (t.surfaces.elevated, t.surfaces.text_primary),
        ButtonIntent::Danger => (t.status_color(StatusTone::Failed), Color::WHITE),
        ButtonIntent::Ghost => (t.surfaces.elevated, t.surfaces.text_primary),
    };
    div()
        .id(id.into())
        .role(Role::Button)
        .aria_label(SharedString::from(spec.label.clone()))
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .px_3()
        .py_1()
        .rounded_md()
        .bg(col(bg))
        .text_color(col(fg))
        .child(spec.label.clone())
        .on_click(on_click)
}
