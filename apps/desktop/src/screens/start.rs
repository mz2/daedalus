//! Start-session screen (US1, §6.3): tool pick → SDD objective ref → env choice (with the
//! worktree note for pre-existing) → backend choice → launch, with validation and the
//! named failure states (FR-005/026/002a) — copy verbatim from the prototype
//! `StartSession` (`design/prototype/screens.jsx`).

use daedalus_app::App;
use daedalus_proto::{BackendKind, BackendStatus, Origin, StartSessionRequest, ToolId};

use crate::components::{ActionButton, ButtonIntent, FailureNotice, NoticeTone};

/// The worktree-isolation reassurance under the pre-existing environment picker
/// (FR-002a; prototype `banner-info`).
pub const WORKTREE_REASSURANCE: &str = "Work happens in an isolated git worktree — your \
     existing checkout and uncommitted work are never disturbed.";

/// The trust banner in the Review & launch modal (FR-031).
pub const SECRETS_TRUST_BANNER: &str = "Secrets are pre-provisioned in the environment — \
     Daedalus never displays or captures them.";

/// The tool field error when launch is attempted without a tool (prototype `toolErr`).
pub const TOOL_FIELD_ERROR: &str = "Choose an agentic tool — nothing can launch without one.";

/// The objective field error when launch is attempted without one (prototype `objErr`).
pub const OBJECTIVE_FIELD_ERROR: &str =
    "Pick a spec branch — the agent needs an objective to work toward.";

/// The tint of the launch-footer hint (prototype `start-foot-hint` classes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterTone {
    /// Default / ready.
    Neutral,
    /// Validation failure after an attempted launch.
    Error,
    /// At the concurrency limit.
    Warn,
}

/// The launch-footer hint: text + tone (prototype start footer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterHint {
    /// Hint text.
    pub text: &'static str,
    /// Tint.
    pub tone: FooterTone,
}

/// The named Start-flow failure/edge states (brief §6.3) — each shapes into the
/// [`FailureNotice`] pattern with the prototype's copy: a reason and next actions,
/// never a dead end.
#[derive(Debug, Clone, PartialEq)]
pub enum StartFailure {
    /// Environment provisioning failed (FR-005): create is atomic, so nothing to clean up.
    ProvisioningFailed {
        /// The raw backend error (rendered monospace).
        reason: String,
    },
    /// The concurrency limit rejected the launch (FR-026).
    LimitReached {
        /// Sessions currently running.
        running: usize,
        /// The configured limit.
        limit: usize,
    },
    /// A pre-existing environment's host is not responding (§6.3).
    EnvUnreachable {
        /// Environment name (e.g. "acme/infra").
        env: String,
        /// Host label (e.g. "eu-fra-1").
        host: String,
        /// When it was last responsive (e.g. "14:02").
        since: String,
    },
    /// The isolated worktree could not be created (FR-002a).
    WorktreeFailed {
        /// The raw `git worktree` error (rendered monospace).
        reason: String,
    },
}

impl StartFailure {
    /// Shape into the [`FailureNotice`] pattern — titles, reasons, action labels, and
    /// reassurance notes verbatim from the prototype `StartSession`.
    #[must_use]
    pub fn notice(&self) -> FailureNotice {
        let btn = |label: &str| ActionButton::enabled(label, ButtonIntent::Ghost);
        match self {
            Self::ProvisioningFailed { reason } => FailureNotice {
                tone: NoticeTone::Error,
                title: "Provisioning failed".into(),
                reason: reason.clone(),
                mono_reason: true,
                actions: vec![
                    btn("Try again"),
                    btn("Choose another backend"),
                    btn("View host"),
                ],
                note: Some("No session was created — nothing to clean up.".into()),
            },
            Self::LimitReached { running, limit } => FailureNotice {
                tone: NoticeTone::Warn,
                title: format!("Concurrency limit reached — {running} of {limit} sessions running"),
                reason: "The session limit keeps agents from exhausting this host. Finish \
                         or stop a running session, or raise the limit to launch another."
                    .into(),
                mono_reason: false,
                actions: vec![btn("Open Fleet"), btn("Raise limit in Settings")],
                note: None,
            },
            Self::EnvUnreachable { env, host, since } => FailureNotice {
                tone: NoticeTone::Error,
                title: "Environment unreachable".into(),
                reason: format!(
                    "{env} lives on host {host}, which hasn't responded since {since} — it \
                     can't take a session until the tunnel is back. Everything else here \
                     still works."
                ),
                mono_reason: false,
                actions: vec![
                    btn("Rescan"),
                    btn("Pick another environment"),
                    btn("Use a fresh environment"),
                ],
                note: None,
            },
            Self::WorktreeFailed { reason } => FailureNotice {
                tone: NoticeTone::Error,
                title: "Couldn't create an isolated worktree".into(),
                reason: reason.clone(),
                mono_reason: true,
                actions: vec![btn("Try another branch"), btn("Use a fresh environment")],
                note: Some(
                    "Your existing checkout and uncommitted work were not touched — the \
                     worktree never attached."
                        .into(),
                ),
            },
        }
    }
}

/// A tool option in the picker.
#[derive(Debug, Clone)]
pub struct ToolOption {
    /// Tool id.
    pub id: ToolId,
    /// Display name.
    pub name: String,
    /// Whether it accepts input (shown as a capability chip).
    pub accepts_input: bool,
}

/// The start-session form view-model.
#[derive(Debug, Clone)]
pub struct StartScreen {
    /// Registered tools to choose from.
    pub tools: Vec<ToolOption>,
    /// Backends and their availability (unavailable ones are shown disabled).
    pub backends: Vec<BackendStatus>,
    /// The active failure/edge state, when one is showing (its notice sits above the form).
    pub failure: Option<StartFailure>,
}

impl StartScreen {
    /// Build the form from current state.
    pub async fn build(app: &App) -> Self {
        let tools = app
            .core()
            .list_tools()
            .unwrap_or_default()
            .into_iter()
            .map(|t| ToolOption {
                id: t.id,
                name: t.name,
                accepts_input: t.capabilities.accepts_interactive_input,
            })
            .collect();
        let backends = app.core().backends_status().await;
        Self {
            tools,
            backends,
            failure: None,
        }
    }

    /// Validate a draft request before launch, returning the field error the UI must
    /// surface (prototype `FieldError` copy; pre-existing requires a worktree — FR-002a).
    pub fn validate(&self, req: &StartSessionRequest) -> Result<(), String> {
        if !self.tools.iter().any(|t| t.id == req.tool_id) {
            return Err(TOOL_FIELD_ERROR.into());
        }
        if req.objective.artifact_ref.tasks_file.trim().is_empty() {
            return Err(OBJECTIVE_FIELD_ERROR.into());
        }
        if req.origin == Origin::PreExisting && req.worktree.is_none() {
            return Err("a pre-existing environment needs an isolated git worktree".into());
        }
        if !self.backends.iter().any(|b| {
            b.kind == req.backend && b.availability != daedalus_proto::Availability::Unavailable
        }) {
            return Err("the selected backend is unavailable".into());
        }
        Ok(())
    }

    /// The launch-footer hint (prototype `start-foot-hint`): the limit warning wins, then
    /// the attempted-launch validation error, then ready / incomplete.
    #[must_use]
    pub fn footer_hint(attempted: bool, valid: bool, at_limit: bool) -> FooterHint {
        if at_limit {
            FooterHint {
                text: "At the concurrency limit — stop a session or raise the limit to launch",
                tone: FooterTone::Warn,
            }
        } else if attempted && !valid {
            FooterHint {
                text: "Fix the highlighted sections — a tool and an objective are required",
                tone: FooterTone::Error,
            }
        } else if valid {
            FooterHint {
                text: "Ready to launch",
                tone: FooterTone::Neutral,
            }
        } else {
            FooterHint {
                text: "Complete tool, objective, and environment",
                tone: FooterTone::Neutral,
            }
        }
    }

    /// The available (non-unavailable) backend kinds.
    #[must_use]
    pub fn available_backends(&self) -> Vec<BackendKind> {
        self.backends
            .iter()
            .filter(|b| b.availability != daedalus_proto::Availability::Unavailable)
            .map(|b| b.kind)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validate_rejects_preexisting_without_worktree() {
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let screen = StartScreen::build(&fx.app).await;
        let mut req = fx.fresh_request(tool);
        req.origin = Origin::PreExisting;
        req.worktree = None;
        assert!(screen.validate(&req).is_err());
    }

    #[tokio::test]
    async fn validation_field_errors_use_the_prototype_copy() {
        // FR-005/§6.3 (T085): the attempted-launch field errors match the prototype
        // `StartSession` toolErr/objErr verbatim.
        let fx = daedalus_tests::Fixture::new();
        let tool = fx.register_sample_tool("claude");
        let screen = StartScreen::build(&fx.app).await;

        let mut req = fx.fresh_request(tool);
        req.tool_id = daedalus_proto::ToolId::new();
        assert_eq!(
            screen.validate(&req).unwrap_err(),
            "Choose an agentic tool — nothing can launch without one."
        );

        let mut req = fx.fresh_request(tool);
        req.objective.artifact_ref.tasks_file = "  ".into();
        assert_eq!(
            screen.validate(&req).unwrap_err(),
            "Pick a spec branch — the agent needs an objective to work toward."
        );
    }

    #[test]
    fn footer_hint_covers_limit_error_ready_and_incomplete() {
        // Prototype start footer (T085): limit > validation > ready > incomplete.
        let limit = StartScreen::footer_hint(true, true, true);
        assert_eq!(
            limit.text,
            "At the concurrency limit — stop a session or raise the limit to launch"
        );
        assert_eq!(limit.tone, FooterTone::Warn);

        let invalid = StartScreen::footer_hint(true, false, false);
        assert_eq!(
            invalid.text,
            "Fix the highlighted sections — a tool and an objective are required"
        );
        assert_eq!(invalid.tone, FooterTone::Error);

        assert_eq!(
            StartScreen::footer_hint(false, true, false).text,
            "Ready to launch"
        );
        assert_eq!(
            StartScreen::footer_hint(false, false, false).text,
            "Complete tool, objective, and environment"
        );
    }

    #[test]
    fn provisioning_failure_notice_matches_the_prototype() {
        // FR-005 (T085): atomic create — reason in mono, three next actions, and the
        // "nothing to clean up" reassurance, copy verbatim.
        let notice = StartFailure::ProvisioningFailed {
            reason:
                "Workshop create failed: image \"ubuntu-24.04-agents\" not found on host eu-fra-1"
                    .into(),
        }
        .notice();
        assert_eq!(notice.tone, NoticeTone::Error);
        assert_eq!(notice.title, "Provisioning failed");
        assert!(notice.mono_reason);
        assert_eq!(
            notice.reason,
            "Workshop create failed: image \"ubuntu-24.04-agents\" not found on host eu-fra-1"
        );
        let labels: Vec<&str> = notice.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["Try again", "Choose another backend", "View host"]);
        assert_eq!(
            notice.note.as_deref(),
            Some("No session was created — nothing to clean up.")
        );
    }

    #[test]
    fn limit_reached_notice_matches_the_prototype() {
        // FR-026 (T085): warn tone, counted title, and the two next actions.
        let notice = StartFailure::LimitReached {
            running: 6,
            limit: 6,
        }
        .notice();
        assert_eq!(notice.tone, NoticeTone::Warn);
        assert_eq!(
            notice.title,
            "Concurrency limit reached — 6 of 6 sessions running"
        );
        assert_eq!(
            notice.reason,
            "The session limit keeps agents from exhausting this host. Finish or stop a \
             running session, or raise the limit to launch another."
        );
        let labels: Vec<&str> = notice.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["Open Fleet", "Raise limit in Settings"]);
        assert_eq!(notice.note, None);
    }

    #[test]
    fn env_unreachable_notice_matches_the_prototype() {
        // §6.3 (T085): the reason names the env + host + last-response time and ends with
        // the "everything else still works" reassurance.
        let notice = StartFailure::EnvUnreachable {
            env: "acme/infra".into(),
            host: "eu-fra-1".into(),
            since: "14:02".into(),
        }
        .notice();
        assert_eq!(notice.title, "Environment unreachable");
        assert!(!notice.mono_reason);
        assert_eq!(
            notice.reason,
            "acme/infra lives on host eu-fra-1, which hasn't responded since 14:02 — it \
             can't take a session until the tunnel is back. Everything else here still works."
        );
        let labels: Vec<&str> = notice.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Rescan",
                "Pick another environment",
                "Use a fresh environment"
            ]
        );
    }

    #[test]
    fn worktree_failure_notice_matches_the_prototype() {
        // FR-002a (T085): raw git error in mono + the "never attached" reassurance.
        let notice = StartFailure::WorktreeFailed {
            reason: "git worktree add failed: branch 'feature/auth' is already checked out at /work/repo"
                .into(),
        }
        .notice();
        assert_eq!(notice.title, "Couldn't create an isolated worktree");
        assert!(notice.mono_reason);
        let labels: Vec<&str> = notice.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["Try another branch", "Use a fresh environment"]);
        assert_eq!(
            notice.note.as_deref(),
            Some(
                "Your existing checkout and uncommitted work were not touched — the \
                 worktree never attached."
            )
        );
    }

    #[test]
    fn reassurance_banners_carry_the_prototype_copy() {
        // FR-002a worktree note + FR-031 secrets trust banner, verbatim.
        assert_eq!(
            WORKTREE_REASSURANCE,
            "Work happens in an isolated git worktree — your existing checkout and \
             uncommitted work are never disturbed."
        );
        assert_eq!(
            SECRETS_TRUST_BANNER,
            "Secrets are pre-provisioned in the environment — Daedalus never displays or \
             captures them."
        );
    }
}
