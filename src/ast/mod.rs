//! AST Parsing Module
//!
//! Provides multi-language AST parsing using tree-sitter for accurate code analysis.

use anyhow::{Context, Result};
use std::collections::HashMap;
use tree_sitter::{Language, Node, Parser, Tree};

/// Supported languages for AST parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    Rust,
    Python,
    TypeScript,
    JavaScript,
}

impl SupportedLanguage {
    pub fn parse_language(lang: &str) -> Option<Self> {
        match lang.to_lowercase().as_str() {
            "rust" | "rs" => Some(Self::Rust),
            "python" | "py" => Some(Self::Python),
            "typescript" | "ts" => Some(Self::TypeScript),
            "javascript" | "js" => Some(Self::JavaScript),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
        }
    }

    fn tree_sitter_language(&self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        }
    }
}

/// Symbol kind extracted from AST
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Enum,
    Trait,
    Interface,
    Constant,
    Variable,
    Module,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Constant => "constant",
            Self::Variable => "variable",
            Self::Module => "module",
        }
    }
}

/// Visibility of a symbol
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Internal,
    Protected,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
            Self::Internal => "internal",
            Self::Protected => "protected",
        }
    }
}

/// Range in source code
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Range {
    fn from_node(node: &Node) -> Self {
        let start = node.start_position();
        let end = node.end_position();
        Self {
            start_line: start.row + 1, // 1-indexed
            start_col: start.column,
            end_line: end.row + 1,
            end_col: end.column,
        }
    }
}

/// Parameter of a function/method
#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub type_annotation: Option<String>,
    pub default_value: Option<String>,
}

/// Symbol extracted from AST
#[derive(Debug, Clone)]
pub struct AstSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub visibility: Visibility,
    pub params: Vec<Parameter>,
    pub return_type: Option<String>,
    pub docstring: Option<String>,
    pub decorators: Vec<String>,
    pub is_async: bool,
    pub is_test: bool,
}

/// Import statement
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub items: Vec<String>,
    pub is_wildcard: bool,
    pub line: usize,
}

/// Function call site
#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub function_name: String,
    pub line: usize,
}

/// Multi-language AST parser
pub struct AstParser {
    parsers: HashMap<SupportedLanguage, Parser>,
}

impl AstParser {
    /// Create a new AST parser with support for multiple languages
    pub fn new() -> Result<Self> {
        let mut parsers = HashMap::new();

        for lang in [
            SupportedLanguage::Rust,
            SupportedLanguage::Python,
            SupportedLanguage::TypeScript,
            SupportedLanguage::JavaScript,
        ] {
            let mut parser = Parser::new();
            parser
                .set_language(&lang.tree_sitter_language())
                .context(format!("Failed to set language for {}", lang.as_str()))?;
            parsers.insert(lang, parser);
        }

        Ok(Self { parsers })
    }

    /// Parse source code into an AST
    pub fn parse(&mut self, language: SupportedLanguage, code: &str) -> Result<Tree> {
        let parser = self
            .parsers
            .get_mut(&language)
            .context("Unsupported language")?;

        parser.parse(code, None).context("Failed to parse code")
    }

    /// Extract all symbols from the AST
    pub fn extract_symbols(
        &self,
        tree: &Tree,
        language: SupportedLanguage,
        source: &str,
    ) -> Vec<AstSymbol> {
        match language {
            SupportedLanguage::Rust => self.extract_rust_symbols(tree, source),
            SupportedLanguage::Python => self.extract_python_symbols(tree, source),
            SupportedLanguage::TypeScript => self.extract_typescript_symbols(tree, source),
            SupportedLanguage::JavaScript => self.extract_javascript_symbols(tree, source),
        }
    }

    /// Extract Rust symbols from AST
    fn extract_rust_symbols(&self, tree: &Tree, source: &str) -> Vec<AstSymbol> {
        let mut symbols = Vec::new();
        let mut cursor = tree.walk();

        fn traverse(
            node: &Node,
            source: &str,
            symbols: &mut Vec<AstSymbol>,
            cursor: &mut tree_sitter::TreeCursor,
        ) {
            match node.kind() {
                "function_item" => {
                    if let Some(symbol) = extract_rust_function(node, source) {
                        symbols.push(symbol);
                    }
                }
                "struct_item" => {
                    if let Some(symbol) = extract_rust_struct(node, source) {
                        symbols.push(symbol);
                    }
                }
                "enum_item" => {
                    if let Some(symbol) = extract_rust_enum(node, source) {
                        symbols.push(symbol);
                    }
                }
                "trait_item" => {
                    if let Some(symbol) = extract_rust_trait(node, source) {
                        symbols.push(symbol);
                    }
                }
                "impl_item" => {
                    // Traverse impl block to find methods
                    if cursor.goto_first_child() {
                        loop {
                            let child = cursor.node();
                            traverse(&child, source, symbols, cursor);
                            if !cursor.goto_next_sibling() {
                                break;
                            }
                        }
                        cursor.goto_parent();
                    }
                }
                _ => {
                    // Recursively traverse children
                    if cursor.goto_first_child() {
                        loop {
                            let child = cursor.node();
                            traverse(&child, source, symbols, cursor);
                            if !cursor.goto_next_sibling() {
                                break;
                            }
                        }
                        cursor.goto_parent();
                    }
                }
            }
        }

        let root = tree.root_node();
        traverse(&root, source, &mut symbols, &mut cursor);
        symbols
    }

    /// Extract Python symbols from AST
    fn extract_python_symbols(&self, tree: &Tree, source: &str) -> Vec<AstSymbol> {
        let mut symbols = Vec::new();
        let mut cursor = tree.walk();

        fn traverse(
            node: &Node,
            source: &str,
            symbols: &mut Vec<AstSymbol>,
            cursor: &mut tree_sitter::TreeCursor,
        ) {
            match node.kind() {
                "function_definition" => {
                    if let Some(symbol) = extract_python_function(node, source) {
                        symbols.push(symbol);
                    }
                }
                "class_definition" => {
                    if let Some(symbol) = extract_python_class(node, source) {
                        symbols.push(symbol);
                    }
                    // Also traverse class methods
                    if cursor.goto_first_child() {
                        loop {
                            let child = cursor.node();
                            traverse(&child, source, symbols, cursor);
                            if !cursor.goto_next_sibling() {
                                break;
                            }
                        }
                        cursor.goto_parent();
                    }
                }
                _ => {
                    if cursor.goto_first_child() {
                        loop {
                            let child = cursor.node();
                            traverse(&child, source, symbols, cursor);
                            if !cursor.goto_next_sibling() {
                                break;
                            }
                        }
                        cursor.goto_parent();
                    }
                }
            }
        }

        let root = tree.root_node();
        traverse(&root, source, &mut symbols, &mut cursor);
        symbols
    }

    /// Extract TypeScript symbols from AST
    fn extract_typescript_symbols(&self, tree: &Tree, source: &str) -> Vec<AstSymbol> {
        let mut symbols = Vec::new();
        let mut cursor = tree.walk();

        fn traverse(
            node: &Node,
            source: &str,
            symbols: &mut Vec<AstSymbol>,
            cursor: &mut tree_sitter::TreeCursor,
        ) {
            match node.kind() {
                "function_declaration" | "method_definition" => {
                    if let Some(symbol) = extract_ts_function(node, source) {
                        symbols.push(symbol);
                    }
                }
                "class_declaration" => {
                    if let Some(symbol) = extract_ts_class(node, source) {
                        symbols.push(symbol);
                    }
                    // Traverse class methods
                    if cursor.goto_first_child() {
                        loop {
                            let child = cursor.node();
                            traverse(&child, source, symbols, cursor);
                            if !cursor.goto_next_sibling() {
                                break;
                            }
                        }
                        cursor.goto_parent();
                    }
                }
                "interface_declaration" => {
                    if let Some(symbol) = extract_ts_interface(node, source) {
                        symbols.push(symbol);
                    }
                }
                _ => {
                    if cursor.goto_first_child() {
                        loop {
                            let child = cursor.node();
                            traverse(&child, source, symbols, cursor);
                            if !cursor.goto_next_sibling() {
                                break;
                            }
                        }
                        cursor.goto_parent();
                    }
                }
            }
        }

        let root = tree.root_node();
        traverse(&root, source, &mut symbols, &mut cursor);
        symbols
    }

    /// Extract JavaScript symbols from AST
    fn extract_javascript_symbols(&self, tree: &Tree, source: &str) -> Vec<AstSymbol> {
        // JavaScript parsing is similar to TypeScript
        self.extract_typescript_symbols(tree, source)
    }

    /// Extract imports from the AST
    pub fn extract_imports(
        &self,
        tree: &Tree,
        language: SupportedLanguage,
        source: &str,
    ) -> Vec<Import> {
        match language {
            SupportedLanguage::Rust => extract_rust_imports(tree, source),
            SupportedLanguage::Python => extract_python_imports(tree, source),
            SupportedLanguage::TypeScript | SupportedLanguage::JavaScript => {
                extract_ts_imports(tree, source)
            }
        }
    }

    /// Calculate cyclomatic complexity of a function
    pub fn calculate_complexity(&self, node: &Node, _source: &str) -> usize {
        let mut complexity = 1; // Base complexity
        let mut cursor = node.walk();

        fn count_decision_points(
            node: &Node,
            cursor: &mut tree_sitter::TreeCursor,
            count: &mut usize,
        ) {
            match node.kind() {
                "if_expression" | "if_statement" | "while_statement" | "while_expression"
                | "for_statement" | "for_expression" | "match_expression" | "match_arm"
                | "binary_expression" => {
                    *count += 1;
                }
                _ => {}
            }

            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    count_decision_points(&child, cursor, count);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                cursor.goto_parent();
            }
        }

        count_decision_points(node, &mut cursor, &mut complexity);
        complexity
    }
}

// Helper functions for extracting Rust symbols
fn extract_rust_function(node: &Node, source: &str) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    let is_async = node.children(&mut node.walk()).any(|n| n.kind() == "async");
    let is_test = has_test_attribute(node, source);

    let visibility = extract_rust_visibility(node, source);
    let params = extract_rust_parameters(node, source);
    let return_type = extract_rust_return_type(node, source);
    let docstring = extract_rust_docstring(node, source);

    Some(AstSymbol {
        name,
        kind: SymbolKind::Function,
        range: Range::from_node(node),
        visibility,
        params,
        return_type,
        docstring,
        decorators: Vec::new(),
        is_async,
        is_test,
    })
}

fn extract_rust_struct(node: &Node, source: &str) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);
    let visibility = extract_rust_visibility(node, source);
    let docstring = extract_rust_docstring(node, source);

    Some(AstSymbol {
        name,
        kind: SymbolKind::Struct,
        range: Range::from_node(node),
        visibility,
        params: Vec::new(),
        return_type: None,
        docstring,
        decorators: Vec::new(),
        is_async: false,
        is_test: false,
    })
}

fn extract_rust_enum(node: &Node, source: &str) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);
    let visibility = extract_rust_visibility(node, source);
    let docstring = extract_rust_docstring(node, source);

    Some(AstSymbol {
        name,
        kind: SymbolKind::Enum,
        range: Range::from_node(node),
        visibility,
        params: Vec::new(),
        return_type: None,
        docstring,
        decorators: Vec::new(),
        is_async: false,
        is_test: false,
    })
}

fn extract_rust_trait(node: &Node, source: &str) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);
    let visibility = extract_rust_visibility(node, source);
    let docstring = extract_rust_docstring(node, source);

    Some(AstSymbol {
        name,
        kind: SymbolKind::Trait,
        range: Range::from_node(node),
        visibility,
        params: Vec::new(),
        return_type: None,
        docstring,
        decorators: Vec::new(),
        is_async: false,
        is_test: false,
    })
}

fn extract_rust_visibility(node: &Node, source: &str) -> Visibility {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "visibility_modifier" {
            let text = get_node_text(&child, source);
            if text.contains("pub") {
                return Visibility::Public;
            }
        }
    }
    Visibility::Private
}

fn extract_rust_parameters(node: &Node, source: &str) -> Vec<Parameter> {
    let mut params = Vec::new();
    if let Some(params_node) = node.child_by_field_name("parameters") {
        for child in params_node.children(&mut params_node.walk()) {
            if child.kind() == "parameter" {
                if let Some(pattern) = child.child_by_field_name("pattern") {
                    let name = get_node_text(&pattern, source);
                    let type_annotation = child
                        .child_by_field_name("type")
                        .map(|t| get_node_text(&t, source));
                    params.push(Parameter {
                        name,
                        type_annotation,
                        default_value: None,
                    });
                }
            }
        }
    }
    params
}

fn extract_rust_return_type(node: &Node, source: &str) -> Option<String> {
    node.child_by_field_name("return_type")
        .map(|t| get_node_text(&t, source))
}

fn extract_rust_docstring(node: &Node, _source: &str) -> Option<String> {
    // Look for doc comments (///) before the node
    let start_line = node.start_position().row;
    if start_line == 0 {
        return None;
    }

    // Simple implementation - could be enhanced
    None
}

fn has_test_attribute(node: &Node, source: &str) -> bool {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "attribute_item" {
            let text = get_node_text(&child, source);
            if text.contains("#[test]") || text.contains("#[cfg(test)]") {
                return true;
            }
        }
    }
    false
}

// Helper functions for Python
fn extract_python_function(node: &Node, source: &str) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    let is_async = node.children(&mut node.walk()).any(|n| n.kind() == "async");

    let decorators = extract_python_decorators(node, source);
    let is_test = decorators.iter().any(|d| d.contains("test"));

    let params = extract_python_parameters(node, source);
    let return_type = extract_python_return_type(node, source);

    Some(AstSymbol {
        name,
        kind: SymbolKind::Function,
        range: Range::from_node(node),
        visibility: Visibility::Public, // Python doesn't have strict visibility
        params,
        return_type,
        docstring: extract_python_docstring(node, source),
        decorators,
        is_async,
        is_test,
    })
}

fn extract_python_parameters(node: &Node, source: &str) -> Vec<Parameter> {
    let mut params = Vec::new();
    
    if let Some(params_node) = node.child_by_field_name("parameters") {
        for child in params_node.children(&mut params_node.walk()) {
            match child.kind() {
                "identifier" => {
                    let name = get_node_text(&child, source);
                    if name != "self" && name != "cls" {
                        params.push(Parameter {
                            name,
                            type_annotation: None,
                            default_value: None,
                        });
                    }
                }
                "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = get_node_text(&name_node, source);
                        if name != "self" && name != "cls" {
                            let type_annotation = child
                                .child_by_field_name("type")
                                .map(|t| get_node_text(&t, source));
                            let default_value = child
                                .child_by_field_name("value")
                                .map(|v| get_node_text(&v, source));
                            params.push(Parameter {
                                name,
                                type_annotation,
                                default_value,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    
    params
}

fn extract_python_return_type(node: &Node, source: &str) -> Option<String> {
    node.child_by_field_name("return_type")
        .map(|t| get_node_text(&t, source))
}

fn extract_python_class(node: &Node, source: &str) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    Some(AstSymbol {
        name,
        kind: SymbolKind::Class,
        range: Range::from_node(node),
        visibility: Visibility::Public,
        params: Vec::new(),
        return_type: None,
        docstring: extract_python_docstring(node, source),
        decorators: extract_python_decorators(node, source),
        is_async: false,
        is_test: false,
    })
}

fn extract_python_decorators(node: &Node, source: &str) -> Vec<String> {
    let mut decorators = Vec::new();
    for child in node.children(&mut node.walk()) {
        if child.kind() == "decorator" {
            decorators.push(get_node_text(&child, source));
        }
    }
    decorators
}

fn extract_python_docstring(node: &Node, source: &str) -> Option<String> {
    // Look for string literal as first statement in body
    if let Some(body) = node.child_by_field_name("body") {
        if let Some(child) = body.children(&mut body.walk()).next() {
            if child.kind() == "expression_statement" {
                for expr_child in child.children(&mut child.walk()) {
                    if expr_child.kind() == "string" {
                        return Some(get_node_text(&expr_child, source));
                    }
                }
            }
        }
    }
    None
}

// Helper functions for TypeScript/JavaScript
fn extract_ts_function(node: &Node, source: &str) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    let is_async = node.children(&mut node.walk()).any(|n| n.kind() == "async");
    
    let visibility = extract_ts_visibility(node, source);
    let params = extract_ts_parameters(node, source);
    let return_type = extract_ts_return_type(node, source);
    let docstring = extract_ts_jsdoc(node, source);
    let decorators = extract_ts_decorators(node, source);

    Some(AstSymbol {
        name,
        kind: if node.kind() == "method_definition" {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        },
        range: Range::from_node(node),
        visibility,
        params,
        return_type,
        docstring,
        decorators,
        is_async,
        is_test: false,
    })
}

fn extract_ts_visibility(node: &Node, source: &str) -> Visibility {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "accessibility_modifier" {
            let text = get_node_text(&child, source);
            return match text.as_str() {
                "public" => Visibility::Public,
                "private" => Visibility::Private,
                "protected" => Visibility::Protected,
                _ => Visibility::Public,
            };
        }
    }
    Visibility::Public
}

fn extract_ts_parameters(node: &Node, source: &str) -> Vec<Parameter> {
    let mut params = Vec::new();
    
    if let Some(params_node) = node.child_by_field_name("parameters") {
        for child in params_node.children(&mut params_node.walk()) {
            match child.kind() {
                "required_parameter" | "optional_parameter" => {
                    if let Some(pattern) = child.child_by_field_name("pattern") {
                        let name = get_node_text(&pattern, source);
                        let type_annotation = child
                            .child_by_field_name("type")
                            .map(|t| get_node_text(&t, source));
                        let default_value = child
                            .child_by_field_name("value")
                            .map(|v| get_node_text(&v, source));
                        params.push(Parameter {
                            name,
                            type_annotation,
                            default_value,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    
    params
}

fn extract_ts_return_type(node: &Node, source: &str) -> Option<String> {
    node.child_by_field_name("return_type")
        .map(|t| get_node_text(&t, source))
}

fn extract_ts_jsdoc(node: &Node, source: &str) -> Option<String> {
    // Look for comment node before the function
    let parent = node.parent()?;
    let start_byte = node.start_byte();
    
    // Search backwards for JSDoc comment
    for sibling in parent.children(&mut parent.walk()) {
        if sibling.end_byte() <= start_byte && sibling.kind() == "comment" {
            let text = get_node_text(&sibling, source);
            if text.starts_with("/**") {
                return Some(text);
            }
        }
    }
    None
}

fn extract_ts_decorators(node: &Node, source: &str) -> Vec<String> {
    let mut decorators = Vec::new();
    for child in node.children(&mut node.walk()) {
        if child.kind() == "decorator" {
            decorators.push(get_node_text(&child, source));
        }
    }
    decorators
}

fn extract_ts_class(node: &Node, source: &str) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    Some(AstSymbol {
        name,
        kind: SymbolKind::Class,
        range: Range::from_node(node),
        visibility: Visibility::Public,
        params: Vec::new(),
        return_type: None,
        docstring: None,
        decorators: Vec::new(),
        is_async: false,
        is_test: false,
    })
}

fn extract_ts_interface(node: &Node, source: &str) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, source);

    Some(AstSymbol {
        name,
        kind: SymbolKind::Interface,
        range: Range::from_node(node),
        visibility: Visibility::Public,
        params: Vec::new(),
        return_type: None,
        docstring: None,
        decorators: Vec::new(),
        is_async: false,
        is_test: false,
    })
}

// Import extraction helpers
fn extract_rust_imports(tree: &Tree, source: &str) -> Vec<Import> {
    let mut imports = Vec::new();
    let mut cursor = tree.walk();

    fn traverse(
        node: &Node,
        source: &str,
        imports: &mut Vec<Import>,
        cursor: &mut tree_sitter::TreeCursor,
    ) {
        if node.kind() == "use_declaration" {
            if let Some(import) = parse_rust_use(node, source) {
                imports.push(import);
            }
        }

        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                traverse(&child, source, imports, cursor);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    let root = tree.root_node();
    traverse(&root, source, &mut imports, &mut cursor);
    imports
}

fn parse_rust_use(node: &Node, source: &str) -> Option<Import> {
    let text = get_node_text(node, source);
    let is_wildcard = text.contains("::*");
    Some(Import {
        module: text,
        items: Vec::new(),
        is_wildcard,
        line: node.start_position().row + 1,
    })
}

fn extract_python_imports(tree: &Tree, source: &str) -> Vec<Import> {
    let mut imports = Vec::new();
    let mut cursor = tree.walk();

    fn traverse(
        node: &Node,
        source: &str,
        imports: &mut Vec<Import>,
        cursor: &mut tree_sitter::TreeCursor,
    ) {
        match node.kind() {
            "import_statement" | "import_from_statement" => {
                if let Some(import) = parse_python_import(node, source) {
                    imports.push(import);
                }
            }
            _ => {}
        }

        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                traverse(&child, source, imports, cursor);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    let root = tree.root_node();
    traverse(&root, source, &mut imports, &mut cursor);
    imports
}

fn parse_python_import(node: &Node, source: &str) -> Option<Import> {
    let text = get_node_text(node, source);
    let is_wildcard = text.contains("*");
    Some(Import {
        module: text,
        items: Vec::new(),
        is_wildcard,
        line: node.start_position().row + 1,
    })
}

fn extract_ts_imports(tree: &Tree, source: &str) -> Vec<Import> {
    let mut imports = Vec::new();
    let mut cursor = tree.walk();

    fn traverse(
        node: &Node,
        source: &str,
        imports: &mut Vec<Import>,
        cursor: &mut tree_sitter::TreeCursor,
    ) {
        if node.kind() == "import_statement" {
            if let Some(import) = parse_ts_import(node, source) {
                imports.push(import);
            }
        }

        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                traverse(&child, source, imports, cursor);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    let root = tree.root_node();
    traverse(&root, source, &mut imports, &mut cursor);
    imports
}

fn parse_ts_import(node: &Node, source: &str) -> Option<Import> {
    let text = get_node_text(node, source);
    let is_wildcard = text.contains("*");
    Some(Import {
        module: text,
        items: Vec::new(),
        is_wildcard,
        line: node.start_position().row + 1,
    })
}

// Utility function to get node text
fn get_node_text(node: &Node, source: &str) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    source[start..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_parsing() {
        let code = r#"
            pub fn hello_world() -> String {
                "Hello, world!".to_string()
            }

            pub struct Person {
                name: String,
                age: u32,
            }
        "#;

        let mut parser = AstParser::new().unwrap();
        let tree = parser.parse(SupportedLanguage::Rust, code).unwrap();
        let symbols = parser.extract_symbols(&tree, SupportedLanguage::Rust, code);

        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "hello_world");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[1].name, "Person");
        assert_eq!(symbols[1].kind, SymbolKind::Struct);
    }

    #[test]
    fn test_python_parsing() {
        let code = r#"
def greet(name):
    """Greet someone"""
    return f"Hello, {name}!"

class Calculator:
    def add(self, a, b):
        return a + b
        "#;

        let mut parser = AstParser::new().unwrap();
        let tree = parser.parse(SupportedLanguage::Python, code).unwrap();
        let symbols = parser.extract_symbols(&tree, SupportedLanguage::Python, code);

        assert!(symbols.len() >= 2);
    }
}
