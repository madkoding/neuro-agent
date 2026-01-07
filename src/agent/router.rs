//! Intelligent Router - Intent detection and tool selection

use regex::Regex;
use serde_json::{json, Value};

pub struct IntelligentRouter;

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    ReadCode,      // "muéstrame X"
    SearchCode,    // "encuentra Y"
    AnalyzeCode,   // "analiza Z"
    BrowseFiles,   // "lista archivos"
    Explain,       // "qué hace X"
    ModifyCode,    // "refactoriza X"
    RunTests,      // "ejecuta tests"
    GitOps,        // "git status"
    Chat,          // fallback
}

pub struct ExecutionPlan {
    pub steps: Vec<ExecutionStep>,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub enum ExecutionStep {
    ToolCall {
        tool_name: String,
        args: Value,
    },
    Reasoning {
        prompt: String,
    },
}

impl Clone for IntelligentRouter {
    fn clone(&self) -> Self {
        Self
    }
}

impl IntelligentRouter {
    pub fn new() -> Self {
        Self
    }

    /// Detect intent from user query
    pub fn detect_intent(&self, query: &str) -> Intent {
        let lower = query.to_lowercase();

        // Keywords para cada intent (español e inglés)
        if self.matches_keywords(&lower, &["busca", "encuentra", "localiza", "dónde", "donde", "search", "find", "locate", "where"]) {
            return Intent::SearchCode;
        }

        if self.matches_keywords(&lower, &["lee", "muestra", "abre", "cat", "ver", "read", "show", "open", "display", "contenido"]) {
            return Intent::ReadCode;
        }

        if self.matches_keywords(&lower, &["analiza", "revisa", "examina", "check", "analyze", "review", "examine", "inspect"]) {
            return Intent::AnalyzeCode;
        }

        if self.matches_keywords(&lower, &["lista", "ls", "archivos", "directorio", "list", "files", "directory", "carpeta", "folder"]) {
            return Intent::BrowseFiles;
        }

        if self.matches_keywords(&lower, &["qué hace", "cómo funciona", "explica", "por qué", "what does", "how does", "explain", "why"]) {
            return Intent::Explain;
        }

        if self.matches_keywords(&lower, &["refactoriza", "mejora", "cambia", "modifica", "edita", "refactor", "improve", "change", "modify", "edit", "update", "actualiza"]) {
            return Intent::ModifyCode;
        }

        if self.matches_keywords(&lower, &["test", "prueba", "ejecuta test", "tests", "testing", "run test"]) {
            return Intent::RunTests;
        }

        if self.matches_keywords(&lower, &["git", "commit", "diff", "status", "branch", "push", "pull", "merge"]) {
            return Intent::GitOps;
        }

        // Check for file operations
        if self.matches_keywords(&lower, &["crea", "escribe", "genera", "create", "write", "generate", "nuevo", "new"]) {
            return Intent::ModifyCode;
        }

        // Check for shell/command execution
        if self.matches_keywords(&lower, &["ejecuta", "corre", "run", "execute", "comando", "command", "shell", "terminal"]) {
            return Intent::ModifyCode;
        }

        Intent::Chat
    }

    /// Build execution plan based on intent
    pub fn build_plan(&self, intent: Intent, query: &str) -> ExecutionPlan {
        match intent {
            Intent::SearchCode => ExecutionPlan {
                steps: vec![ExecutionStep::ToolCall {
                    tool_name: "semantic_search".to_string(),
                    args: json!({
                        "query": query,
                        "project_id": ".",
                        "limit": 5
                    }),
                }],
                confidence: 0.9,
            },

            Intent::ReadCode => {
                if let Some(path) = self.extract_file_path(query) {
                    ExecutionPlan {
                        steps: vec![ExecutionStep::ToolCall {
                            tool_name: "read_file".to_string(),
                            args: json!({"path": path}),
                        }],
                        confidence: 0.95,
                    }
                } else {
                    // Buscar primero, luego leer
                    ExecutionPlan {
                        steps: vec![
                            ExecutionStep::ToolCall {
                                tool_name: "semantic_search".to_string(),
                                args: json!({
                                    "query": query,
                                    "project_id": ".",
                                    "limit": 3
                                }),
                            },
                            ExecutionStep::Reasoning {
                                prompt: "Ahora lee el archivo más relevante".to_string(),
                            },
                        ],
                        confidence: 0.7,
                    }
                }
            }

            Intent::AnalyzeCode => {
                if let Some(path) = self.extract_file_path(query) {
                    ExecutionPlan {
                        steps: vec![ExecutionStep::ToolCall {
                            tool_name: "analyze_code".to_string(),
                            args: json!({"path": path}),
                        }],
                        confidence: 0.9,
                    }
                } else {
                    ExecutionPlan {
                        steps: vec![ExecutionStep::ToolCall {
                            tool_name: "project_context".to_string(),
                            args: json!({"path": "."}),
                        }],
                        confidence: 0.8,
                    }
                }
            }

            Intent::BrowseFiles => ExecutionPlan {
                steps: vec![ExecutionStep::ToolCall {
                    tool_name: "list_directory".to_string(),
                    args: json!({"path": ".", "recursive": false}),
                }],
                confidence: 0.9,
            },

            Intent::Explain => {
                // Multi-step: buscar + analizar + explicar
                ExecutionPlan {
                    steps: vec![
                        ExecutionStep::ToolCall {
                            tool_name: "semantic_search".to_string(),
                            args: json!({
                                "query": query,
                                "project_id": ".",
                                "limit": 3
                            }),
                        },
                        ExecutionStep::Reasoning {
                            prompt: format!("Explica en detalle: {}", query),
                        },
                    ],
                    confidence: 0.7,
                }
            }

            _ => ExecutionPlan {
                steps: vec![ExecutionStep::Reasoning {
                    prompt: query.to_string(),
                }],
                confidence: 0.5,
            },
        }
    }

    fn matches_keywords(&self, text: &str, keywords: &[&str]) -> bool {
        keywords.iter().any(|k| text.contains(k))
    }

    fn extract_file_path(&self, query: &str) -> Option<String> {
        // Regex para encontrar rutas de archivo
        let patterns = vec![
            r"[\w/]+\.(rs|py|ts|js|go|java|cpp|c|h)",  // extension-based
            r"src/[\w/]+",                               // src/ paths
            r"[\w/]+/[\w/]+",                            // generic paths
        ];

        for pattern in patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(query) {
                    return Some(m.as_str().to_string());
                }
            }
        }

        None
    }
}

impl Default for IntelligentRouter {
    fn default() -> Self {
        Self::new()
    }
}
