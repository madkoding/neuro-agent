//! Code analyzer - Analyze code structure and complexity

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Code analysis output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAnalysis {
    pub file: PathBuf,
    pub language: String,
    pub metrics: CodeMetrics,
    pub symbols: Vec<CodeSymbol>,
    pub imports: Vec<ImportInfo>,
    pub issues: Vec<CodeIssue>,
}

/// Code metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeMetrics {
    pub total_lines: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
    pub functions: usize,
    pub classes: usize,
    pub complexity: usize,
    pub max_nesting: usize,
    pub avg_function_length: f32,
}

/// Code symbol (function, class, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSymbol {
    pub name: String,
    pub symbol_type: SymbolType,
    pub line_start: usize,
    pub line_end: usize,
    pub visibility: Visibility,
    pub params: Vec<String>,
    pub return_type: Option<String>,
    pub complexity: usize,
}

/// Symbol type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SymbolType {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Constant,
    Variable,
    Module,
    Type,
}

/// Visibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
}

/// Import info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportInfo {
    pub module: String,
    pub items: Vec<String>,
    pub line: usize,
    pub is_external: bool,
}

/// Code issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeIssue {
    pub severity: IssueSeverity,
    pub message: String,
    pub line: Option<usize>,
    pub rule: String,
}

/// Issue severity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Code analyzer tool
#[derive(Debug, Clone, Default)]
pub struct CodeAnalyzerTool;

impl CodeAnalyzerTool {
    pub const NAME: &'static str = "analyze_code";

    pub fn new() -> Self {
        Self
    }

    /// Analyze a single file
    pub async fn analyze_file(&self, args: AnalyzeFileArgs) -> Result<CodeAnalysis, AnalyzerError> {
        let path = PathBuf::from(&args.path);

        if !path.exists() {
            return Err(AnalyzerError::FileNotFound(args.path));
        }

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| AnalyzerError::IoError(e.to_string()))?;

        let language = detect_language(&path);
        let metrics = calculate_metrics(&content, &language);
        let symbols = extract_symbols(&content, &language);
        let imports = extract_imports(&content, &language);
        let issues = check_issues(&content, &language, &symbols);

        Ok(CodeAnalysis {
            file: path,
            language,
            metrics,
            symbols,
            imports,
            issues,
        })
    }

    /// Analyze a function or class
    pub async fn analyze_symbol(
        &self,
        args: AnalyzeSymbolArgs,
    ) -> Result<SymbolAnalysis, AnalyzerError> {
        let path = PathBuf::from(&args.path);
        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| AnalyzerError::IoError(e.to_string()))?;

        let language = detect_language(&path);
        let symbols = extract_symbols(&content, &language);

        let symbol = symbols
            .into_iter()
            .find(|s| s.name == args.symbol_name)
            .ok_or_else(|| AnalyzerError::SymbolNotFound(args.symbol_name.clone()))?;

        let lines: Vec<&str> = content.lines().collect();
        let symbol_content = lines
            .get(symbol.line_start.saturating_sub(1)..symbol.line_end)
            .map(|l| l.join("\n"))
            .unwrap_or_default();

        let complexity = calculate_cyclomatic_complexity(&symbol_content);
        let calls = extract_function_calls(&symbol_content, &language);
        let variables = extract_local_variables(&symbol_content, &language);

        Ok(SymbolAnalysis {
            symbol,
            content: symbol_content,
            complexity,
            function_calls: calls,
            local_variables: variables,
        })
    }

    /// Generate documentation for code
    pub async fn generate_docs(&self, args: GenerateDocsArgs) -> Result<String, AnalyzerError> {
        let analysis = self
            .analyze_file(AnalyzeFileArgs {
                path: args.path.clone(),
            })
            .await?;

        let mut docs = String::new();
        docs.push_str(&format!(
            "# {}\n\n",
            analysis
                .file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        docs.push_str(&format!("**Language:** {}\n\n", analysis.language));

        docs.push_str("## Metrics\n\n");
        docs.push_str(&format!(
            "- Total lines: {}\n",
            analysis.metrics.total_lines
        ));
        docs.push_str(&format!("- Code lines: {}\n", analysis.metrics.code_lines));
        docs.push_str(&format!("- Functions: {}\n", analysis.metrics.functions));
        docs.push_str(&format!(
            "- Complexity: {}\n\n",
            analysis.metrics.complexity
        ));

        if !analysis.imports.is_empty() {
            docs.push_str("## Dependencies\n\n");
            for import in &analysis.imports {
                docs.push_str(&format!("- `{}`", import.module));
                if !import.items.is_empty() {
                    docs.push_str(&format!(": {}", import.items.join(", ")));
                }
                docs.push('\n');
            }
            docs.push('\n');
        }

        docs.push_str("## Symbols\n\n");

        // Group by type
        let functions: Vec<_> = analysis
            .symbols
            .iter()
            .filter(|s| {
                s.symbol_type == SymbolType::Function || s.symbol_type == SymbolType::Method
            })
            .collect();

        let types: Vec<_> = analysis
            .symbols
            .iter()
            .filter(|s| {
                matches!(
                    s.symbol_type,
                    SymbolType::Class | SymbolType::Struct | SymbolType::Enum | SymbolType::Trait
                )
            })
            .collect();

        if !types.is_empty() {
            docs.push_str("### Types\n\n");
            for sym in types {
                docs.push_str(&format!("#### `{}`\n", sym.name));
                docs.push_str(&format!("- Type: {:?}\n", sym.symbol_type));
                docs.push_str(&format!("- Lines: {}-{}\n\n", sym.line_start, sym.line_end));
            }
        }

        if !functions.is_empty() {
            docs.push_str("### Functions\n\n");
            for sym in functions {
                docs.push_str(&format!("#### `{}`\n", sym.name));
                if !sym.params.is_empty() {
                    docs.push_str(&format!("- Parameters: {}\n", sym.params.join(", ")));
                }
                if let Some(ref ret) = sym.return_type {
                    docs.push_str(&format!("- Returns: `{}`\n", ret));
                }
                docs.push_str(&format!("- Complexity: {}\n\n", sym.complexity));
            }
        }

        if !analysis.issues.is_empty() {
            docs.push_str("## Issues\n\n");
            for issue in &analysis.issues {
                let icon = match issue.severity {
                    IssueSeverity::Error => "❌",
                    IssueSeverity::Warning => "⚠️",
                    IssueSeverity::Info => "ℹ️",
                    IssueSeverity::Hint => "💡",
                };
                docs.push_str(&format!("{} {}", icon, issue.message));
                if let Some(line) = issue.line {
                    docs.push_str(&format!(" (line {})", line));
                }
                docs.push('\n');
            }
        }

        Ok(docs)
    }
}

/// Symbol analysis output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolAnalysis {
    pub symbol: CodeSymbol,
    pub content: String,
    pub complexity: usize,
    pub function_calls: Vec<String>,
    pub local_variables: Vec<String>,
}

/// Arguments for analyzing a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeFileArgs {
    pub path: String,
}

/// Arguments for analyzing a symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeSymbolArgs {
    pub path: String,
    pub symbol_name: String,
}

/// Arguments for generating docs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateDocsArgs {
    pub path: String,
    pub format: Option<String>,
}

/// Analyzer errors
#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

fn detect_language(path: &Path) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "rs" => "Rust",
        "py" => "Python",
        "js" | "mjs" => "JavaScript",
        "ts" | "mts" => "TypeScript",
        "tsx" | "jsx" => "React",
        "go" => "Go",
        "java" => "Java",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        _ => "Unknown",
    }
    .to_string()
}

fn calculate_metrics(content: &str, language: &str) -> CodeMetrics {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let mut code_lines = 0usize;
    let mut comment_lines = 0usize;
    let mut blank_lines = 0usize;
    let mut max_nesting = 0i32;
    let mut current_nesting = 0i32;
    let mut in_multiline_comment = false;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            blank_lines += 1;
            continue;
        }

        // Check for comments
        let is_comment = match language {
            "Rust" | "JavaScript" | "TypeScript" | "Java" | "C" | "C++" | "Go" | "C#" => {
                if trimmed.starts_with("/*") {
                    in_multiline_comment = true;
                }
                if trimmed.ends_with("*/") {
                    in_multiline_comment = false;
                    true
                } else {
                    in_multiline_comment || trimmed.starts_with("//")
                }
            }
            "Python" | "Ruby" => {
                if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
                    in_multiline_comment = !in_multiline_comment;
                    true
                } else {
                    in_multiline_comment || trimmed.starts_with('#')
                }
            }
            _ => trimmed.starts_with("//") || trimmed.starts_with('#'),
        };

        if is_comment {
            comment_lines += 1;
        } else {
            code_lines += 1;
        }

        // Track nesting
        for c in line.chars() {
            match c {
                '{' | '(' | '[' => current_nesting += 1,
                '}' | ')' | ']' => current_nesting = current_nesting.saturating_sub(1),
                _ => {}
            }
        }
        max_nesting = max_nesting.max(current_nesting);
    }

    let symbols = extract_symbols(content, language);
    let functions = symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Function || s.symbol_type == SymbolType::Method)
        .count();
    let classes = symbols
        .iter()
        .filter(|s| matches!(s.symbol_type, SymbolType::Class | SymbolType::Struct))
        .count();

    let avg_function_length = if functions > 0 {
        let total_func_lines: usize = symbols
            .iter()
            .filter(|s| {
                s.symbol_type == SymbolType::Function || s.symbol_type == SymbolType::Method
            })
            .map(|s| s.line_end - s.line_start + 1)
            .sum();
        total_func_lines as f32 / functions as f32
    } else {
        0.0
    };

    let complexity = calculate_cyclomatic_complexity(content);

    CodeMetrics {
        total_lines,
        code_lines,
        comment_lines,
        blank_lines,
        functions,
        classes,
        complexity,
        max_nesting: max_nesting as usize,
        avg_function_length,
    }
}

fn extract_symbols(content: &str, language: &str) -> Vec<CodeSymbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let line_num = i + 1;

        match language {
            "Rust" => {
                // Functions
                if trimmed.starts_with("pub fn ")
                    || trimmed.starts_with("fn ")
                    || trimmed.starts_with("pub async fn ")
                    || trimmed.starts_with("async fn ")
                {
                    if let Some(sym) = parse_rust_function(trimmed, line_num, &lines) {
                        symbols.push(sym);
                    }
                }
                // Structs
                if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
                    if let Some(sym) = parse_rust_struct(trimmed, line_num, &lines) {
                        symbols.push(sym);
                    }
                }
                // Enums
                if trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ") {
                    if let Some(sym) = parse_rust_enum(trimmed, line_num, &lines) {
                        symbols.push(sym);
                    }
                }
                // Traits
                if trimmed.starts_with("pub trait ") || trimmed.starts_with("trait ") {
                    if let Some(sym) = parse_rust_trait(trimmed, line_num, &lines) {
                        symbols.push(sym);
                    }
                }
                // Impl blocks
                if trimmed.starts_with("impl ") {
                    if let Some(sym) = parse_rust_impl(trimmed, line_num, &lines) {
                        symbols.push(sym);
                    }
                }
            }
            "Python" => {
                if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
                    if let Some(sym) = parse_python_function(trimmed, line_num, &lines) {
                        symbols.push(sym);
                    }
                }
                if trimmed.starts_with("class ") {
                    if let Some(sym) = parse_python_class(trimmed, line_num, &lines) {
                        symbols.push(sym);
                    }
                }
            }
            "JavaScript" | "TypeScript" => {
                if trimmed.starts_with("function ")
                    || trimmed.contains(" function ")
                    || trimmed.starts_with("const ") && trimmed.contains(" = (")
                    || trimmed.starts_with("async function")
                {
                    if let Some(sym) = parse_js_function(trimmed, line_num, &lines) {
                        symbols.push(sym);
                    }
                }
                if trimmed.starts_with("class ") {
                    if let Some(sym) = parse_js_class(trimmed, line_num, &lines) {
                        symbols.push(sym);
                    }
                }
            }
            _ => {}
        }
    }

    symbols
}

fn extract_imports(content: &str, language: &str) -> Vec<ImportInfo> {
    let mut imports = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = i + 1;

        match language {
            "Rust" => {
                if trimmed.starts_with("use ") {
                    let module = trimmed
                        .trim_start_matches("use ")
                        .trim_end_matches(';')
                        .to_string();
                    let is_external = !module.starts_with("crate::")
                        && !module.starts_with("self::")
                        && !module.starts_with("super::");
                    imports.push(ImportInfo {
                        module,
                        items: vec![],
                        line: line_num,
                        is_external,
                    });
                }
            }
            "Python" => {
                if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    let module = parts.get(1).unwrap_or(&"").to_string();
                    let is_external = !module.starts_with('.');
                    imports.push(ImportInfo {
                        module,
                        items: vec![],
                        line: line_num,
                        is_external,
                    });
                }
            }
            "JavaScript" | "TypeScript" => {
                if trimmed.starts_with("import ") || trimmed.contains(" require(") {
                    let is_external = !trimmed.contains("./") && !trimmed.contains("../");
                    let module = extract_js_import_module(trimmed);
                    imports.push(ImportInfo {
                        module,
                        items: vec![],
                        line: line_num,
                        is_external,
                    });
                }
            }
            _ => {}
        }
    }

    imports
}

fn check_issues(content: &str, language: &str, symbols: &[CodeSymbol]) -> Vec<CodeIssue> {
    let mut issues = Vec::new();

    // Check for long functions
    for sym in symbols {
        if sym.symbol_type == SymbolType::Function || sym.symbol_type == SymbolType::Method {
            let length = sym.line_end - sym.line_start;
            if length > 50 {
                issues.push(CodeIssue {
                    severity: IssueSeverity::Warning,
                    message: format!("Function '{}' is too long ({} lines)", sym.name, length),
                    line: Some(sym.line_start),
                    rule: "long-function".to_string(),
                });
            }
            if sym.complexity > 10 {
                issues.push(CodeIssue {
                    severity: IssueSeverity::Warning,
                    message: format!(
                        "Function '{}' has high complexity ({})",
                        sym.name, sym.complexity
                    ),
                    line: Some(sym.line_start),
                    rule: "high-complexity".to_string(),
                });
            }
        }
    }

    // Check for TODO/FIXME comments
    for (i, line) in content.lines().enumerate() {
        if line.contains("TODO") || line.contains("FIXME") || line.contains("HACK") {
            issues.push(CodeIssue {
                severity: IssueSeverity::Info,
                message: format!("Found TODO/FIXME comment: {}", line.trim()),
                line: Some(i + 1),
                rule: "todo-comment".to_string(),
            });
        }
    }

    // Language-specific checks
    match language {
        "Rust" => {
            for (i, line) in content.lines().enumerate() {
                if line.contains("unwrap()") && !line.contains("// ") {
                    issues.push(CodeIssue {
                        severity: IssueSeverity::Hint,
                        message: "Consider handling the error instead of unwrap()".to_string(),
                        line: Some(i + 1),
                        rule: "unwrap-used".to_string(),
                    });
                }
            }
        }
        "JavaScript" | "TypeScript" => {
            for (i, line) in content.lines().enumerate() {
                if line.contains("console.log") {
                    issues.push(CodeIssue {
                        severity: IssueSeverity::Hint,
                        message: "Remove console.log before production".to_string(),
                        line: Some(i + 1),
                        rule: "no-console".to_string(),
                    });
                }
            }
        }
        _ => {}
    }

    issues
}

fn calculate_cyclomatic_complexity(content: &str) -> usize {
    let mut complexity = 1; // Base complexity

    let decision_keywords = [
        "if ", "else if", "elif ", "while ", "for ", "case ", "catch ", "&&", "||", "?", "match ",
        "when ",
    ];

    for line in content.lines() {
        for keyword in &decision_keywords {
            complexity += line.matches(keyword).count();
        }
    }

    complexity
}

fn extract_function_calls(content: &str, _language: &str) -> Vec<String> {
    let mut calls = Vec::new();
    let re = regex::Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();

    for cap in re.captures_iter(content) {
        if let Some(name) = cap.get(1) {
            let name = name.as_str();
            if !["if", "while", "for", "match", "switch", "catch"].contains(&name) {
                calls.push(name.to_string());
            }
        }
    }

    calls.sort();
    calls.dedup();
    calls
}

fn extract_local_variables(content: &str, language: &str) -> Vec<String> {
    let mut vars = Vec::new();

    let re = match language {
        "Rust" => regex::Regex::new(r"let\s+(mut\s+)?([a-zA-Z_][a-zA-Z0-9_]*)").unwrap(),
        "Python" => regex::Regex::new(r"^(\s*)([a-zA-Z_][a-zA-Z0-9_]*)\s*=").unwrap(),
        "JavaScript" | "TypeScript" => {
            regex::Regex::new(r"(const|let|var)\s+([a-zA-Z_][a-zA-Z0-9_]*)").unwrap()
        }
        _ => return vars,
    };

    for cap in re.captures_iter(content) {
        if let Some(name) = cap.get(2) {
            vars.push(name.as_str().to_string());
        }
    }

    vars
}

// Parser helper functions
fn parse_rust_function(line: &str, line_num: usize, lines: &[&str]) -> Option<CodeSymbol> {
    let visibility = if line.starts_with("pub") {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let name_start = line.find("fn ")? + 3;
    let name_end = line[name_start..].find('(')? + name_start;
    let name = line[name_start..name_end].trim().to_string();

    let params_start = name_end + 1;
    let params_end = line[params_start..].find(')')? + params_start;
    let params_str = &line[params_start..params_end];
    let params: Vec<String> = params_str
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    let return_type = if line.contains("->") {
        let ret_start = line.find("->")? + 2;
        let ret_end = line.find('{').unwrap_or(line.len());
        Some(line[ret_start..ret_end].trim().to_string())
    } else {
        None
    };

    let line_end = find_block_end(lines, line_num - 1);
    let block_content = lines[line_num - 1..line_end].join("\n");
    let complexity = calculate_cyclomatic_complexity(&block_content);

    Some(CodeSymbol {
        name,
        symbol_type: SymbolType::Function,
        line_start: line_num,
        line_end,
        visibility,
        params,
        return_type,
        complexity,
    })
}

fn parse_rust_struct(line: &str, line_num: usize, lines: &[&str]) -> Option<CodeSymbol> {
    let visibility = if line.starts_with("pub") {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let name_start = line.find("struct ")? + 7;
    let name_end = line[name_start..]
        .find(|c: char| c == '<' || c == '{' || c == '(' || c == ';' || c.is_whitespace())
        .map(|i| i + name_start)
        .unwrap_or(line.len());
    let name = line[name_start..name_end].trim().to_string();

    let line_end = if line.contains(';') {
        line_num
    } else {
        find_block_end(lines, line_num - 1)
    };

    Some(CodeSymbol {
        name,
        symbol_type: SymbolType::Struct,
        line_start: line_num,
        line_end,
        visibility,
        params: vec![],
        return_type: None,
        complexity: 1,
    })
}

fn parse_rust_enum(line: &str, line_num: usize, lines: &[&str]) -> Option<CodeSymbol> {
    let visibility = if line.starts_with("pub") {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let name_start = line.find("enum ")? + 5;
    let name_end = line[name_start..]
        .find(|c: char| c == '<' || c == '{' || c.is_whitespace())
        .map(|i| i + name_start)
        .unwrap_or(line.len());
    let name = line[name_start..name_end].trim().to_string();

    let line_end = find_block_end(lines, line_num - 1);

    Some(CodeSymbol {
        name,
        symbol_type: SymbolType::Enum,
        line_start: line_num,
        line_end,
        visibility,
        params: vec![],
        return_type: None,
        complexity: 1,
    })
}

fn parse_rust_trait(line: &str, line_num: usize, lines: &[&str]) -> Option<CodeSymbol> {
    let visibility = if line.starts_with("pub") {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let name_start = line.find("trait ")? + 6;
    let name_end = line[name_start..]
        .find(|c: char| c == '<' || c == '{' || c == ':' || c.is_whitespace())
        .map(|i| i + name_start)
        .unwrap_or(line.len());
    let name = line[name_start..name_end].trim().to_string();

    let line_end = find_block_end(lines, line_num - 1);

    Some(CodeSymbol {
        name,
        symbol_type: SymbolType::Trait,
        line_start: line_num,
        line_end,
        visibility,
        params: vec![],
        return_type: None,
        complexity: 1,
    })
}

fn parse_rust_impl(line: &str, line_num: usize, lines: &[&str]) -> Option<CodeSymbol> {
    let name_start = line.find("impl ")? + 5;
    let name = line[name_start..]
        .split_whitespace()
        .next()?
        .trim_end_matches(['<', '{'])
        .to_string();

    let line_end = find_block_end(lines, line_num - 1);

    Some(CodeSymbol {
        name: format!("impl {}", name),
        symbol_type: SymbolType::Module,
        line_start: line_num,
        line_end,
        visibility: Visibility::Public,
        params: vec![],
        return_type: None,
        complexity: 1,
    })
}

fn parse_python_function(line: &str, line_num: usize, lines: &[&str]) -> Option<CodeSymbol> {
    let _is_async = line.starts_with("async ");
    let name_start = line.find("def ")? + 4;

    let name_end = line[name_start..].find('(')? + name_start;
    let name = line[name_start..name_end].trim().to_string();

    let visibility = if name.starts_with('_') && !name.starts_with("__") {
        Visibility::Private
    } else {
        Visibility::Public
    };

    let line_end = find_python_block_end(lines, line_num - 1);
    let block_content = lines[line_num - 1..line_end].join("\n");
    let complexity = calculate_cyclomatic_complexity(&block_content);

    Some(CodeSymbol {
        name,
        symbol_type: SymbolType::Function,
        line_start: line_num,
        line_end,
        visibility,
        params: vec![],
        return_type: None,
        complexity,
    })
}

fn parse_python_class(line: &str, line_num: usize, lines: &[&str]) -> Option<CodeSymbol> {
    let name_start = line.find("class ")? + 6;
    let name_end = line[name_start..]
        .find(['(', ':'])
        .map(|i| i + name_start)
        .unwrap_or(line.len());
    let name = line[name_start..name_end].trim().to_string();

    let line_end = find_python_block_end(lines, line_num - 1);

    Some(CodeSymbol {
        name,
        symbol_type: SymbolType::Class,
        line_start: line_num,
        line_end,
        visibility: Visibility::Public,
        params: vec![],
        return_type: None,
        complexity: 1,
    })
}

fn parse_js_function(line: &str, line_num: usize, lines: &[&str]) -> Option<CodeSymbol> {
    let name = if line.starts_with("function ") || line.starts_with("async function") {
        let start = line.find("function")? + 8;
        let trimmed = line[start..].trim();
        let end = trimmed.find('(')?;
        trimmed[..end].trim().to_string()
    } else if line.starts_with("const ") || line.starts_with("let ") {
        let start = line.find(char::is_alphabetic)?;
        let end = line[start..].find(|c: char| !c.is_alphanumeric() && c != '_')? + start;
        line[start..end].to_string()
    } else {
        return None;
    };

    if name.is_empty() {
        return None;
    }

    let line_end = find_block_end(lines, line_num - 1);
    let block_content = lines[line_num - 1..line_end].join("\n");
    let complexity = calculate_cyclomatic_complexity(&block_content);

    Some(CodeSymbol {
        name,
        symbol_type: SymbolType::Function,
        line_start: line_num,
        line_end,
        visibility: Visibility::Public,
        params: vec![],
        return_type: None,
        complexity,
    })
}

fn parse_js_class(line: &str, line_num: usize, lines: &[&str]) -> Option<CodeSymbol> {
    let name_start = line.find("class ")? + 6;
    let name_end = line[name_start..]
        .find(['{', ' '])
        .map(|i| i + name_start)
        .unwrap_or(line.len());
    let name = line[name_start..name_end].trim().to_string();

    let line_end = find_block_end(lines, line_num - 1);

    Some(CodeSymbol {
        name,
        symbol_type: SymbolType::Class,
        line_start: line_num,
        line_end,
        visibility: Visibility::Public,
        params: vec![],
        return_type: None,
        complexity: 1,
    })
}

fn find_block_end(lines: &[&str], start: usize) -> usize {
    let mut depth = 0;
    let mut started = false;

    for (i, line) in lines.iter().enumerate().skip(start) {
        for c in line.chars() {
            if c == '{' {
                depth += 1;
                started = true;
            } else if c == '}' {
                depth -= 1;
                if started && depth == 0 {
                    return i + 1;
                }
            }
        }
    }

    lines.len()
}

fn find_python_block_end(lines: &[&str], start: usize) -> usize {
    let start_indent = lines
        .get(start)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent <= start_indent {
            return i;
        }
    }

    lines.len()
}

fn extract_js_import_module(line: &str) -> String {
    if line.contains("from ") {
        if let Some(start) = line.find("from ") {
            let start = start + 5;
            let content = line[start..].trim();
            return content
                .trim_matches(|c| c == '"' || c == '\'' || c == ';')
                .to_string();
        }
    } else if line.contains("require(") {
        if let Some(start) = line.find("require(") {
            let start = start + 8;
            if let Some(end) = line[start..].find(')') {
                return line[start..start + end]
                    .trim_matches(|c| c == '"' || c == '\'')
                    .to_string();
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(detect_language(Path::new("test.rs")), "Rust");
        assert_eq!(detect_language(Path::new("test.py")), "Python");
        assert_eq!(detect_language(Path::new("test.ts")), "TypeScript");
    }

    #[test]
    fn test_complexity_calculation() {
        let code = r#"
        fn test() {
            if a {
                if b {
                    while c {
                    }
                }
            }
        }
        "#;
        let complexity = calculate_cyclomatic_complexity(code);
        assert!(complexity >= 4);
    }
}
