//! MCP Server - Model Context Protocol integration
//!
//! Provides a basic MCP server implementation that exposes Neuro's capabilities
//! to MCP clients like Claude Desktop, Cody, and other compatible tools.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

/// MCP Protocol version
const MCP_VERSION: &str = "2024-11-05";

/// Tool definition for MCP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// MCP Request
#[derive(Debug, Deserialize)]
struct McpRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

/// MCP Response
#[derive(Debug, Serialize)]
struct McpResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, Serialize)]
struct McpError {
    code: i32,
    message: String,
}

pub struct NeuroMcpServer {
    tools: Vec<McpTool>,
    initialized: Arc<Mutex<bool>>,
}

impl NeuroMcpServer {
    pub fn new() -> Self {
        Self {
            tools: Self::define_tools(),
            initialized: Arc::new(Mutex::new(false)),
        }
    }

    /// Define available MCP tools
    fn define_tools() -> Vec<McpTool> {
        vec![
            McpTool {
                name: "semantic_code_search".to_string(),
                description: "Search codebase using semantic similarity".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query"
                        },
                        "project_id": {
                            "type": "string",
                            "description": "Project identifier"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum results",
                            "default": 5
                        }
                    },
                    "required": ["query", "project_id"]
                }),
            },
            McpTool {
                name: "analyze_code_file".to_string(),
                description: "Analyze a code file and extract symbols".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to analyze"
                        },
                        "language": {
                            "type": "string",
                            "description": "Programming language",
                            "enum": ["rust", "python", "typescript", "javascript"]
                        }
                    },
                    "required": ["file_path", "language"]
                }),
            },
            McpTool {
                name: "get_project_context".to_string(),
                description: "Get high-level project context and statistics".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "project_id": {
                            "type": "string",
                            "description": "Project identifier"
                        }
                    },
                    "required": ["project_id"]
                }),
            },
        ]
    }

    /// Start the MCP server with stdio transport
    pub async fn start(&self) -> Result<()> {
        eprintln!("Starting Neuro MCP Server v{}", MCP_VERSION);

        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let reader = stdin.lock();

        for line in reader.lines() {
            let line = line.context("Failed to read line from stdin")?;
            
            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<McpRequest>(&line) {
                Ok(request) => {
                    let response = self.handle_request(request).await;
                    let response_json = serde_json::to_string(&response)?;
                    writeln!(stdout, "{}", response_json)?;
                    stdout.flush()?;
                }
                Err(e) => {
                    eprintln!("Failed to parse request: {}", e);
                    let error_response = McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: None,
                        result: None,
                        error: Some(McpError {
                            code: -32700,
                            message: format!("Parse error: {}", e),
                        }),
                    };
                    let response_json = serde_json::to_string(&error_response)?;
                    writeln!(stdout, "{}", response_json)?;
                    stdout.flush()?;
                }
            }
        }

        Ok(())
    }

    /// Handle MCP request
    async fn handle_request(&self, request: McpRequest) -> McpResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request).await,
            "tools/list" => self.handle_tools_list(request),
            "tools/call" => self.handle_tools_call(request).await,
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(McpError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                }),
            },
        }
    }

    /// Handle initialize request
    async fn handle_initialize(&self, request: McpRequest) -> McpResponse {
        let mut initialized = self.initialized.lock().await;
        *initialized = true;

        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "protocolVersion": MCP_VERSION,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "neuro-agent",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
            error: None,
        }
    }

    /// Handle tools/list request
    fn handle_tools_list(&self, request: McpRequest) -> McpResponse {
        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "tools": self.tools
            })),
            error: None,
        }
    }

    /// Handle tools/call request
    async fn handle_tools_call(&self, request: McpRequest) -> McpResponse {
        let params = match request.params {
            Some(p) => p,
            None => {
                return McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(McpError {
                        code: -32602,
                        message: "Missing params".to_string(),
                    }),
                }
            }
        };

        // In a real implementation, this would call the actual tool implementations
        // For now, return a placeholder response
        McpResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Tool execution placeholder: {:?}", params)
                }]
            })),
            error: None,
        }
    }

    /// Expose tools as list (for compatibility)
    pub fn expose_tools(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name.clone()).collect()
    }
}

impl Default for NeuroMcpServer {
    fn default() -> Self {
        Self::new()
    }
}
