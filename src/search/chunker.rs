//! Code Chunker - AST-aware code chunking for semantic search
//!
//! Divides code into semantic chunks (functions, structs, modules) for embedding generation.

use crate::ast::{AstParser, AstSymbol, Range, SupportedLanguage, SymbolKind};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Maximum lines for a file chunk (if no symbols found)
const MAX_FILE_CHUNK_LINES: usize = 200;

/// Type of code chunk
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkType {
    Function,
    Method,
    Struct,
    Class,
    Enum,
    Trait,
    Interface,
    Module,
    File,
}

impl ChunkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Module => "module",
            Self::File => "file",
        }
    }

    fn from_symbol_kind(kind: &SymbolKind) -> Self {
        match kind {
            SymbolKind::Function => Self::Function,
            SymbolKind::Method => Self::Method,
            SymbolKind::Struct => Self::Struct,
            SymbolKind::Class => Self::Class,
            SymbolKind::Enum => Self::Enum,
            SymbolKind::Trait => Self::Trait,
            SymbolKind::Interface => Self::Interface,
            SymbolKind::Module => Self::Module,
            _ => Self::Function,
        }
    }
}

/// A semantic code chunk
#[derive(Debug, Clone)]
pub struct CodeChunk {
    /// Unique identifier for this chunk
    pub id: String,

    /// Type of chunk
    pub chunk_type: ChunkType,

    /// Full text content of the chunk
    pub text: String,

    /// Summary/description (for LLM context)
    pub summary: String,

    /// File path this chunk belongs to
    pub file_path: PathBuf,

    /// Associated symbol ID from database (if any)
    pub symbol_id: Option<i64>,

    /// Start line (1-indexed)
    pub line_start: usize,

    /// End line (1-indexed)
    pub line_end: usize,

    /// Programming language
    pub language: String,

    /// Symbol name (if applicable)
    pub symbol_name: Option<String>,
}

impl CodeChunk {
    /// Create a new code chunk
    pub fn new(
        chunk_type: ChunkType,
        text: String,
        file_path: PathBuf,
        line_start: usize,
        line_end: usize,
        language: String,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let summary = Self::generate_summary(&text, &chunk_type, line_start, line_end);

        Self {
            id,
            chunk_type,
            text,
            summary,
            file_path,
            symbol_id: None,
            line_start,
            line_end,
            language,
            symbol_name: None,
        }
    }

    /// Create chunk from AST symbol
    pub fn from_symbol(
        symbol: &AstSymbol,
        source: &str,
        file_path: PathBuf,
        language: String,
    ) -> Self {
        let chunk_type = ChunkType::from_symbol_kind(&symbol.kind);
        let text = extract_text_from_range(&symbol.range, source);
        let id = Uuid::new_v4().to_string();

        let summary = format!(
            "{} `{}` (lines {}-{}): {}",
            chunk_type.as_str(),
            symbol.name,
            symbol.range.start_line,
            symbol.range.end_line,
            symbol.docstring.as_deref().unwrap_or("No documentation")
        );

        Self {
            id,
            chunk_type,
            text,
            summary,
            file_path,
            symbol_id: None,
            line_start: symbol.range.start_line,
            line_end: symbol.range.end_line,
            language,
            symbol_name: Some(symbol.name.clone()),
        }
    }

    /// Generate a summary for the chunk
    fn generate_summary(
        text: &str,
        chunk_type: &ChunkType,
        line_start: usize,
        line_end: usize,
    ) -> String {
        let lines = text.lines().count();
        format!(
            "{} chunk (lines {}-{}, {} lines)",
            chunk_type.as_str(),
            line_start,
            line_end,
            lines
        )
    }

    /// Format chunk for LLM context
    pub fn format_for_llm(&self) -> String {
        format!(
            "**{}** in `{}`\n```{}\n{}\n```",
            self.summary,
            self.file_path.display(),
            self.language,
            self.text
        )
    }
}

/// Code chunker using AST
pub struct CodeChunker {
    ast_parser: AstParser,
}

impl CodeChunker {
    /// Create a new code chunker
    pub fn new() -> Result<Self> {
        let ast_parser = AstParser::new()?;
        Ok(Self { ast_parser })
    }

    /// Chunk a file into semantic chunks
    pub fn chunk_file(
        &mut self,
        file_path: &Path,
        content: &str,
        language: &str,
    ) -> Result<Vec<CodeChunk>> {
        let supported_lang = SupportedLanguage::parse_language(language);

        // If language not supported, fall back to simple chunking
        let Some(lang) = supported_lang else {
            return Ok(self.chunk_by_lines(file_path, content, language));
        };

        // Parse with AST
        let tree = self
            .ast_parser
            .parse(lang, content)
            .context("Failed to parse file")?;

        let symbols = self.ast_parser.extract_symbols(&tree, lang, content);

        // If no symbols found, fall back to file-level chunk
        if symbols.is_empty() {
            return Ok(self.chunk_by_lines(file_path, content, language));
        }

        // Create chunks from symbols
        let mut chunks = Vec::new();
        for symbol in symbols {
            let chunk = CodeChunk::from_symbol(
                &symbol,
                content,
                file_path.to_path_buf(),
                language.to_string(),
            );
            chunks.push(chunk);
        }

        Ok(chunks)
    }

    /// Simple line-based chunking (fallback)
    fn chunk_by_lines(&self, file_path: &Path, content: &str, language: &str) -> Vec<CodeChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // If file is small enough, create single chunk
        if total_lines <= MAX_FILE_CHUNK_LINES {
            let chunk = CodeChunk::new(
                ChunkType::File,
                content.to_string(),
                file_path.to_path_buf(),
                1,
                total_lines,
                language.to_string(),
            );
            return vec![chunk];
        }

        // Split into multiple chunks
        let mut chunks = Vec::new();
        let mut start = 0;

        while start < total_lines {
            let end = (start + MAX_FILE_CHUNK_LINES).min(total_lines);
            let chunk_text = lines[start..end].join("\n");

            let chunk = CodeChunk::new(
                ChunkType::File,
                chunk_text,
                file_path.to_path_buf(),
                start + 1,
                end,
                language.to_string(),
            );

            chunks.push(chunk);
            start = end;
        }

        chunks
    }

    /// Chunk multiple files
    pub fn chunk_files(&mut self, files: Vec<(PathBuf, String, String)>) -> Result<Vec<CodeChunk>> {
        let mut all_chunks = Vec::new();

        for (path, content, language) in files {
            let chunks = self.chunk_file(&path, &content, &language)?;
            all_chunks.extend(chunks);
        }

        Ok(all_chunks)
    }
}

/// Extract text from a range in source code
fn extract_text_from_range(range: &Range, source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();

    if range.start_line > lines.len() || range.start_line == 0 {
        return String::new();
    }

    let start_idx = range.start_line - 1; // Convert to 0-indexed
    let end_idx = (range.end_line).min(lines.len());

    if start_idx >= end_idx {
        return String::new();
    }

    lines[start_idx..end_idx].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_rust_file() {
        let code = r#"
pub fn hello() {
    println!("Hello");
}

pub struct Point {
    x: i32,
    y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}
"#;

        let mut chunker = CodeChunker::new().unwrap();
        let chunks = chunker
            .chunk_file(Path::new("test.rs"), code, "rust")
            .unwrap();

        // Should have chunks for function, struct, and impl method
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn test_chunk_python_file() {
        let code = r#"
def greet(name):
    """Greet someone"""
    return f"Hello, {name}"

class Calculator:
    def add(self, a, b):
        return a + b
"#;

        let mut chunker = CodeChunker::new().unwrap();
        let chunks = chunker
            .chunk_file(Path::new("test.py"), code, "python")
            .unwrap();

        assert!(chunks.len() >= 2);
    }

    #[test]
    fn test_chunk_by_lines() {
        let code = "Line 1\nLine 2\nLine 3";
        let chunker = CodeChunker::new().unwrap();

        let chunks = chunker.chunk_by_lines(Path::new("test.txt"), code, "text");

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_type, ChunkType::File);
        assert_eq!(chunks[0].line_start, 1);
        assert_eq!(chunks[0].line_end, 3);
    }

    #[test]
    fn test_chunk_large_file() {
        let mut lines = Vec::new();
        for i in 0..300 {
            lines.push(format!("Line {}", i));
        }
        let code = lines.join("\n");

        let chunker = CodeChunker::new().unwrap();
        let chunks = chunker.chunk_by_lines(Path::new("large.txt"), &code, "text");

        // Should be split into multiple chunks
        assert!(chunks.len() > 1);
    }
}
