//! Documentation generator tool

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;
use std::process::Stdio;

/// Documentation format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DocFormat {
    Markdown,
    Html,
    Json,
    Rst,
}

impl Default for DocFormat {
    fn default() -> Self {
        Self::Markdown
    }
}

/// Function documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDoc {
    pub name: String,
    pub signature: String,
    pub description: String,
    pub params: Vec<ParamDoc>,
    pub returns: Option<String>,
    pub examples: Vec<String>,
    pub raises: Vec<String>,
}

/// Parameter documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDoc {
    pub name: String,
    pub type_: Option<String>,
    pub description: String,
    pub default: Option<String>,
    pub required: bool,
}

/// Module documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDoc {
    pub name: String,
    pub path: String,
    pub description: String,
    pub functions: Vec<FunctionDoc>,
    pub classes: Vec<ClassDoc>,
    pub constants: Vec<ConstantDoc>,
}

/// Class documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassDoc {
    pub name: String,
    pub description: String,
    pub methods: Vec<FunctionDoc>,
    pub properties: Vec<ParamDoc>,
    pub parent: Option<String>,
}

/// Constant documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstantDoc {
    pub name: String,
    pub type_: Option<String>,
    pub value: String,
    pub description: String,
}

/// Documentation generator arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocGenArgs {
    pub path: String,
    pub output: Option<String>,
    pub format: Option<DocFormat>,
    pub include_private: Option<bool>,
    pub include_tests: Option<bool>,
}

/// Documentation generator output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocOutput {
    pub path: String,
    pub modules: Vec<ModuleDoc>,
    pub total_functions: usize,
    pub total_classes: usize,
    pub coverage: f32,
}

/// Documentation generator tool
#[derive(Debug, Clone)]
pub struct DocumentationTool;

impl Default for DocumentationTool {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentationTool {
    pub const NAME: &'static str = "generate_documentation";

    pub fn new() -> Self {
        Self
    }

    /// Generate documentation for a project
    pub async fn generate(&self, args: DocGenArgs) -> Result<DocOutput, DocError> {
        let path = PathBuf::from(&args.path);
        
        if !path.exists() {
            return Err(DocError::PathNotFound(args.path));
        }

        // Detect project type and use appropriate doc generator
        if path.join("Cargo.toml").exists() {
            self.generate_rust_docs(&path, &args).await
        } else if path.join("package.json").exists() {
            self.generate_js_docs(&path, &args).await
        } else if path.join("pyproject.toml").exists() || path.join("setup.py").exists() {
            self.generate_python_docs(&path, &args).await
        } else {
            // Generic documentation extraction
            self.generate_generic_docs(&path, &args).await
        }
    }

    async fn generate_rust_docs(&self, path: &Path, args: &DocGenArgs) -> Result<DocOutput, DocError> {
        // Try to run cargo doc
        let mut cmd = Command::new("cargo");
        cmd.arg("doc")
            .arg("--no-deps")
            .current_dir(path);

        if args.include_private.unwrap_or(false) {
            cmd.arg("--document-private-items");
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let _ = cmd.output().await
            .map_err(|e| DocError::GenerationError(e.to_string()))?;

        // Parse Rust source files for documentation
        let modules = self.parse_rust_modules(path).await?;

        let total_functions: usize = modules.iter()
            .map(|m| m.functions.len())
            .sum();
        let total_classes: usize = modules.iter()
            .map(|m| m.classes.len())
            .sum();

        // Calculate documentation coverage
        let documented: usize = modules.iter()
            .map(|m| {
                m.functions.iter().filter(|f| !f.description.is_empty()).count() +
                m.classes.iter().filter(|c| !c.description.is_empty()).count()
            })
            .sum();
        let total = total_functions + total_classes;
        let coverage = if total > 0 { documented as f32 / total as f32 * 100.0 } else { 100.0 };

        Ok(DocOutput {
            path: path.to_string_lossy().to_string(),
            modules,
            total_functions,
            total_classes,
            coverage,
        })
    }

    async fn parse_rust_modules(&self, path: &Path) -> Result<Vec<ModuleDoc>, DocError> {
        let mut modules = Vec::new();
        let src_path = path.join("src");
        
        if src_path.exists() {
            self.scan_rust_dir(&src_path, &mut modules).await?;
        }

        Ok(modules)
    }

    async fn scan_rust_dir(&self, path: &Path, modules: &mut Vec<ModuleDoc>) -> Result<(), DocError> {
        let mut read_dir = fs::read_dir(path).await
            .map_err(|e| DocError::IoError(e.to_string()))?;

        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| DocError::IoError(e.to_string()))?
        {
            let entry_path = entry.path();
            
            if entry_path.is_dir() {
                Box::pin(self.scan_rust_dir(&entry_path, modules)).await?;
            } else if entry_path.extension().map(|e| e == "rs").unwrap_or(false) {
                if let Ok(module) = self.parse_rust_file(&entry_path).await {
                    modules.push(module);
                }
            }
        }

        Ok(())
    }

    async fn parse_rust_file(&self, path: &Path) -> Result<ModuleDoc, DocError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| DocError::IoError(e.to_string()))?;

        let name = path.file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut current_doc = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();

            // Collect doc comments
            if trimmed.starts_with("///") {
                let doc_line = trimmed.trim_start_matches("///").trim();
                current_doc.push(doc_line.to_string());
            } else if trimmed.starts_with("//!") {
                // Module-level doc, skip for now
            } else if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ") {
                // Function
                let is_pub = trimmed.starts_with("pub ");
                if is_pub || true { // Include all for now
                    let signature = extract_rust_signature(trimmed);
                    let fn_name = extract_rust_fn_name(trimmed);
                    
                    functions.push(FunctionDoc {
                        name: fn_name,
                        signature,
                        description: current_doc.join(" "),
                        params: vec![],
                        returns: None,
                        examples: vec![],
                        raises: vec![],
                    });
                }
                current_doc.clear();
            } else if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
                let struct_name = extract_rust_struct_name(trimmed);
                classes.push(ClassDoc {
                    name: struct_name,
                    description: current_doc.join(" "),
                    methods: vec![],
                    properties: vec![],
                    parent: None,
                });
                current_doc.clear();
            } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
                current_doc.clear();
            }
        }

        Ok(ModuleDoc {
            name,
            path: path.to_string_lossy().to_string(),
            description: String::new(),
            functions,
            classes,
            constants: vec![],
        })
    }

    async fn generate_js_docs(&self, path: &Path, _args: &DocGenArgs) -> Result<DocOutput, DocError> {
        // Try to use JSDoc or TypeDoc
        let modules = self.parse_js_modules(path).await?;

        let total_functions: usize = modules.iter()
            .map(|m| m.functions.len())
            .sum();
        let total_classes: usize = modules.iter()
            .map(|m| m.classes.len())
            .sum();

        Ok(DocOutput {
            path: path.to_string_lossy().to_string(),
            modules,
            total_functions,
            total_classes,
            coverage: 0.0,
        })
    }

    async fn parse_js_modules(&self, path: &Path) -> Result<Vec<ModuleDoc>, DocError> {
        let mut modules = Vec::new();
        
        // Scan for .js, .ts, .jsx, .tsx files
        let extensions = ["js", "ts", "jsx", "tsx"];
        
        for ext in &extensions {
            self.scan_js_dir(path, &mut modules, ext).await?;
        }

        Ok(modules)
    }

    async fn scan_js_dir(&self, path: &Path, modules: &mut Vec<ModuleDoc>, ext: &str) -> Result<(), DocError> {
        let mut read_dir = fs::read_dir(path).await
            .map_err(|e| DocError::IoError(e.to_string()))?;

        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| DocError::IoError(e.to_string()))?
        {
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            // Skip node_modules
            if file_name == "node_modules" {
                continue;
            }
            
            if entry_path.is_dir() {
                Box::pin(self.scan_js_dir(&entry_path, modules, ext)).await?;
            } else if entry_path.extension().map(|e| e == ext).unwrap_or(false) {
                if let Ok(module) = self.parse_js_file(&entry_path).await {
                    modules.push(module);
                }
            }
        }

        Ok(())
    }

    async fn parse_js_file(&self, path: &Path) -> Result<ModuleDoc, DocError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| DocError::IoError(e.to_string()))?;

        let name = path.file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut current_doc = Vec::new();
        let mut in_jsdoc = false;

        for line in content.lines() {
            let trimmed = line.trim();

            // JSDoc comment handling
            if trimmed.starts_with("/**") {
                in_jsdoc = true;
                current_doc.clear();
            } else if trimmed.contains("*/") && in_jsdoc {
                in_jsdoc = false;
            } else if in_jsdoc {
                let doc_line = trimmed.trim_start_matches('*').trim();
                if !doc_line.starts_with('@') {
                    current_doc.push(doc_line.to_string());
                }
            }

            // Function detection
            if trimmed.starts_with("function ") || 
               trimmed.starts_with("export function ") ||
               trimmed.starts_with("async function ") ||
               trimmed.starts_with("export async function ") ||
               trimmed.contains("=> {") {
                let fn_name = extract_js_fn_name(trimmed);
                if !fn_name.is_empty() {
                    functions.push(FunctionDoc {
                        name: fn_name,
                        signature: trimmed.to_string(),
                        description: current_doc.join(" "),
                        params: vec![],
                        returns: None,
                        examples: vec![],
                        raises: vec![],
                    });
                }
                current_doc.clear();
            }

            // Class detection
            if trimmed.starts_with("class ") || trimmed.starts_with("export class ") {
                let class_name = extract_js_class_name(trimmed);
                classes.push(ClassDoc {
                    name: class_name,
                    description: current_doc.join(" "),
                    methods: vec![],
                    properties: vec![],
                    parent: None,
                });
                current_doc.clear();
            }
        }

        Ok(ModuleDoc {
            name,
            path: path.to_string_lossy().to_string(),
            description: String::new(),
            functions,
            classes,
            constants: vec![],
        })
    }

    async fn generate_python_docs(&self, path: &Path, _args: &DocGenArgs) -> Result<DocOutput, DocError> {
        let modules = self.parse_python_modules(path).await?;

        let total_functions: usize = modules.iter()
            .map(|m| m.functions.len())
            .sum();
        let total_classes: usize = modules.iter()
            .map(|m| m.classes.len())
            .sum();

        Ok(DocOutput {
            path: path.to_string_lossy().to_string(),
            modules,
            total_functions,
            total_classes,
            coverage: 0.0,
        })
    }

    async fn parse_python_modules(&self, path: &Path) -> Result<Vec<ModuleDoc>, DocError> {
        let mut modules = Vec::new();
        self.scan_python_dir(path, &mut modules).await?;
        Ok(modules)
    }

    async fn scan_python_dir(&self, path: &Path, modules: &mut Vec<ModuleDoc>) -> Result<(), DocError> {
        let mut read_dir = fs::read_dir(path).await
            .map_err(|e| DocError::IoError(e.to_string()))?;

        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| DocError::IoError(e.to_string()))?
        {
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            // Skip common non-source dirs
            if file_name.starts_with('.') || 
               file_name == "__pycache__" || 
               file_name == "venv" ||
               file_name == ".venv" {
                continue;
            }
            
            if entry_path.is_dir() {
                Box::pin(self.scan_python_dir(&entry_path, modules)).await?;
            } else if entry_path.extension().map(|e| e == "py").unwrap_or(false) {
                if let Ok(module) = self.parse_python_file(&entry_path).await {
                    modules.push(module);
                }
            }
        }

        Ok(())
    }

    async fn parse_python_file(&self, path: &Path) -> Result<ModuleDoc, DocError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| DocError::IoError(e.to_string()))?;

        let name = path.file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut functions = Vec::new();
        let mut classes = Vec::new();
        let mut module_doc = String::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        // Check for module docstring
        while i < lines.len() {
            let line = lines[i].trim();
            if line.starts_with("\"\"\"") || line.starts_with("'''") {
                let quote = if line.starts_with("\"\"\"") { "\"\"\"" } else { "'''" };
                if line.len() > 6 && line.ends_with(quote) {
                    module_doc = line[3..line.len()-3].to_string();
                } else {
                    let mut doc_lines = vec![line[3..].to_string()];
                    i += 1;
                    while i < lines.len() && !lines[i].contains(quote) {
                        doc_lines.push(lines[i].to_string());
                        i += 1;
                    }
                    module_doc = doc_lines.join("\n");
                }
                break;
            } else if !line.is_empty() && !line.starts_with('#') {
                break;
            }
            i += 1;
        }

        // Parse functions and classes
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            if trimmed.starts_with("def ") {
                let fn_name = extract_python_fn_name(trimmed);
                let docstring = extract_python_docstring(&lines, idx);
                
                functions.push(FunctionDoc {
                    name: fn_name,
                    signature: trimmed.to_string(),
                    description: docstring,
                    params: vec![],
                    returns: None,
                    examples: vec![],
                    raises: vec![],
                });
            } else if trimmed.starts_with("class ") {
                let class_name = extract_python_class_name(trimmed);
                let docstring = extract_python_docstring(&lines, idx);
                
                classes.push(ClassDoc {
                    name: class_name,
                    description: docstring,
                    methods: vec![],
                    properties: vec![],
                    parent: None,
                });
            }
        }

        Ok(ModuleDoc {
            name,
            path: path.to_string_lossy().to_string(),
            description: module_doc,
            functions,
            classes,
            constants: vec![],
        })
    }

    async fn generate_generic_docs(&self, path: &Path, _args: &DocGenArgs) -> Result<DocOutput, DocError> {
        Ok(DocOutput {
            path: path.to_string_lossy().to_string(),
            modules: vec![],
            total_functions: 0,
            total_classes: 0,
            coverage: 0.0,
        })
    }

    /// Generate README.md content
    pub fn generate_readme(&self, project: &ProjectInfo) -> String {
        let mut readme = String::new();
        
        readme.push_str(&format!("# {}\n\n", project.name));
        
        if let Some(ref desc) = project.description {
            readme.push_str(&format!("{}\n\n", desc));
        }
        
        readme.push_str("## Installation\n\n");
        readme.push_str("```bash\n");
        readme.push_str(&format!("# Clone the repository\n"));
        readme.push_str(&format!("git clone {}\n", project.repository.as_deref().unwrap_or("https://github.com/user/repo")));
        readme.push_str("cd ");
        readme.push_str(&project.name);
        readme.push_str("\n```\n\n");
        
        readme.push_str("## Usage\n\n");
        readme.push_str("TODO: Add usage instructions\n\n");
        
        readme.push_str("## License\n\n");
        readme.push_str(&format!("{}\n", project.license.as_deref().unwrap_or("MIT")));
        
        readme
    }
}

/// Project information for README generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub author: Option<String>,
}

// Helper functions
fn extract_rust_signature(line: &str) -> String {
    // Extract full function signature
    let start = line.find("fn ").map(|i| i).unwrap_or(0);
    if let Some(end) = line.find('{') {
        line[start..end].trim().to_string()
    } else {
        line[start..].trim().to_string()
    }
}

fn extract_rust_fn_name(line: &str) -> String {
    if let Some(start) = line.find("fn ") {
        let rest = &line[start + 3..];
        if let Some(end) = rest.find('(') {
            return rest[..end].trim().to_string();
        }
    }
    String::new()
}

fn extract_rust_struct_name(line: &str) -> String {
    let line = line.trim_start_matches("pub ");
    if let Some(start) = line.find("struct ") {
        let rest = &line[start + 7..];
        let end = rest.find(|c: char| c == ' ' || c == '{' || c == '(' || c == '<')
            .unwrap_or(rest.len());
        return rest[..end].trim().to_string();
    }
    String::new()
}

fn extract_js_fn_name(line: &str) -> String {
    let line = line.replace("export ", "").replace("async ", "");
    if let Some(start) = line.find("function ") {
        let rest = &line[start + 9..];
        if let Some(end) = rest.find('(') {
            return rest[..end].trim().to_string();
        }
    }
    // Arrow function: const name = () =>
    if let Some(eq) = line.find('=') {
        let before = line[..eq].trim();
        let name = before.split_whitespace().last().unwrap_or("");
        if line.contains("=>") {
            return name.to_string();
        }
    }
    String::new()
}

fn extract_js_class_name(line: &str) -> String {
    let line = line.replace("export ", "");
    if let Some(start) = line.find("class ") {
        let rest = &line[start + 6..];
        let end = rest.find(|c: char| c == ' ' || c == '{' || c == '<')
            .unwrap_or(rest.len());
        return rest[..end].trim().to_string();
    }
    String::new()
}

fn extract_python_fn_name(line: &str) -> String {
    if let Some(start) = line.find("def ") {
        let rest = &line[start + 4..];
        if let Some(end) = rest.find('(') {
            return rest[..end].trim().to_string();
        }
    }
    String::new()
}

fn extract_python_class_name(line: &str) -> String {
    if let Some(start) = line.find("class ") {
        let rest = &line[start + 6..];
        let end = rest.find(|c: char| c == '(' || c == ':')
            .unwrap_or(rest.len());
        return rest[..end].trim().to_string();
    }
    String::new()
}

fn extract_python_docstring(lines: &[&str], start_idx: usize) -> String {
    // Look for docstring on next line after def/class
    if start_idx + 1 >= lines.len() {
        return String::new();
    }

    let next_line = lines[start_idx + 1].trim();
    if next_line.starts_with("\"\"\"") || next_line.starts_with("'''") {
        let quote = if next_line.starts_with("\"\"\"") { "\"\"\"" } else { "'''" };
        
        // Single line docstring
        if next_line.len() > 6 && next_line.ends_with(quote) {
            return next_line[3..next_line.len()-3].to_string();
        }
        
        // Multi-line docstring
        let mut doc_lines = vec![next_line[3..].to_string()];
        let mut i = start_idx + 2;
        while i < lines.len() {
            let line = lines[i];
            if line.contains(quote) {
                let end_idx = line.find(quote).unwrap_or(line.len());
                doc_lines.push(line[..end_idx].trim().to_string());
                break;
            }
            doc_lines.push(line.trim().to_string());
            i += 1;
        }
        return doc_lines.join(" ").trim().to_string();
    }

    String::new()
}

/// Documentation errors
#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Generation error: {0}")]
    GenerationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_fn_extraction() {
        assert_eq!(extract_rust_fn_name("pub fn test_function(arg: i32) -> i32 {"), "test_function");
        assert_eq!(extract_rust_fn_name("fn another() {"), "another");
    }

    #[test]
    fn test_python_fn_extraction() {
        assert_eq!(extract_python_fn_name("def my_function(self, arg):"), "my_function");
        assert_eq!(extract_python_fn_name("    def inner():"), "inner");
    }
}
