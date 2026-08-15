use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult};

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.definition().name;
        self.tools.insert(name, tool);
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn execute(
        &self,
        name: &str,
        context: &ToolContext,
        input: Value,
    ) -> Result<ToolResult, ToolError> {
        let Some(tool) = self.get(name) else {
            return Err(ToolError::Failed(format!("unknown tool: {name}")));
        };
        tool.execute(context, input)
    }

    pub fn approval_prompt(
        &self,
        name: &str,
        context: &ToolContext,
        input: &Value,
    ) -> (String, String) {
        self.get(name)
            .map(|tool| tool.approval_prompt(context, input))
            .unwrap_or_else(|| (format!("Run `{name}`"), String::new()))
    }
}
