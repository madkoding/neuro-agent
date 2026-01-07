//! Code formatter tool - Format code in various languages

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;
use std::process::Stdio;

/// Supported languages for formatting
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FormatLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Cpp,
    C,
    Json,
    Yaml,
    Toml,
    Markdown,
    Html,
    Css,
    Sql,
}

/// Formatter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatConfig {
    pub indent_size: Option<usize>,
    pub use_tabs: Option<bool>,
    pub line_width: Option<usize>,
    pub quote_style: Option<QuoteStyle>,
    pub trailing_comma: Option<bool>,
    pub semicolons: Option<bool>,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent_size: Some(4),
            use_tabs: Some(false),
            line_width: Some(100),
            quote_style: Some(QuoteStyle::Double),
            trailing_comma: Some(true),
            semicolons: Some(true),
        }
    }
}

/// Quote style preference
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QuoteStyle {
    Single,
    Double,
}

/// Format arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatArgs {
    pub path: String,
    pub language: Option<FormatLanguage>,
    pub config: Option<FormatConfig>,
    pub check_only: Option<bool>,
    pub recursive: Option<bool>,
}

/// Format result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatResult {
    pub path: String,
    pub formatted: bool,
    pub changed: bool,
    pub diff: Option<String>,
    pub error: Option<String>,
}

/// Format output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatOutput {
    pub results: Vec<FormatResult>,
    pub total_files: usize,
    pub formatted_files: usize,
    pub changed_files: usize,
    pub error_files: usize,
}

/// Code formatter tool
#[derive(Debug, Clone)]
pub struct FormatterTool;

impl Default for FormatterTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatterTool {
    pub const NAME: &'static str = "format_code";

    pub fn new() -> Self {
        Self
    }

    /// Detect language from file extension
    pub fn detect_language(path: &Path) -> Option<FormatLanguage> {
        let ext = path.extension()?.to_str()?;
        
        match ext.to_lowercase().as_str() {
            "rs" => Some(FormatLanguage::Rust),
            "py" => Some(FormatLanguage::Python),
            "js" | "mjs" | "cjs" => Some(FormatLanguage::JavaScript),
            "ts" | "mts" | "cts" => Some(FormatLanguage::TypeScript),
            "jsx" => Some(FormatLanguage::JavaScript),
            "tsx" => Some(FormatLanguage::TypeScript),
            "go" => Some(FormatLanguage::Go),
            "java" => Some(FormatLanguage::Java),
            "cpp" | "cc" | "cxx" | "hpp" => Some(FormatLanguage::Cpp),
            "c" | "h" => Some(FormatLanguage::C),
            "json" => Some(FormatLanguage::Json),
            "yaml" | "yml" => Some(FormatLanguage::Yaml),
            "toml" => Some(FormatLanguage::Toml),
            "md" | "markdown" => Some(FormatLanguage::Markdown),
            "html" | "htm" => Some(FormatLanguage::Html),
            "css" | "scss" | "less" => Some(FormatLanguage::Css),
            "sql" => Some(FormatLanguage::Sql),
            _ => None,
        }
    }

    /// Format a file or directory
    pub async fn format(&self, args: FormatArgs) -> Result<FormatOutput, FormatError> {
        let path = PathBuf::from(&args.path);
        
        if !path.exists() {
            return Err(FormatError::PathNotFound(args.path));
        }

        let mut results = Vec::new();

        if path.is_file() {
            let result = self.format_file(&path, &args).await;
            results.push(result);
        } else if args.recursive.unwrap_or(false) {
            self.format_dir_recursive(&path, &args, &mut results).await?;
        } else {
            self.format_dir(&path, &args, &mut results).await?;
        }

        let total_files = results.len();
        let formatted_files = results.iter().filter(|r| r.formatted).count();
        let changed_files = results.iter().filter(|r| r.changed).count();
        let error_files = results.iter().filter(|r| r.error.is_some()).count();

        Ok(FormatOutput {
            results,
            total_files,
            formatted_files,
            changed_files,
            error_files,
        })
    }

    async fn format_file(&self, path: &Path, args: &FormatArgs) -> FormatResult {
        let language = args.language.clone()
            .or_else(|| Self::detect_language(path));

        let Some(lang) = language else {
            return FormatResult {
                path: path.to_string_lossy().to_string(),
                formatted: false,
                changed: false,
                diff: None,
                error: Some("Unknown language".to_string()),
            };
        };

        let result = match lang {
            FormatLanguage::Rust => self.format_rust(path, args).await,
            FormatLanguage::Python => self.format_python(path, args).await,
            FormatLanguage::JavaScript | FormatLanguage::TypeScript => {
                self.format_js_ts(path, args).await
            }
            FormatLanguage::Go => self.format_go(path, args).await,
            FormatLanguage::Json => self.format_json(path, args).await,
            FormatLanguage::Yaml => self.format_yaml(path, args).await,
            FormatLanguage::Toml => self.format_toml(path, args).await,
            _ => self.format_with_prettier(path, args).await,
        };

        match result {
            Ok((changed, diff)) => FormatResult {
                path: path.to_string_lossy().to_string(),
                formatted: true,
                changed,
                diff,
                error: None,
            },
            Err(e) => FormatResult {
                path: path.to_string_lossy().to_string(),
                formatted: false,
                changed: false,
                diff: None,
                error: Some(e.to_string()),
            },
        }
    }

    async fn format_dir(&self, path: &Path, args: &FormatArgs, results: &mut Vec<FormatResult>) -> Result<(), FormatError> {
        let mut read_dir = fs::read_dir(path).await
            .map_err(|e| FormatError::IoError(e.to_string()))?;

        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| FormatError::IoError(e.to_string()))?
        {
            let entry_path = entry.path();
            if entry_path.is_file() && Self::detect_language(&entry_path).is_some() {
                let result = self.format_file(&entry_path, args).await;
                results.push(result);
            }
        }

        Ok(())
    }

    async fn format_dir_recursive(&self, path: &Path, args: &FormatArgs, results: &mut Vec<FormatResult>) -> Result<(), FormatError> {
        let mut read_dir = fs::read_dir(path).await
            .map_err(|e| FormatError::IoError(e.to_string()))?;

        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| FormatError::IoError(e.to_string()))?
        {
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            // Skip hidden dirs and common non-source dirs
            if file_name.starts_with('.') || 
               file_name == "node_modules" ||
               file_name == "target" ||
               file_name == "__pycache__" ||
               file_name == "venv" {
                continue;
            }
            
            if entry_path.is_dir() {
                Box::pin(self.format_dir_recursive(&entry_path, args, results)).await?;
            } else if entry_path.is_file() && Self::detect_language(&entry_path).is_some() {
                let result = self.format_file(&entry_path, args).await;
                results.push(result);
            }
        }

        Ok(())
    }

    async fn format_rust(&self, path: &Path, args: &FormatArgs) -> Result<(bool, Option<String>), FormatError> {
        let check_only = args.check_only.unwrap_or(false);
        
        let mut cmd = Command::new("rustfmt");
        
        if check_only {
            cmd.arg("--check");
        }
        
        cmd.arg(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await
            .map_err(|e| FormatError::FormatterError(format!("rustfmt: {}", e)))?;

        let changed = !output.status.success() && check_only;
        let diff = if check_only && !output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            None
        };

        Ok((changed, diff))
    }

    async fn format_python(&self, path: &Path, args: &FormatArgs) -> Result<(bool, Option<String>), FormatError> {
        let check_only = args.check_only.unwrap_or(false);
        
        // Try black first, fall back to autopep8
        let mut cmd = Command::new("black");
        
        if check_only {
            cmd.arg("--check").arg("--diff");
        }
        
        if let Some(ref config) = args.config {
            if let Some(line_width) = config.line_width {
                cmd.arg("--line-length").arg(line_width.to_string());
            }
        }
        
        cmd.arg(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await
            .map_err(|e| FormatError::FormatterError(format!("black: {}", e)))?;

        let changed = !output.status.success() && check_only;
        let diff = if check_only {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            None
        };

        Ok((changed, diff))
    }

    async fn format_js_ts(&self, path: &Path, args: &FormatArgs) -> Result<(bool, Option<String>), FormatError> {
        let check_only = args.check_only.unwrap_or(false);
        
        // Use prettier
        let mut cmd = Command::new("npx");
        cmd.arg("prettier");
        
        if check_only {
            cmd.arg("--check");
        } else {
            cmd.arg("--write");
        }
        
        if let Some(ref config) = args.config {
            if let Some(indent_size) = config.indent_size {
                cmd.arg("--tab-width").arg(indent_size.to_string());
            }
            if let Some(use_tabs) = config.use_tabs {
                if use_tabs {
                    cmd.arg("--use-tabs");
                }
            }
            if let Some(ref quote_style) = config.quote_style {
                match quote_style {
                    QuoteStyle::Single => cmd.arg("--single-quote"),
                    QuoteStyle::Double => cmd.arg("--no-single-quote"),
                };
            }
            if let Some(trailing_comma) = config.trailing_comma {
                cmd.arg("--trailing-comma").arg(if trailing_comma { "all" } else { "none" });
            }
        }
        
        cmd.arg(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await
            .map_err(|e| FormatError::FormatterError(format!("prettier: {}", e)))?;

        let changed = !output.status.success() && check_only;

        Ok((changed, None))
    }

    async fn format_go(&self, path: &Path, args: &FormatArgs) -> Result<(bool, Option<String>), FormatError> {
        let check_only = args.check_only.unwrap_or(false);
        
        let mut cmd = Command::new("gofmt");
        
        if check_only {
            cmd.arg("-d"); // Print diff
        } else {
            cmd.arg("-w"); // Write to file
        }
        
        cmd.arg(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await
            .map_err(|e| FormatError::FormatterError(format!("gofmt: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let changed = !stdout.is_empty() && check_only;
        let diff = if check_only && !stdout.is_empty() {
            Some(stdout.to_string())
        } else {
            None
        };

        Ok((changed, diff))
    }

    async fn format_json(&self, path: &Path, args: &FormatArgs) -> Result<(bool, Option<String>), FormatError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| FormatError::IoError(e.to_string()))?;

        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| FormatError::ParseError(e.to_string()))?;

        let indent = args.config.as_ref()
            .and_then(|c| c.indent_size)
            .unwrap_or(2);

        let formatted = serde_json::to_string_pretty(&parsed)
            .map_err(|e| FormatError::FormatterError(e.to_string()))?;

        // Apply custom indent
        let formatted = if indent != 2 {
            let spaces = " ".repeat(indent);
            formatted.lines()
                .map(|line| {
                    let trimmed = line.trim_start();
                    let leading_spaces = line.len() - trimmed.len();
                    let indent_level = leading_spaces / 2;
                    format!("{}{}", spaces.repeat(indent_level), trimmed)
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            formatted
        };

        let changed = formatted != content;

        if !args.check_only.unwrap_or(false) && changed {
            fs::write(path, &formatted).await
                .map_err(|e| FormatError::IoError(e.to_string()))?;
        }

        Ok((changed, None))
    }

    async fn format_yaml(&self, _path: &Path, _args: &FormatArgs) -> Result<(bool, Option<String>), FormatError> {
        // YAML formatting would require a YAML library
        // For now, just return unchanged
        Ok((false, None))
    }

    async fn format_toml(&self, path: &Path, args: &FormatArgs) -> Result<(bool, Option<String>), FormatError> {
        let content = fs::read_to_string(path).await
            .map_err(|e| FormatError::IoError(e.to_string()))?;

        let parsed: toml::Value = toml::from_str(&content)
            .map_err(|e| FormatError::ParseError(e.to_string()))?;

        let formatted = toml::to_string_pretty(&parsed)
            .map_err(|e| FormatError::FormatterError(e.to_string()))?;

        let changed = formatted != content;

        if !args.check_only.unwrap_or(false) && changed {
            fs::write(path, &formatted).await
                .map_err(|e| FormatError::IoError(e.to_string()))?;
        }

        Ok((changed, None))
    }

    async fn format_with_prettier(&self, path: &Path, args: &FormatArgs) -> Result<(bool, Option<String>), FormatError> {
        // Generic formatting with prettier
        let check_only = args.check_only.unwrap_or(false);
        
        let mut cmd = Command::new("npx");
        cmd.arg("prettier");
        
        if check_only {
            cmd.arg("--check");
        } else {
            cmd.arg("--write");
        }
        
        cmd.arg(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await
            .map_err(|e| FormatError::FormatterError(format!("prettier: {}", e)))?;

        let changed = !output.status.success() && check_only;

        Ok((changed, None))
    }

    /// Format code string directly
    pub async fn format_string(&self, code: &str, language: FormatLanguage) -> Result<String, FormatError> {
        match language {
            FormatLanguage::Json => {
                let parsed: serde_json::Value = serde_json::from_str(code)
                    .map_err(|e| FormatError::ParseError(e.to_string()))?;
                serde_json::to_string_pretty(&parsed)
                    .map_err(|e| FormatError::FormatterError(e.to_string()))
            }
            FormatLanguage::Toml => {
                let parsed: toml::Value = toml::from_str(code)
                    .map_err(|e| FormatError::ParseError(e.to_string()))?;
                toml::to_string_pretty(&parsed)
                    .map_err(|e| FormatError::FormatterError(e.to_string()))
            }
            _ => {
                // For other languages, we'd need to write to a temp file
                // For now, return as-is
                Ok(code.to_string())
            }
        }
    }
}

/// Formatter errors
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Formatter error: {0}")]
    FormatterError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(FormatterTool::detect_language(Path::new("test.rs")), Some(FormatLanguage::Rust));
        assert_eq!(FormatterTool::detect_language(Path::new("test.py")), Some(FormatLanguage::Python));
        assert_eq!(FormatterTool::detect_language(Path::new("test.ts")), Some(FormatLanguage::TypeScript));
        assert_eq!(FormatterTool::detect_language(Path::new("test.json")), Some(FormatLanguage::Json));
    }

    #[tokio::test]
    async fn test_format_json_string() {
        let formatter = FormatterTool::new();
        let input = r#"{"name":"test","value":123}"#;
        let result = formatter.format_string(input, FormatLanguage::Json).await.unwrap();
        assert!(result.contains('\n'));
        assert!(result.contains("  ")); // Indentation
    }
}
