//! MCP Server - Model Context Protocol integration

use anyhow::Result;

pub struct NeuroMcpServer {
    // Placeholder for MCP server implementation
}

impl NeuroMcpServer {
    pub fn new() -> Self {
        Self {}
    }

    /// Start the MCP server (stdio transport)
    pub async fn start(&self) -> Result<()> {
        // TODO: Implement MCP server
        // This is optional and can be implemented later
        // Requires mcp-sdk dependency
        eprintln!("MCP Server implementation pending");
        Ok(())
    }

    /// Expose tools as MCP resources
    pub fn expose_tools(&self) -> Vec<String> {
        vec![
            "semantic_code_search".to_string(),
            "analyze_code_file".to_string(),
            "get_project_context".to_string(),
        ]
    }
}

impl Default for NeuroMcpServer {
    fn default() -> Self {
        Self::new()
    }
}
