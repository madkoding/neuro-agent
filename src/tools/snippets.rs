//! Snippet manager tool - Code snippets and templates

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

/// Code snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSnippet {
    pub id: String,
    pub name: String,
    pub description: String,
    pub language: String,
    pub code: String,
    pub placeholders: Vec<Placeholder>,
    pub tags: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Placeholder in snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placeholder {
    pub name: String,
    pub default: Option<String>,
    pub description: Option<String>,
}

/// Snippet collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetCollection {
    pub name: String,
    pub snippets: Vec<CodeSnippet>,
}

/// Snippet manager tool
#[derive(Debug, Clone)]
pub struct SnippetTool {
    snippets: HashMap<String, CodeSnippet>,
    storage_path: PathBuf,
}

impl SnippetTool {
    pub const NAME: &'static str = "snippets";

    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            snippets: HashMap::new(),
            storage_path,
        }
    }

    /// Initialize with default snippets
    pub fn with_defaults() -> Self {
        let mut tool = Self::new(PathBuf::from(".neuro/snippets.json"));
        tool.load_builtin_snippets();
        tool
    }

    fn load_builtin_snippets(&mut self) {
        // Rust snippets
        self.add_snippet(CodeSnippet {
            id: "rust-fn".to_string(),
            name: "Function".to_string(),
            description: "Rust function template".to_string(),
            language: "rust".to_string(),
            code: r#"/// ${description}
pub fn ${name}(${params}) -> ${return_type} {
    ${body}
}"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "description".to_string(),
                    default: Some("TODO".to_string()),
                    description: Some("Function description".to_string()),
                },
                Placeholder {
                    name: "name".to_string(),
                    default: Some("my_function".to_string()),
                    description: Some("Function name".to_string()),
                },
                Placeholder {
                    name: "params".to_string(),
                    default: Some("".to_string()),
                    description: Some("Parameters".to_string()),
                },
                Placeholder {
                    name: "return_type".to_string(),
                    default: Some("()".to_string()),
                    description: Some("Return type".to_string()),
                },
                Placeholder {
                    name: "body".to_string(),
                    default: Some("todo!()".to_string()),
                    description: Some("Function body".to_string()),
                },
            ],
            tags: vec!["rust".to_string(), "function".to_string()],
            created_at: 0,
            updated_at: 0,
        });

        self.add_snippet(CodeSnippet {
            id: "rust-struct".to_string(),
            name: "Struct".to_string(),
            description: "Rust struct with common derives".to_string(),
            language: "rust".to_string(),
            code: r#"/// ${description}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ${name} {
    ${fields}
}"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "description".to_string(),
                    default: Some("TODO".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "name".to_string(),
                    default: Some("MyStruct".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "fields".to_string(),
                    default: Some("pub field: String,".to_string()),
                    description: None,
                },
            ],
            tags: vec!["rust".to_string(), "struct".to_string()],
            created_at: 0,
            updated_at: 0,
        });

        self.add_snippet(CodeSnippet {
            id: "rust-impl".to_string(),
            name: "Impl block".to_string(),
            description: "Rust impl block".to_string(),
            language: "rust".to_string(),
            code: r#"impl ${type_name} {
    pub fn new(${params}) -> Self {
        Self {
            ${init}
        }
    }
}"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "type_name".to_string(),
                    default: Some("MyType".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "params".to_string(),
                    default: Some("".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "init".to_string(),
                    default: Some("".to_string()),
                    description: None,
                },
            ],
            tags: vec!["rust".to_string(), "impl".to_string()],
            created_at: 0,
            updated_at: 0,
        });

        self.add_snippet(CodeSnippet {
            id: "rust-test".to_string(),
            name: "Test function".to_string(),
            description: "Rust test function".to_string(),
            language: "rust".to_string(),
            code: r#"#[test]
fn test_${name}() {
    ${body}
    assert!(${assertion});
}"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "name".to_string(),
                    default: Some("something".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "body".to_string(),
                    default: Some("".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "assertion".to_string(),
                    default: Some("true".to_string()),
                    description: None,
                },
            ],
            tags: vec!["rust".to_string(), "test".to_string()],
            created_at: 0,
            updated_at: 0,
        });

        self.add_snippet(CodeSnippet {
            id: "rust-error".to_string(),
            name: "Error enum".to_string(),
            description: "Rust error enum with thiserror".to_string(),
            language: "rust".to_string(),
            code: r#"#[derive(Debug, thiserror::Error)]
pub enum ${name}Error {
    #[error("${error_msg}")]
    ${variant}(${inner}),
}"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "name".to_string(),
                    default: Some("My".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "error_msg".to_string(),
                    default: Some("An error occurred".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "variant".to_string(),
                    default: Some("Generic".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "inner".to_string(),
                    default: Some("String".to_string()),
                    description: None,
                },
            ],
            tags: vec!["rust".to_string(), "error".to_string()],
            created_at: 0,
            updated_at: 0,
        });

        // Python snippets
        self.add_snippet(CodeSnippet {
            id: "python-class".to_string(),
            name: "Class".to_string(),
            description: "Python class with init".to_string(),
            language: "python".to_string(),
            code: r#"class ${name}:
    """${description}"""
    
    def __init__(self${params}):
        ${init}
    
    def __repr__(self):
        return f"${name}(${repr})"
"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "name".to_string(),
                    default: Some("MyClass".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "description".to_string(),
                    default: Some("TODO".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "params".to_string(),
                    default: Some("".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "init".to_string(),
                    default: Some("pass".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "repr".to_string(),
                    default: Some("".to_string()),
                    description: None,
                },
            ],
            tags: vec!["python".to_string(), "class".to_string()],
            created_at: 0,
            updated_at: 0,
        });

        self.add_snippet(CodeSnippet {
            id: "python-async".to_string(),
            name: "Async function".to_string(),
            description: "Python async function".to_string(),
            language: "python".to_string(),
            code: r#"async def ${name}(${params}) -> ${return_type}:
    """${description}"""
    ${body}
"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "name".to_string(),
                    default: Some("my_async_func".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "params".to_string(),
                    default: Some("".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "return_type".to_string(),
                    default: Some("None".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "description".to_string(),
                    default: Some("TODO".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "body".to_string(),
                    default: Some("pass".to_string()),
                    description: None,
                },
            ],
            tags: vec!["python".to_string(), "async".to_string()],
            created_at: 0,
            updated_at: 0,
        });

        // TypeScript snippets
        self.add_snippet(CodeSnippet {
            id: "ts-interface".to_string(),
            name: "Interface".to_string(),
            description: "TypeScript interface".to_string(),
            language: "typescript".to_string(),
            code: r#"/**
 * ${description}
 */
export interface ${name} {
    ${fields}
}
"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "description".to_string(),
                    default: Some("TODO".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "name".to_string(),
                    default: Some("MyInterface".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "fields".to_string(),
                    default: Some("field: string;".to_string()),
                    description: None,
                },
            ],
            tags: vec!["typescript".to_string(), "interface".to_string()],
            created_at: 0,
            updated_at: 0,
        });

        self.add_snippet(CodeSnippet {
            id: "ts-async".to_string(),
            name: "Async function".to_string(),
            description: "TypeScript async function".to_string(),
            language: "typescript".to_string(),
            code: r#"/**
 * ${description}
 */
export async function ${name}(${params}): Promise<${return_type}> {
    ${body}
}
"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "description".to_string(),
                    default: Some("TODO".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "name".to_string(),
                    default: Some("myAsyncFunc".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "params".to_string(),
                    default: Some("".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "return_type".to_string(),
                    default: Some("void".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "body".to_string(),
                    default: Some("throw new Error('Not implemented');".to_string()),
                    description: None,
                },
            ],
            tags: vec![
                "typescript".to_string(),
                "async".to_string(),
                "function".to_string(),
            ],
            created_at: 0,
            updated_at: 0,
        });

        self.add_snippet(CodeSnippet {
            id: "ts-react-component".to_string(),
            name: "React Component".to_string(),
            description: "TypeScript React functional component".to_string(),
            language: "typescript".to_string(),
            code: r#"import React from 'react';

interface ${name}Props {
    ${props}
}

export const ${name}: React.FC<${name}Props> = ({ ${destructured} }) => {
    return (
        <div>
            ${content}
        </div>
    );
};
"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "name".to_string(),
                    default: Some("MyComponent".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "props".to_string(),
                    default: Some("children?: React.ReactNode;".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "destructured".to_string(),
                    default: Some("children".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "content".to_string(),
                    default: Some("{children}".to_string()),
                    description: None,
                },
            ],
            tags: vec![
                "typescript".to_string(),
                "react".to_string(),
                "component".to_string(),
            ],
            created_at: 0,
            updated_at: 0,
        });

        // JavaScript snippets
        self.add_snippet(CodeSnippet {
            id: "js-express-route".to_string(),
            name: "Express Route".to_string(),
            description: "Express.js route handler".to_string(),
            language: "javascript".to_string(),
            code: r#"router.${method}('${path}', async (req, res) => {
    try {
        ${body}
        res.json({ success: true });
    } catch (error) {
        res.status(500).json({ error: error.message });
    }
});
"#
            .to_string(),
            placeholders: vec![
                Placeholder {
                    name: "method".to_string(),
                    default: Some("get".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "path".to_string(),
                    default: Some("/api/resource".to_string()),
                    description: None,
                },
                Placeholder {
                    name: "body".to_string(),
                    default: Some("// TODO: implement".to_string()),
                    description: None,
                },
            ],
            tags: vec![
                "javascript".to_string(),
                "express".to_string(),
                "api".to_string(),
            ],
            created_at: 0,
            updated_at: 0,
        });
    }

    /// Add a new snippet
    pub fn add_snippet(&mut self, snippet: CodeSnippet) {
        self.snippets.insert(snippet.id.clone(), snippet);
    }

    /// Get snippet by ID
    pub fn get_snippet(&self, id: &str) -> Option<&CodeSnippet> {
        self.snippets.get(id)
    }

    /// List all snippets
    pub fn list_snippets(&self) -> Vec<&CodeSnippet> {
        self.snippets.values().collect()
    }

    /// Search snippets by query
    pub fn search(&self, query: &str) -> Vec<&CodeSnippet> {
        let query_lower = query.to_lowercase();
        self.snippets
            .values()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.description.to_lowercase().contains(&query_lower)
                    || s.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Get snippets by language
    pub fn by_language(&self, language: &str) -> Vec<&CodeSnippet> {
        self.snippets
            .values()
            .filter(|s| s.language.eq_ignore_ascii_case(language))
            .collect()
    }

    /// Get snippets by tag
    pub fn by_tag(&self, tag: &str) -> Vec<&CodeSnippet> {
        let tag_lower = tag.to_lowercase();
        self.snippets
            .values()
            .filter(|s| s.tags.iter().any(|t| t.to_lowercase() == tag_lower))
            .collect()
    }

    /// Expand snippet with values
    pub fn expand(&self, id: &str, values: &HashMap<String, String>) -> Option<String> {
        let snippet = self.snippets.get(id)?;
        let mut result = snippet.code.clone();

        // Replace placeholders with values or defaults
        for placeholder in &snippet.placeholders {
            let value = values
                .get(&placeholder.name)
                .or(placeholder.default.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("");

            result = result.replace(&format!("${{{}}}", placeholder.name), value);
        }

        Some(result)
    }

    /// Delete snippet
    pub fn delete_snippet(&mut self, id: &str) -> Option<CodeSnippet> {
        self.snippets.remove(id)
    }

    /// Save snippets to file
    pub async fn save(&self) -> Result<(), SnippetError> {
        let collection = SnippetCollection {
            name: "user".to_string(),
            snippets: self.snippets.values().cloned().collect(),
        };

        let json = serde_json::to_string_pretty(&collection)
            .map_err(|e| SnippetError::SerializeError(e.to_string()))?;

        // Ensure directory exists
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| SnippetError::IoError(e.to_string()))?;
        }

        fs::write(&self.storage_path, json)
            .await
            .map_err(|e| SnippetError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Load snippets from file
    pub async fn load(&mut self) -> Result<(), SnippetError> {
        if !self.storage_path.exists() {
            return Ok(());
        }

        let json = fs::read_to_string(&self.storage_path)
            .await
            .map_err(|e| SnippetError::IoError(e.to_string()))?;

        let collection: SnippetCollection = serde_json::from_str(&json)
            .map_err(|e| SnippetError::DeserializeError(e.to_string()))?;

        for snippet in collection.snippets {
            self.snippets.insert(snippet.id.clone(), snippet);
        }

        Ok(())
    }

    /// Import snippets from VS Code format
    pub fn import_vscode(&mut self, json: &str) -> Result<usize, SnippetError> {
        let vscode_snippets: HashMap<String, VsCodeSnippet> = serde_json::from_str(json)
            .map_err(|e| SnippetError::DeserializeError(e.to_string()))?;

        let mut count = 0;
        for (name, vs_snippet) in vscode_snippets {
            let code = match &vs_snippet.body {
                VsCodeBody::String(s) => s.clone(),
                VsCodeBody::Array(arr) => arr.join("\n"),
            };

            let snippet = CodeSnippet {
                id: name.to_lowercase().replace(' ', "-"),
                name: name.clone(),
                description: vs_snippet.description.unwrap_or_default(),
                language: "unknown".to_string(),
                code,
                placeholders: vec![],
                tags: vec![],
                created_at: 0,
                updated_at: 0,
            };

            self.add_snippet(snippet);
            count += 1;
        }

        Ok(count)
    }
}

/// VS Code snippet format
#[derive(Debug, Deserialize)]
struct VsCodeSnippet {
    body: VsCodeBody,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum VsCodeBody {
    String(String),
    Array(Vec<String>),
}

/// Snippet errors
#[derive(Debug, thiserror::Error)]
pub enum SnippetError {
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Serialize error: {0}")]
    SerializeError(String),
    #[error("Deserialize error: {0}")]
    DeserializeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snippet_expansion() {
        let tool = SnippetTool::with_defaults();

        let mut values = HashMap::new();
        values.insert("name".to_string(), "calculate_sum".to_string());
        values.insert("params".to_string(), "a: i32, b: i32".to_string());
        values.insert("return_type".to_string(), "i32".to_string());
        values.insert("body".to_string(), "a + b".to_string());
        values.insert(
            "description".to_string(),
            "Calculate sum of two numbers".to_string(),
        );

        let expanded = tool.expand("rust-fn", &values).unwrap();
        assert!(expanded.contains("fn calculate_sum"));
        assert!(expanded.contains("a: i32, b: i32"));
        assert!(expanded.contains("-> i32"));
    }

    #[test]
    fn test_search() {
        let tool = SnippetTool::with_defaults();

        let results = tool.search("rust");
        assert!(!results.is_empty());

        let results = tool.by_language("rust");
        assert!(!results.is_empty());
    }
}
