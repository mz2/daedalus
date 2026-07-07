//! Terminal-state, stall, failure, and waiting-for-input detection, including the
//! `AwaitingConfirmation` path (FR-015/015a/015b/020). The core never marks a session
//! `Completed` on agent exit alone — that requires all tracked tasks done or an explicit
//! confirmation. A session blocked on operator input is surfaced as `WaitingForInput` per
//! the tool's declared prompt convention and is never reported stalled.

use daedalus_proto::{Outcome, PromptConvention, SessionId, SessionStatus};
use daedalus_sdk::WaitingState;

use crate::session::state::{transition, Trigger};
use crate::{clock, map_not_found, Core, CoreError};

/// A signal observed about a running agent that may move its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSignal {
    /// The agent process exited cleanly (zero exit). The resulting state depends on whether
    /// all tracked tasks are done (`Completed`) or not (`AwaitingConfirmation`, FR-015a).
    ExitedCleanly,
    /// The agent crashed / exited abnormally ⇒ `Failed` (FR-005).
    Crashed,
    /// No output/progress for the configured stall interval ⇒ `Stalled` (FR-020).
    NoProgress,
    /// Output/progress resumed ⇒ `Stalled → Running`.
    Progress,
}

impl Core {
    /// Apply an observed [`AgentSignal`] to a session, advancing the state machine and
    /// persisting the new status (with an `ended_at` for terminal states).
    ///
    /// The stall detector deliberately **skips** a session that is `WaitingForInput`: it is
    /// blocked on the operator, not stalled (FR-015b).
    pub async fn apply_signal(
        &self,
        id: SessionId,
        signal: AgentSignal,
    ) -> Result<SessionStatus, CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;

        if signal == AgentSignal::NoProgress && session.status == SessionStatus::WaitingForInput {
            return Ok(session.status);
        }

        let (trigger, mut note) = match signal {
            AgentSignal::Crashed => (
                Trigger::AgentCrashed,
                Some("agent crashed or exited abnormally".to_string()),
            ),
            AgentSignal::NoProgress => (
                Trigger::StallDetected,
                Some(format!(
                    "no progress for {}s",
                    self.config.lock().expect("poisoned").stall_interval_secs
                )),
            ),
            AgentSignal::Progress => (Trigger::ProgressResumed, None),
            AgentSignal::ExitedCleanly => {
                let tasks = self.store.list_tasks(id)?;
                let all_done = !tasks.is_empty()
                    && tasks
                        .iter()
                        .all(|t| t.status == daedalus_proto::TaskStatus::Done);
                let trigger = if all_done {
                    Trigger::AllTasksDone
                } else {
                    Trigger::AgentExitedWithUnfinishedTasks
                };
                (trigger, None)
            }
        };

        let next = transition(session.status, trigger)?;
        let ended = next.is_terminal().then(clock::now);
        if next == SessionStatus::AwaitingConfirmation {
            // A clean exit awaiting confirmation carries the exit code and the agent's
            // exit summary, and anchors the waiting duration (FR-015a, FR-021a). The
            // summary also rides on the notification, so the operator can read what is
            // blocked on them without opening the session first (FR-021).
            let outcome = Outcome {
                reason: None,
                exit_code: Some(0),
                exit_summary: self.exit_summary(id),
            };
            note = outcome.exit_summary.clone();
            // One atomic write: status + outcome + the waiting anchor together, so a reader
            // never sees AwaitingConfirmation without its `waiting_since` (P4).
            self.store.set_session_state(
                id,
                next,
                ended,
                Some(&outcome),
                None,
                Some(clock::now()),
            )?;
        } else {
            // `set_session_state` normalizes the waiting columns and `terminal_outcome` from
            // the target status: exiting a waiting state clears its prompt/anchor (S2) and a
            // live transition (e.g. Stalled → Running on Progress) clears the stale outcome
            // (P6).
            let outcome = note.as_deref().map(Outcome::reason);
            self.store
                .set_session_state(id, next, ended, outcome.as_ref(), None, None)?;
        }
        self.record_lifecycle(id, next, note);
        Ok(next)
    }

    /// The agent's exit summary: the last non-empty line of its captured output, if any.
    fn exit_summary(&self, id: SessionId) -> Option<String> {
        let tail = self.store.output_tail(id, 4096);
        tail.lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    }

    /// Move a running session into `WaitingForInput`, capturing the agent's pending
    /// question and the waiting anchor (FR-015b).
    pub fn mark_waiting_for_input(
        &self,
        id: SessionId,
        prompt: &str,
    ) -> Result<SessionStatus, CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        let next = transition(session.status, Trigger::InputRequested)?;
        // One atomic write: enter WaitingForInput with its prompt + waiting anchor together.
        self.store
            .set_session_state(id, next, None, None, Some(prompt), Some(clock::now()))?;
        self.record_lifecycle(id, next, Some(prompt.to_string()));
        Ok(next)
    }

    /// Return a `WaitingForInput` session to `Running`, clearing the pending prompt and
    /// waiting anchor (the answer was delivered, or the agent unblocked itself).
    pub fn clear_waiting_for_input(&self, id: SessionId) -> Result<SessionStatus, CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        let next = transition(session.status, Trigger::InputProvided)?;
        // Back to Running: clears the pending prompt + anchor and any stale outcome (P6).
        self.store
            .set_session_state(id, next, None, None, None, None)?;
        self.record_lifecycle(id, next, None);
        Ok(next)
    }

    /// Observe a chunk of the session's terminal output and, when the tool declares a
    /// [`PromptConvention::PromptPattern`] that matches, enter `WaitingForInput` with the
    /// matched question text (FR-015b). Tools with no declaration never enter the state.
    pub fn observe_output(&self, id: SessionId, text: &str) -> Result<(), CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        if session.status != SessionStatus::Running || !session.accepts_input {
            return Ok(());
        }
        let tool = self.store.get_tool(session.tool_id)?;
        let Some(PromptConvention::PromptPattern(pattern)) = tool.capabilities.prompt_convention
        else {
            return Ok(());
        };
        // Compile once per tool (patterns are validated at registration and tool records
        // are immutable) — the output path only ever matches the cached regex.
        let question = {
            let mut cache = self.prompt_patterns.lock().expect("poisoned");
            let re = match cache.entry(tool.id) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(v) => {
                    let Ok(re) = regex::Regex::new(&pattern) else {
                        return Ok(()); // validated at registration; never fail the output path
                    };
                    v.insert(re)
                }
            };
            re.captures(text).map(|caps| {
                caps.get(1)
                    .or_else(|| caps.get(0))
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default()
            })
        };
        if let Some(question) = question {
            self.mark_waiting_for_input(id, &question)?;
        }
        Ok(())
    }

    /// Consume the in-Workshop SDK's waiting signal for a tool declared with
    /// [`PromptConvention::SdkSignal`] (FR-015b): `Some` enters `WaitingForInput` with the
    /// pending question; `None` clears it (the agent unblocked itself).
    pub fn apply_sdk_waiting(
        &self,
        id: SessionId,
        signal: Option<WaitingState>,
    ) -> Result<SessionStatus, CoreError> {
        let session = self.store.get_session(id).map_err(map_not_found)?;
        let tool = self.store.get_tool(session.tool_id)?;
        if !session.accepts_input
            || tool.capabilities.prompt_convention != Some(PromptConvention::SdkSignal)
        {
            return Ok(session.status);
        }
        match signal {
            Some(waiting) if session.status == SessionStatus::Running => {
                self.mark_waiting_for_input(id, &waiting.question)
            }
            None if session.status == SessionStatus::WaitingForInput => {
                self.clear_waiting_for_input(id)
            }
            _ => Ok(session.status),
        }
    }
}
