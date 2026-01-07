//! Refactor tool - Code refactoring operations

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use regex::Regex;

/// Refactor operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefactorOperation {
    Rename {
        old_name: String,
        new_name: String,
        scope: RefactorScope,
    },
    Extract {
        code: String,
        name: String,
        extract_type: ExtractType,
    },
    Inline {
        name: String,
    },
    MoveToFile {
        symbol: String,
        target_file: String,
    },
    ChangeSignature {
        function_name: String,
        new_signature: String,
    },
    AddParameter {
        function_name: String,
        param_name: String,
        param_type: String,
        default_value: Option<String>,
    },
    RemoveParameter {
        function_name: String,
        param_name: String,
    },
}

/// Scope for refactoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefactorScope {
    File(String),
    Directory(String),
    Project,
}

/// Type of extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtractType {
    Function,
    Method,
    Variable,
    Constant,
    Class,
    Module,
}

/// Refactor arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactorArgs {
    pub operation: RefactorOperation,
    pub path: String,
    pub dry_run: Option<bool>,
}

/// Change made during refactoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactorChange {
    pub file: String,
    pub line: usize,
    pub old_text: String,
    pub new_text: String,
}

/// Refactor result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactorResult {
    pub success: bool,
    pub changes: Vec<RefactorChange>,
    pub files_modified: usize,
    pub total_changes: usize,
    pub errors: Vec<String>,
}

/// Refactor tool
#[derive(Debug, Clone)]
pub struct RefactorTool;

impl Default for RefactorTool {
    fn default() -> Self {
        Self::new()
    }
}

impl RefactorTool {
    pub const NAME: &'static str = "refactor_code";

    pub fn new() -> Self {
        Self
    }

    /// Execute a refactoring operation
    pub async fn refactor(&self, args: RefactorArgs) -> Result<RefactorResult, RefactorError> {
        let path = PathBuf::from(&args.path);
        
        if !path.exists() {
            return Err(RefactorError::PathNotFound(args.path));
        }

        match args.operation {
            RefactorOperation::Rename { ref old_name, ref new_name, ref scope } => {
                self.rename_symbol(old_name, new_name, scope, args.dry_run.unwrap_or(false)).await
            }
            RefactorOperation::Extract { ref code, ref name, ref extract_type } => {
                self.extract(code, name, extract_type, &path, args.dry_run.unwrap_or(false)).await
            }
            RefactorOperation::Inline { ref name } => {
                self.inline(name, &path, args.dry_run.unwrap_or(false)).await
            }
            RefactorOperation::MoveToFile { ref symbol, ref target_file } => {
                self.move_to_file(symbol, &path, target_file, args.dry_run.unwrap_or(false)).await
            }
            RefactorOperation::ChangeSignature { ref function_name, ref new_signature } => {
                self.change_signature(function_name, new_signature, &path, args.dry_run.unwrap_or(false)).await
            }
            RefactorOperation::AddParameter { ref function_name, ref param_name, ref param_type, ref default_value } => {
                self.add_parameter(function_name, param_name, param_type, default_value.as_deref(), &path, args.dry_run.unwrap_or(false)).await
            }
            RefactorOperation::RemoveParameter { ref function_name, ref param_name } => {
                self.remove_parameter(function_name, param_name, &path, args.dry_run.unwrap_or(false)).await
            }
        }
    }

    /// Rename a symbol across files
    async fn rename_symbol(
        &self,
        old_name: &str,
        new_name: &str,
        scope: &RefactorScope,
        dry_run: bool,
    ) -> Result<RefactorResult, RefactorError> {
        let mut changes = Vec::new();
        let mut errors = Vec::new();
        let mut files_modified = 0;

        // Build regex pattern for word boundary matching
        let pattern = format!(r"\b{}\b", regex::escape(old_name));
        let regex = Regex::new(&pattern)
            .map_err(|e| RefactorError::InvalidPattern(e.to_string()))?;

        let files = match scope {
            RefactorScope::File(file) => vec![PathBuf::from(file)],
            RefactorScope::Directory(dir) => self.collect_source_files(dir).await?,
            RefactorScope::Project => self.collect_source_files(".").await?,
        };

        for file_path in files {
            match self.rename_in_file(&file_path, &regex, old_name, new_name, dry_run).await {
                Ok(file_changes) => {
                    if !file_changes.is_empty() {
                        files_modified += 1;
                        changes.extend(file_changes);
                    }
                }
                Err(e) => {
                    errors.push(format!("{}: {}", file_path.display(), e));
                }
            }
        }

        let total_changes = changes.len();

        Ok(RefactorResult {
            success: errors.is_empty(),
            changes,
            files_modified,
            total_changes,
            errors,
        })
    }

    async fn rename_in_file(
        &self,
        path: &Path,
        regex: &Regex,
        _old_name: &str,
        new_name: &str,
        dry_run: bool,
    ) -> Result<Vec<RefactorChange>, RefactorError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| RefactorError::IoError(e.to_string()))?;

        let mut changes = Vec::new();
        let mut new_content = String::new();
        let file_path = path.to_string_lossy().to_string();

        for (line_num, line) in content.lines().enumerate() {
            if regex.is_match(line) {
                let new_line = regex.replace_all(line, new_name);
                changes.push(RefactorChange {
                    file: file_path.clone(),
                    line: line_num + 1,
                    old_text: line.to_string(),
                    new_text: new_line.to_string(),
                });
                new_content.push_str(&new_line);
            } else {
                new_content.push_str(line);
            }
            new_content.push('\n');
        }

        if !changes.is_empty() && !dry_run {
            fs::write(path, new_content).await
                .map_err(|e| RefactorError::IoError(e.to_string()))?;
        }

        Ok(changes)
    }

    /// Extract code into a new function/variable
    async fn extract(
        &self,
        code: &str,
        name: &str,
        extract_type: &ExtractType,
        path: &Path,
        dry_run: bool,
    ) -> Result<RefactorResult, RefactorError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| RefactorError::IoError(e.to_string()))?;

        let (extracted, replacement) = match extract_type {
            ExtractType::Function => self.extract_to_function(code, name, path),
            ExtractType::Variable => self.extract_to_variable(code, name, path),
            ExtractType::Constant => self.extract_to_constant(code, name, path),
            _ => return Err(RefactorError::UnsupportedOperation(format!("{:?}", extract_type))),
        }?;

        let new_content = content.replace(code, &replacement);
        let new_content = format!("{}\n{}", extracted, new_content);

        let changes = vec![RefactorChange {
            file: path.to_string_lossy().to_string(),
            line: 1,
            old_text: code.to_string(),
            new_text: replacement.clone(),
        }];

        if !dry_run {
            fs::write(path, new_content).await
                .map_err(|e| RefactorError::IoError(e.to_string()))?;
        }

        Ok(RefactorResult {
            success: true,
            changes,
            files_modified: 1,
            total_changes: 1,
            errors: vec![],
        })
    }

    fn extract_to_function(&self, code: &str, name: &str, path: &Path) -> Result<(String, String), RefactorError> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let (extracted, call) = match ext {
            "rs" => (
                format!("fn {}() {{\n    {}\n}}", name, code.replace('\n', "\n    ")),
                format!("{}()", name),
            ),
            "py" => (
                format!("def {}():\n    {}", name, code.replace('\n', "\n    ")),
                format!("{}()", name),
            ),
            "js" | "ts" => (
                format!("function {}() {{\n    {}\n}}", name, code.replace('\n', "\n    ")),
                format!("{}()", name),
            ),
            _ => return Err(RefactorError::UnsupportedLanguage(ext.to_string())),
        };

        Ok((extracted, call))
    }

    fn extract_to_variable(&self, code: &str, name: &str, path: &Path) -> Result<(String, String), RefactorError> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let (extracted, reference) = match ext {
            "rs" => (
                format!("let {} = {};", name, code),
                name.to_string(),
            ),
            "py" => (
                format!("{} = {}", name, code),
                name.to_string(),
            ),
            "js" | "ts" => (
                format!("const {} = {};", name, code),
                name.to_string(),
            ),
            _ => return Err(RefactorError::UnsupportedLanguage(ext.to_string())),
        };

        Ok((extracted, reference))
    }

    fn extract_to_constant(&self, code: &str, name: &str, path: &Path) -> Result<(String, String), RefactorError> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let upper_name = name.to_uppercase();
        
        let (extracted, reference) = match ext {
            "rs" => (
                format!("const {}: _ = {};", upper_name, code),
                upper_name.clone(),
            ),
            "py" => (
                format!("{} = {}", upper_name, code),
                upper_name.clone(),
            ),
            "js" | "ts" => (
                format!("const {} = {};", upper_name, code),
                upper_name.clone(),
            ),
            _ => return Err(RefactorError::UnsupportedLanguage(ext.to_string())),
        };

        Ok((extracted, reference))
    }

    /// Inline a function/variable
    async fn inline(
        &self,
        name: &str,
        path: &Path,
        dry_run: bool,
    ) -> Result<RefactorResult, RefactorError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| RefactorError::IoError(e.to_string()))?;

        // Find the definition
        let definition = self.find_definition(&content, name)?;
        
        // Replace all usages with the definition body
        let pattern = format!(r"\b{}\b", regex::escape(name));
        let regex = Regex::new(&pattern)
            .map_err(|e| RefactorError::InvalidPattern(e.to_string()))?;

        let new_content = regex.replace_all(&content, &definition);

        let changes = vec![RefactorChange {
            file: path.to_string_lossy().to_string(),
            line: 0,
            old_text: name.to_string(),
            new_text: definition.clone(),
        }];

        if !dry_run {
            fs::write(path, new_content.to_string()).await
                .map_err(|e| RefactorError::IoError(e.to_string()))?;
        }

        Ok(RefactorResult {
            success: true,
            changes,
            files_modified: 1,
            total_changes: 1,
            errors: vec![],
        })
    }

    fn find_definition(&self, content: &str, name: &str) -> Result<String, RefactorError> {
        // Simple pattern matching for variable definitions
        for line in content.lines() {
            let trimmed = line.trim();
            
            // Rust: let name = value;
            if trimmed.starts_with("let ") || trimmed.starts_with("const ") {
                if let Some(eq_pos) = trimmed.find('=') {
                    let var_part = &trimmed[..eq_pos];
                    if var_part.contains(name) {
                        let value_part = trimmed[eq_pos + 1..].trim().trim_end_matches(';');
                        return Ok(value_part.to_string());
                    }
                }
            }
            
            // Python: name = value
            if let Some(eq_pos) = trimmed.find('=') {
                let var_part = trimmed[..eq_pos].trim();
                if var_part == name {
                    let value_part = trimmed[eq_pos + 1..].trim();
                    return Ok(value_part.to_string());
                }
            }
        }

        Err(RefactorError::DefinitionNotFound(name.to_string()))
    }

    /// Move symbol to another file
    async fn move_to_file(
        &self,
        symbol: &str,
        source_path: &Path,
        target_file: &str,
        dry_run: bool,
    ) -> Result<RefactorResult, RefactorError> {
        let source_content = fs::read_to_string(source_path).await
            .map_err(|e| RefactorError::IoError(e.to_string()))?;

        // Find the symbol definition
        let (definition, start_line, end_line) = self.find_symbol_bounds(&source_content, symbol)?;

        // Remove from source
        let source_lines: Vec<&str> = source_content.lines().collect();
        let new_source: String = source_lines.iter()
            .enumerate()
            .filter(|(i, _)| *i < start_line || *i >= end_line)
            .map(|(_, l)| *l)
            .collect::<Vec<_>>()
            .join("\n");

        // Add to target
        let target_path = PathBuf::from(target_file);
        let target_content = if target_path.exists() {
            fs::read_to_string(&target_path).await
                .map_err(|e| RefactorError::IoError(e.to_string()))?
        } else {
            String::new()
        };

        let new_target = format!("{}\n{}", target_content, definition);

        let changes = vec![
            RefactorChange {
                file: source_path.to_string_lossy().to_string(),
                line: start_line + 1,
                old_text: definition.clone(),
                new_text: String::new(),
            },
            RefactorChange {
                file: target_file.to_string(),
                line: 0,
                old_text: String::new(),
                new_text: definition.clone(),
            },
        ];

        if !dry_run {
            fs::write(source_path, new_source).await
                .map_err(|e| RefactorError::IoError(e.to_string()))?;
            fs::write(&target_path, new_target).await
                .map_err(|e| RefactorError::IoError(e.to_string()))?;
        }

        Ok(RefactorResult {
            success: true,
            changes,
            files_modified: 2,
            total_changes: 2,
            errors: vec![],
        })
    }

    fn find_symbol_bounds(&self, content: &str, symbol: &str) -> Result<(String, usize, usize), RefactorError> {
        let lines: Vec<&str> = content.lines().collect();
        let mut start_line = None;
        let mut brace_count = 0;
        
        for (i, line) in lines.iter().enumerate() {
            if start_line.is_none() {
                // Look for function/struct/class definition
                if line.contains(&format!("fn {}", symbol)) ||
                   line.contains(&format!("struct {}", symbol)) ||
                   line.contains(&format!("class {}", symbol)) ||
                   line.contains(&format!("def {}", symbol)) ||
                   line.contains(&format!("function {}", symbol)) {
                    start_line = Some(i);
                    brace_count = line.matches('{').count() as i32 - line.matches('}').count() as i32;
                    
                    // Single line definition
                    if brace_count == 0 && line.contains('}') {
                        let definition = lines[i].to_string();
                        return Ok((definition, i, i + 1));
                    }
                }
            } else {
                brace_count += line.matches('{').count() as i32;
                brace_count -= line.matches('}').count() as i32;
                
                if brace_count <= 0 {
                    let start = start_line.unwrap();
                    let definition = lines[start..=i].join("\n");
                    return Ok((definition, start, i + 1));
                }
            }
        }

        Err(RefactorError::DefinitionNotFound(symbol.to_string()))
    }

    /// Change function signature
    async fn change_signature(
        &self,
        function_name: &str,
        new_signature: &str,
        path: &Path,
        dry_run: bool,
    ) -> Result<RefactorResult, RefactorError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| RefactorError::IoError(e.to_string()))?;

        let mut changes = Vec::new();
        let mut new_lines = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if line.contains(&format!("fn {}", function_name)) ||
               line.contains(&format!("def {}", function_name)) ||
               line.contains(&format!("function {}", function_name)) {
                changes.push(RefactorChange {
                    file: path.to_string_lossy().to_string(),
                    line: i + 1,
                    old_text: line.to_string(),
                    new_text: new_signature.to_string(),
                });
                new_lines.push(new_signature.to_string());
            } else {
                new_lines.push(line.to_string());
            }
        }

        let total_changes = changes.len();
        let files_modified = if total_changes == 0 { 0 } else { 1 };

        if !dry_run && total_changes > 0 {
            fs::write(path, new_lines.join("\n")).await
                .map_err(|e| RefactorError::IoError(e.to_string()))?;
        }

        Ok(RefactorResult {
            success: true,
            changes,
            files_modified,
            total_changes,
            errors: vec![],
        })
    }

    /// Add parameter to function
    async fn add_parameter(
        &self,
        function_name: &str,
        param_name: &str,
        param_type: &str,
        default_value: Option<&str>,
        path: &Path,
        dry_run: bool,
    ) -> Result<RefactorResult, RefactorError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| RefactorError::IoError(e.to_string()))?;

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let new_param = match ext {
            "rs" => format!("{}: {}", param_name, param_type),
            "py" => {
                if let Some(default) = default_value {
                    format!("{}: {} = {}", param_name, param_type, default)
                } else {
                    format!("{}: {}", param_name, param_type)
                }
            }
            "ts" => format!("{}: {}", param_name, param_type),
            "js" => param_name.to_string(),
            _ => format!("{}: {}", param_name, param_type),
        };

        let mut changes = Vec::new();
        let mut new_lines = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if line.contains(&format!("fn {}(", function_name)) ||
                line.contains(&format!("def {}(", function_name)) ||
                line.contains(&format!("function {}(", function_name)) {
                
                // Find the closing paren
                if let Some(paren_pos) = line.rfind(')') {
                    let mut new_line = line.to_string();
                    
                    // Check if params exist
                    if let Some(open_paren) = line.find('(') {
                        let params = &line[open_paren + 1..paren_pos];
                        if params.trim().is_empty() {
                            new_line = format!("{}{}{}",
                                &line[..open_paren + 1],
                                new_param,
                                &line[paren_pos..]);
                        } else {
                            new_line = format!("{}, {}{}",
                                &line[..paren_pos],
                                new_param,
                                &line[paren_pos..]);
                        }
                    }
                    
                    changes.push(RefactorChange {
                        file: path.to_string_lossy().to_string(),
                        line: i + 1,
                        old_text: line.to_string(),
                        new_text: new_line.clone(),
                    });
                    new_lines.push(new_line);
                } else {
                    new_lines.push(line.to_string());
                }
            } else {
                new_lines.push(line.to_string());
            }
        }

        let total_changes = changes.len();
        let files_modified = if total_changes == 0 { 0 } else { 1 };

        if !dry_run && total_changes > 0 {
            fs::write(path, new_lines.join("\n")).await
                .map_err(|e| RefactorError::IoError(e.to_string()))?;
        }

        Ok(RefactorResult {
            success: true,
            changes,
            files_modified,
            total_changes,
            errors: vec![],
        })
    }

    /// Remove parameter from function
    async fn remove_parameter(
        &self,
        function_name: &str,
        param_name: &str,
        path: &Path,
        dry_run: bool,
    ) -> Result<RefactorResult, RefactorError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| RefactorError::IoError(e.to_string()))?;

        let mut changes = Vec::new();
        let mut new_lines = Vec::new();

        for (i, line) in content.lines().enumerate() {
            if (line.contains(&format!("fn {}(", function_name)) ||
                line.contains(&format!("def {}(", function_name)) ||
                line.contains(&format!("function {}(", function_name))) &&
               line.contains(param_name) {
                
                // Remove the parameter
                let pattern = format!(r",?\s*{}[^,)]*,?", regex::escape(param_name));
                let regex = Regex::new(&pattern).unwrap();
                let new_line = regex.replace(line, "").to_string();
                
                // Clean up double commas or leading/trailing commas in params
                let new_line = new_line
                    .replace("(,", "(")
                    .replace(",)", ")")
                    .replace(",,", ",");
                
                changes.push(RefactorChange {
                    file: path.to_string_lossy().to_string(),
                    line: i + 1,
                    old_text: line.to_string(),
                    new_text: new_line.clone(),
                });
                new_lines.push(new_line);
            } else {
                new_lines.push(line.to_string());
            }
        }

        let total_changes = changes.len();
        let files_modified = if total_changes == 0 { 0 } else { 1 };

        if !dry_run && total_changes > 0 {
            fs::write(path, new_lines.join("\n")).await
                .map_err(|e| RefactorError::IoError(e.to_string()))?;
        }

        Ok(RefactorResult {
            success: true,
            changes,
            files_modified,
            total_changes,
            errors: vec![],
        })
    }

    /// Collect source files from directory
    async fn collect_source_files(&self, dir: &str) -> Result<Vec<PathBuf>, RefactorError> {
        let mut files = Vec::new();
        let extensions = ["rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "cpp", "c", "h"];
        
        self.collect_files_recursive(Path::new(dir), &extensions, &mut files).await?;
        
        Ok(files)
    }

    async fn collect_files_recursive(
        &self,
        path: &Path,
        extensions: &[&str],
        files: &mut Vec<PathBuf>,
    ) -> Result<(), RefactorError> {
        let mut read_dir = fs::read_dir(path).await
            .map_err(|e| RefactorError::IoError(e.to_string()))?;

        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| RefactorError::IoError(e.to_string()))?
        {
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            // Skip hidden and common non-source dirs
            if file_name.starts_with('.') ||
               file_name == "node_modules" ||
               file_name == "target" ||
               file_name == "__pycache__" {
                continue;
            }
            
            if entry_path.is_dir() {
                Box::pin(self.collect_files_recursive(&entry_path, extensions, files)).await?;
            } else if let Some(ext) = entry_path.extension() {
                if extensions.contains(&ext.to_str().unwrap_or("")) {
                    files.push(entry_path);
                }
            }
        }

        Ok(())
    }
}

/// Refactor errors
#[derive(Debug, thiserror::Error)]
pub enum RefactorError {
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
    #[error("Definition not found: {0}")]
    DefinitionNotFound(String),
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_type() {
        let tool = RefactorTool::new();
        let path = Path::new("test.rs");
        let (extracted, call) = tool.extract_to_function("x + y", "add", path).unwrap();
        assert!(extracted.contains("fn add()"));
        assert_eq!(call, "add()");
    }
}
