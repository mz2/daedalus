//! The agentic-tool registry: declarative, operator-registered tool definitions (FR-001a).

use daedalus_proto::{AgenticTool, ToolDef, ToolId};

use crate::{Core, CoreError};

impl Core {
    /// Register a declaratively-defined agentic tool. Fails with a stated reason if the name
    /// is empty/duplicate or the invocation is ill-formed (data-model validation).
    pub fn register_tool(&self, def: ToolDef) -> Result<ToolId, CoreError> {
        if def.name.trim().is_empty() {
            return Err(CoreError::InvalidTool("name must be non-empty".into()));
        }
        if def.invocation.program.trim().is_empty() {
            return Err(CoreError::InvalidTool(
                "invocation program must be non-empty".into(),
            ));
        }
        if self.store.tool_name_exists(&def.name)? {
            return Err(CoreError::DuplicateTool(def.name));
        }
        let tool = AgenticTool {
            id: ToolId::new(),
            name: def.name,
            invocation: def.invocation,
            capabilities: def.capabilities,
        };
        self.store.upsert_tool(&tool)?;
        Ok(tool.id)
    }

    /// All registered tools.
    pub fn list_tools(&self) -> Result<Vec<AgenticTool>, CoreError> {
        Ok(self.store.list_tools()?)
    }
}
