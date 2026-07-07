//! The surface-agnostic app service: a thin facade over [`daedalus_core::Core`] that
//! executes [`Command`]s, answers [`AppQuery`] reads, and exposes the [`AppEvent`] stream.
//! A future web surface reuses this unchanged (FR-007a).

pub mod api;

use std::sync::Arc;

use tokio::sync::broadcast;

pub use api::{AppEvent, AppQuery, AppQueryAsync, Command, CommandResult};

use daedalus_core::{Core, CoreError};
use daedalus_proto::{
    AggregateTask, AttentionItem, BackendStatus, DiscoveredSession, EventRecord, MetricKind,
    ResourceUsageMetric, SessionDetail, SessionId, SessionSummary, TrackedTask,
};

/// The app service. Holds an [`Arc<Core>`]; cloning is cheap and share-safe.
#[derive(Clone)]
pub struct App {
    core: Arc<Core>,
}

impl App {
    /// Wrap a core.
    #[must_use]
    pub fn new(core: Arc<Core>) -> Self {
        Self { core }
    }

    /// Access the underlying core (e.g. for the terminal attach path).
    #[must_use]
    pub fn core(&self) -> &Arc<Core> {
        &self.core
    }

    /// Subscribe to the push event stream.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.core.subscribe()
    }

    /// Execute an operator command (FR-006). Every capability is reachable here (C-A5).
    pub async fn execute(&self, command: Command) -> Result<CommandResult, CoreError> {
        match command {
            Command::RegisterTool(def) => {
                Ok(CommandResult::ToolRegistered(self.core.register_tool(def)?))
            }
            Command::StartSession(req) => Ok(CommandResult::SessionStarted(
                self.core.start_session(req).await?,
            )),
            Command::StopSession(id) => {
                self.core.stop_session(id).await?;
                Ok(CommandResult::Done)
            }
            Command::SendInput { session, data } => {
                self.core.send_input(session, data).await?;
                Ok(CommandResult::Done)
            }
            Command::ConfirmCompletion(id) => {
                self.core.confirm_completion(id).await?;
                Ok(CommandResult::Done)
            }
            Command::CleanUp(id) => {
                self.core.clean_up(id).await?;
                Ok(CommandResult::Done)
            }
            Command::DeleteRecord(id) => {
                self.core.delete_record(id).await?;
                Ok(CommandResult::Done)
            }
            Command::ConnectDiscovered(id) => Ok(CommandResult::Connected(Box::new(
                self.core.connect_discovered(id).await?,
            ))),
            Command::SetIdleRate { backend, rate } => {
                self.core.set_idle_rate(backend, rate)?;
                Ok(CommandResult::Done)
            }
            Command::SetConcurrencyLimit(limit) => {
                self.core.set_concurrency_limit(limit)?;
                Ok(CommandResult::Done)
            }
            Command::SetStallInterval(secs) => {
                self.core.set_stall_interval(secs)?;
                Ok(CommandResult::Done)
            }
        }
    }
}

impl AppQuery for App {
    fn fleet(&self) -> Vec<SessionSummary> {
        self.core.fleet().unwrap_or_default()
    }

    fn session(&self, id: SessionId) -> Option<SessionDetail> {
        self.core.session_detail(id).ok()
    }

    fn task_board(&self, id: SessionId) -> Vec<TrackedTask> {
        self.core.task_board(id).unwrap_or_default()
    }

    fn all_tasks(&self) -> Vec<AggregateTask> {
        self.core.all_tasks().unwrap_or_default()
    }

    fn resource_usage(&self, id: SessionId) -> Vec<ResourceUsageMetric> {
        self.core.resource_usage(id).unwrap_or_default()
    }

    fn resource_history(
        &self,
        id: SessionId,
        metric: MetricKind,
        limit: usize,
    ) -> Vec<ResourceUsageMetric> {
        self.core
            .resource_history(id, metric, limit)
            .unwrap_or_default()
    }

    fn session_events(&self, id: SessionId) -> Vec<EventRecord> {
        self.core.session_events(id).unwrap_or_default()
    }

    fn needs_you(&self) -> Vec<AttentionItem> {
        self.core.needs_you().unwrap_or_default()
    }
}

impl AppQueryAsync for App {
    async fn discovered(&self) -> Vec<DiscoveredSession> {
        self.core.discovered().await
    }

    async fn backends(&self) -> Vec<BackendStatus> {
        self.core.backends_status().await
    }
}
