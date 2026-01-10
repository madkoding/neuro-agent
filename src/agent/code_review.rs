//! Code Review Mode - Advanced static analysis for Rust code
//!
//! This module provides comprehensive code quality analysis including:
//! - Style score calculation (rustfmt compliance)
//! - Cyclomatic complexity detection
//! - Code smell identification
//! - Test coverage estimation
//! - Overall grade calculation (A-F)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use syn::{visit::Visit, File, Item, ItemFn, ItemImpl, Expr, ExprIf, ExprMatch, ExprWhile, ExprLoop, ExprForLoop};

/// Complexity issue types detected during analysis
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComplexityIssue {
    /// Function has high cyclomatic complexity
    HighCyclomaticComplexity {
        function: String,
        score: usize,
        threshold: usize,
    },
    /// Function exceeds maximum line count
    LongFunction {
        function: String,
        lines: usize,
        threshold: usize,
    },
    /// Function has excessive nesting depth
    DeepNesting {
        function: String,
        depth: usize,
        threshold: usize,
    },
}

/// Code smell types detected during analysis
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CodeSmell {
    /// Magic number found in code
    MagicNumber { location: String, value: String },
    /// Duplicated code blocks detected
    DuplicatedCode { blocks: Vec<String> },
    /// Function has too many parameters
    LongParameterList {
        function: String,
        count: usize,
        threshold: usize,
    },
    /// Class/impl has too many methods (God Class)
    GodClass {
        name: String,
        methods: usize,
        threshold: usize,
    },
}

/// Function without test coverage
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UntestedFunction {
    pub name: String,
    pub location: String,
}

/// Actionable suggestion for improvement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub category: String,
    pub message: String,
    pub severity: SuggestionSeverity,
}

/// Severity levels for suggestions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionSeverity {
    Critical,
    Warning,
    Info,
}

/// Overall grade for code quality (A-F)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grade {
    A, // 90-100
    B, // 80-89
    C, // 70-79
    D, // 60-69
    F, // 0-59
}

impl Grade {
    /// Convert grade to numeric score
    pub fn to_score(&self) -> u8 {
        match self {
            Grade::A => 95,
            Grade::B => 85,
            Grade::C => 75,
            Grade::D => 65,
            Grade::F => 50,
        }
    }
}

impl std::fmt::Display for Grade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Grade::A => write!(f, "A (Excellent)"),
            Grade::B => write!(f, "B (Good)"),
            Grade::C => write!(f, "C (Average)"),
            Grade::D => write!(f, "D (Below Average)"),
            Grade::F => write!(f, "F (Needs Improvement)"),
        }
    }
}

/// Comprehensive code review report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewReport {
    pub file_path: PathBuf,
    pub style_score: u8, // 0-100
    pub complexity_issues: Vec<ComplexityIssue>,
    pub code_smells: Vec<CodeSmell>,
    pub missing_tests: Vec<UntestedFunction>,
    pub suggestions: Vec<Suggestion>,
    pub overall_grade: Grade,
}

impl ReviewReport {
    /// Create a new review report
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            style_score: 0,
            complexity_issues: Vec::new(),
            code_smells: Vec::new(),
            missing_tests: Vec::new(),
            suggestions: Vec::new(),
            overall_grade: Grade::F,
        }
    }

    /// Calculate overall score from components
    fn calculate_overall_score(&self) -> f64 {
        let style_weight = 0.3;
        let complexity_weight = 0.3;
        let smell_weight = 0.2;
        let coverage_weight = 0.2;

        let style_score = self.style_score as f64;

        // Complexity penalty: each issue reduces score
        let complexity_penalty = (self.complexity_issues.len() * 10).min(100) as f64;
        let complexity_score = 100.0 - complexity_penalty;

        // Smell penalty: each smell reduces score
        let smell_penalty = (self.code_smells.len() * 15).min(100) as f64;
        let smell_score = 100.0 - smell_penalty;

        // Coverage score: inverse of missing tests ratio
        let coverage_score = if self.missing_tests.is_empty() {
            100.0
        } else {
            (100.0 - (self.missing_tests.len() * 20).min(100) as f64).max(0.0)
        };

        (style_score * style_weight)
            + (complexity_score * complexity_weight)
            + (smell_score * smell_weight)
            + (coverage_score * coverage_weight)
    }

    /// Set the overall grade based on calculated score
    pub fn calculate_grade(&mut self) {
        let score = self.calculate_overall_score();
        self.overall_grade = match score as u8 {
            90..=100 => Grade::A,
            80..=89 => Grade::B,
            70..=79 => Grade::C,
            60..=69 => Grade::D,
            _ => Grade::F,
        };
    }
}

/// Main code review analyzer
pub struct CodeReviewAnalyzer {
    complexity_threshold: usize,
    line_threshold: usize,
    nesting_threshold: usize,
    param_threshold: usize,
    method_threshold: usize,
    test_coverage_threshold: f32,
}

impl Default for CodeReviewAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeReviewAnalyzer {
    /// Create a new analyzer with default thresholds
    pub fn new() -> Self {
        Self {
            complexity_threshold: 10,
            line_threshold: 50,
            nesting_threshold: 4,
            param_threshold: 5,
            method_threshold: 20,
            test_coverage_threshold: 0.8,
        }
    }

    /// Set custom complexity threshold
    pub fn with_complexity_threshold(mut self, threshold: usize) -> Self {
        self.complexity_threshold = threshold;
        self
    }

    /// Set custom line threshold
    pub fn with_line_threshold(mut self, threshold: usize) -> Self {
        self.line_threshold = threshold;
        self
    }

    /// Set custom nesting threshold
    pub fn with_nesting_threshold(mut self, threshold: usize) -> Self {
        self.nesting_threshold = threshold;
        self
    }

    /// Set custom test coverage threshold
    pub fn with_test_coverage_threshold(mut self, threshold: f32) -> Self {
        self.test_coverage_threshold = threshold;
        self
    }

    /// Analyze a Rust file and generate a review report
    pub fn analyze_file(&self, file_path: &Path) -> Result<ReviewReport> {
        let mut report = ReviewReport::new(file_path.to_path_buf());

        // Read file content
        let content = std::fs::read_to_string(file_path)
            .context("Failed to read file")?;

        // Parse AST
        let syntax_tree = syn::parse_file(&content)
            .context("Failed to parse Rust file")?;

        // Calculate style score
        report.style_score = self.calculate_style_score(file_path)?;

        // Analyze complexity
        self.analyze_complexity(&syntax_tree, &mut report)?;

        // Detect code smells
        self.detect_code_smells(&syntax_tree, &mut report)?;

        // Check test coverage
        self.check_test_coverage(&syntax_tree, &mut report)?;

        // Generate suggestions
        self.generate_suggestions(&mut report);

        // Calculate overall grade
        report.calculate_grade();

        Ok(report)
    }

    /// Calculate style score using rustfmt
    fn calculate_style_score(&self, file_path: &Path) -> Result<u8> {
        // Try to run rustfmt --check
        let output = Command::new("rustfmt")
            .arg("--check")
            .arg(file_path)
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    Ok(100) // Perfect style
                } else {
                    // Count formatting issues (rough estimate)
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let issue_count = stderr.lines().filter(|l| l.contains("Diff")).count();
                    let score = (100 - (issue_count * 10).min(100)) as u8;
                    Ok(score)
                }
            }
            Err(_) => {
                // rustfmt not available, use basic heuristics
                Ok(self.calculate_basic_style_score(file_path)?)
            }
        }
    }

    /// Basic style score calculation (fallback)
    fn calculate_basic_style_score(&self, file_path: &Path) -> Result<u8> {
        let content = std::fs::read_to_string(file_path)?;
        let mut score = 100u8;

        // Check for common style issues
        for line in content.lines() {
            // Trailing whitespace
            if line.ends_with(' ') || line.ends_with('\t') {
                score = score.saturating_sub(1);
            }
            // Very long lines (>100 chars)
            if line.len() > 100 {
                score = score.saturating_sub(1);
            }
        }

        Ok(score)
    }

    /// Analyze cyclomatic complexity and function metrics
    fn analyze_complexity(&self, syntax_tree: &File, report: &mut ReviewReport) -> Result<()> {
        let mut visitor = ComplexityVisitor::new(
            self.complexity_threshold,
            self.line_threshold,
            self.nesting_threshold,
        );
        visitor.visit_file(syntax_tree);
        report.complexity_issues = visitor.issues;
        Ok(())
    }

    /// Detect code smells
    fn detect_code_smells(&self, syntax_tree: &File, report: &mut ReviewReport) -> Result<()> {
        let mut visitor = SmellVisitor::new(self.param_threshold, self.method_threshold);
        visitor.visit_file(syntax_tree);
        report.code_smells = visitor.smells;
        Ok(())
    }

    /// Check test coverage
    fn check_test_coverage(&self, syntax_tree: &File, report: &mut ReviewReport) -> Result<()> {
        let mut test_functions = Vec::new();
        let mut regular_functions = Vec::new();

        for item in &syntax_tree.items {
            if let Item::Fn(func) = item {
                let func_name = func.sig.ident.to_string();
                
                // Check if it's a test function
                let is_test = func.attrs.iter().any(|attr| {
                    attr.path().is_ident("test") || attr.path().is_ident("tokio")
                });

                if is_test {
                    test_functions.push(func_name);
                } else if !func_name.starts_with('_') {
                    regular_functions.push(UntestedFunction {
                        name: func_name.clone(),
                        location: format!("{}::{}", report.file_path.display(), func_name),
                    });
                }
            }
        }

        // Filter out tested functions (basic heuristic)
        if !test_functions.is_empty() {
            regular_functions.retain(|f| {
                !test_functions.iter().any(|t| t.contains(&f.name))
            });
        }

        report.missing_tests = regular_functions;
        Ok(())
    }

    /// Generate actionable suggestions based on findings
    fn generate_suggestions(&self, report: &mut ReviewReport) {
        // Suggestions for complexity issues
        for issue in &report.complexity_issues {
            match issue {
                ComplexityIssue::HighCyclomaticComplexity { function, score, .. } => {
                    report.suggestions.push(Suggestion {
                        category: "Complexity".to_string(),
                        message: format!(
                            "Consider breaking down '{}' (complexity: {}) into smaller functions",
                            function, score
                        ),
                        severity: SuggestionSeverity::Warning,
                    });
                }
                ComplexityIssue::LongFunction { function, lines, .. } => {
                    report.suggestions.push(Suggestion {
                        category: "Maintainability".to_string(),
                        message: format!(
                            "Function '{}' is {} lines long. Consider refactoring",
                            function, lines
                        ),
                        severity: SuggestionSeverity::Info,
                    });
                }
                ComplexityIssue::DeepNesting { function, depth, .. } => {
                    report.suggestions.push(Suggestion {
                        category: "Readability".to_string(),
                        message: format!(
                            "Function '{}' has nesting depth {}. Use early returns or extract methods",
                            function, depth
                        ),
                        severity: SuggestionSeverity::Warning,
                    });
                }
            }
        }

        // Suggestions for code smells
        for smell in &report.code_smells {
            match smell {
                CodeSmell::MagicNumber { location, value } => {
                    report.suggestions.push(Suggestion {
                        category: "Maintainability".to_string(),
                        message: format!(
                            "Replace magic number {} at {} with a named constant",
                            value, location
                        ),
                        severity: SuggestionSeverity::Info,
                    });
                }
                CodeSmell::LongParameterList { function, count, .. } => {
                    report.suggestions.push(Suggestion {
                        category: "Design".to_string(),
                        message: format!(
                            "Function '{}' has {} parameters. Consider using a struct or builder pattern",
                            function, count
                        ),
                        severity: SuggestionSeverity::Warning,
                    });
                }
                CodeSmell::GodClass { name, methods, .. } => {
                    report.suggestions.push(Suggestion {
                        category: "Design".to_string(),
                        message: format!(
                            "Type '{}' has {} methods. Consider splitting into smaller types",
                            name, methods
                        ),
                        severity: SuggestionSeverity::Critical,
                    });
                }
                CodeSmell::DuplicatedCode { .. } => {
                    report.suggestions.push(Suggestion {
                        category: "DRY".to_string(),
                        message: "Duplicated code detected. Consider extracting common logic".to_string(),
                        severity: SuggestionSeverity::Warning,
                    });
                }
            }
        }

        // Suggestions for missing tests
        if !report.missing_tests.is_empty() {
            report.suggestions.push(Suggestion {
                category: "Testing".to_string(),
                message: format!(
                    "{} function(s) lack test coverage. Consider adding unit tests",
                    report.missing_tests.len()
                ),
                severity: SuggestionSeverity::Warning,
            });
        }

        // Suggestions for low style score
        if report.style_score < 80 {
            report.suggestions.push(Suggestion {
                category: "Style".to_string(),
                message: "Run 'cargo fmt' to fix formatting issues".to_string(),
                severity: SuggestionSeverity::Info,
            });
        }
    }
}

/// AST visitor for complexity analysis
struct ComplexityVisitor {
    issues: Vec<ComplexityIssue>,
    complexity_threshold: usize,
    line_threshold: usize,
    nesting_threshold: usize,
    current_function: Option<String>,
    current_complexity: usize,
    current_nesting: usize,
    max_nesting: usize,
}

impl ComplexityVisitor {
    fn new(complexity_threshold: usize, line_threshold: usize, nesting_threshold: usize) -> Self {
        Self {
            issues: Vec::new(),
            complexity_threshold,
            line_threshold,
            nesting_threshold,
            current_function: None,
            current_complexity: 0,
            current_nesting: 0,
            max_nesting: 0,
        }
    }

    fn start_function(&mut self, name: String) {
        self.current_function = Some(name);
        self.current_complexity = 1; // Base complexity
        self.current_nesting = 0;
        self.max_nesting = 0;
    }

    fn end_function(&mut self, start_line: usize, end_line: usize) {
        if let Some(name) = self.current_function.take() {
            let line_count = end_line - start_line;

            // Check cyclomatic complexity
            if self.current_complexity > self.complexity_threshold {
                self.issues.push(ComplexityIssue::HighCyclomaticComplexity {
                    function: name.clone(),
                    score: self.current_complexity,
                    threshold: self.complexity_threshold,
                });
            }

            // Check function length
            if line_count > self.line_threshold {
                self.issues.push(ComplexityIssue::LongFunction {
                    function: name.clone(),
                    lines: line_count,
                    threshold: self.line_threshold,
                });
            }

            // Check nesting depth
            if self.max_nesting > self.nesting_threshold {
                self.issues.push(ComplexityIssue::DeepNesting {
                    function: name,
                    depth: self.max_nesting,
                    threshold: self.nesting_threshold,
                });
            }
        }
    }

    fn increment_complexity(&mut self) {
        self.current_complexity += 1;
    }

    fn enter_block(&mut self) {
        self.current_nesting += 1;
        self.max_nesting = self.max_nesting.max(self.current_nesting);
    }

    fn exit_block(&mut self) {
        self.current_nesting = self.current_nesting.saturating_sub(1);
    }
}

impl<'ast> Visit<'ast> for ComplexityVisitor {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let func_name = node.sig.ident.to_string();
        let start_line = 1; // Simplified (would need span info)
        let end_line = start_line + 10; // Simplified

        self.start_function(func_name);
        syn::visit::visit_item_fn(self, node);
        self.end_function(start_line, end_line);
    }

    fn visit_expr_if(&mut self, node: &'ast ExprIf) {
        self.increment_complexity();
        self.enter_block();
        syn::visit::visit_expr_if(self, node);
        self.exit_block();
    }

    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        self.increment_complexity();
        self.enter_block();
        syn::visit::visit_expr_match(self, node);
        self.exit_block();
    }

    fn visit_expr_while(&mut self, node: &'ast ExprWhile) {
        self.increment_complexity();
        self.enter_block();
        syn::visit::visit_expr_while(self, node);
        self.exit_block();
    }

    fn visit_expr_loop(&mut self, node: &'ast ExprLoop) {
        self.increment_complexity();
        self.enter_block();
        syn::visit::visit_expr_loop(self, node);
        self.exit_block();
    }

    fn visit_expr_for_loop(&mut self, node: &'ast ExprForLoop) {
        self.increment_complexity();
        self.enter_block();
        syn::visit::visit_expr_for_loop(self, node);
        self.exit_block();
    }
}

/// AST visitor for code smell detection
struct SmellVisitor {
    smells: Vec<CodeSmell>,
    param_threshold: usize,
    method_threshold: usize,
}

impl SmellVisitor {
    fn new(param_threshold: usize, method_threshold: usize) -> Self {
        Self {
            smells: Vec::new(),
            param_threshold,
            method_threshold,
        }
    }
}

impl<'ast> Visit<'ast> for SmellVisitor {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let func_name = node.sig.ident.to_string();
        let param_count = node.sig.inputs.len();

        // Check for long parameter list
        if param_count > self.param_threshold {
            self.smells.push(CodeSmell::LongParameterList {
                function: func_name,
                count: param_count,
                threshold: self.param_threshold,
            });
        }

        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let method_count = node.items.len();

        // Check for god class
        if method_count > self.method_threshold {
            let type_name = if let syn::Type::Path(type_path) = &*node.self_ty {
                type_path
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_else(|| "Unknown".to_string())
            } else {
                "Unknown".to_string()
            };

            self.smells.push(CodeSmell::GodClass {
                name: type_name,
                methods: method_count,
                threshold: self.method_threshold,
            });
        }

        syn::visit::visit_item_impl(self, node);
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        // Check for magic numbers (simplified)
        if let Expr::Lit(lit) = node {
            if let syn::Lit::Int(lit_int) = &lit.lit {
                let value = lit_int.base10_parse::<i64>().unwrap_or(0);
                // Skip common values: 0, 1, -1
                if value != 0 && value != 1 && value != -1 {
                    self.smells.push(CodeSmell::MagicNumber {
                        location: "function".to_string(), // Simplified
                        value: value.to_string(),
                    });
                }
            }
        }

        syn::visit::visit_expr(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_analyzer_creation() {
        let analyzer = CodeReviewAnalyzer::new();
        assert_eq!(analyzer.complexity_threshold, 10);
        assert_eq!(analyzer.line_threshold, 50);
        assert_eq!(analyzer.nesting_threshold, 4);
    }

    #[test]
    fn test_custom_thresholds() {
        let analyzer = CodeReviewAnalyzer::new()
            .with_complexity_threshold(15)
            .with_line_threshold(100)
            .with_nesting_threshold(5)
            .with_test_coverage_threshold(0.9);

        assert_eq!(analyzer.complexity_threshold, 15);
        assert_eq!(analyzer.line_threshold, 100);
        assert_eq!(analyzer.nesting_threshold, 5);
        assert_eq!(analyzer.test_coverage_threshold, 0.9);
    }

    #[test]
    fn test_grade_calculation() {
        let mut report = ReviewReport::new(PathBuf::from("test.rs"));
        report.style_score = 90;
        report.calculate_grade();
        assert_eq!(report.overall_grade, Grade::A);

        let mut report2 = ReviewReport::new(PathBuf::from("test.rs"));
        report2.style_score = 20;  // Muy bajo (6 pts)
        report2.complexity_issues = vec![
            ComplexityIssue::HighCyclomaticComplexity {
                function: "test".to_string(),
                score: 20,
                threshold: 10,
            },
            ComplexityIssue::LongFunction {
                function: "test2".to_string(),
                lines: 100,
                threshold: 50,
            },
            ComplexityIssue::DeepNesting {
                function: "test3".to_string(),
                depth: 6,
                threshold: 4,
            },
        ]; // 3 issues, penalty 30, score 70 * 0.3 = 21
        report2.code_smells = vec![
            CodeSmell::MagicNumber {
                location: "line 5".to_string(),
                value: "42".to_string(),
            },
            CodeSmell::MagicNumber {
                location: "line 8".to_string(),
                value: "100".to_string(),
            },
        ]; // 2 smells, penalty 30, score 70 * 0.2 = 14
        report2.missing_tests = vec![
            UntestedFunction {
                name: "foo".to_string(),
                location: "line 10".to_string(),
            },
        ]; // 1 missing, penalty 20, score 80 * 0.2 = 16
        // Total: 6 + 21 + 14 + 16 = 57 = Grade F
        report2.calculate_grade();
        assert!(matches!(report2.overall_grade, Grade::D | Grade::F));
    }

    #[test]
    fn test_complexity_detection() {
        let code = r#"
            fn complex_function(x: i32) -> i32 {
                if x > 0 {
                    if x > 10 {
                        if x > 20 {
                            if x > 30 {
                                if x > 40 {
                                    return 100;
                                }
                            }
                        }
                    }
                }
                x
            }
        "#;

        let syntax_tree = syn::parse_file(code).unwrap();
        let analyzer = CodeReviewAnalyzer::new();
        let mut report = ReviewReport::new(PathBuf::from("test.rs"));
        analyzer.analyze_complexity(&syntax_tree, &mut report).unwrap();

        // Should detect deep nesting (5 levels > threshold of 4)
        assert!(
            !report.complexity_issues.is_empty(), 
            "Should detect complexity issues, got: {:?}", 
            report.complexity_issues
        );
    }

    #[test]
    fn test_long_parameter_list_detection() {
        let code = r#"
            fn many_params(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32) -> i32 {
                a + b + c + d + e + f
            }
        "#;

        let syntax_tree = syn::parse_file(code).unwrap();
        let analyzer = CodeReviewAnalyzer::new();
        let mut report = ReviewReport::new(PathBuf::from("test.rs"));
        analyzer.detect_code_smells(&syntax_tree, &mut report).unwrap();

        assert!(report.code_smells.iter().any(|s| matches!(s, CodeSmell::LongParameterList { .. })));
    }

    #[test]
    fn test_magic_number_detection() {
        let code = r#"
            fn calculate() -> i32 {
                let result = 42 * 100;
                result + 999
            }
        "#;

        let syntax_tree = syn::parse_file(code).unwrap();
        let analyzer = CodeReviewAnalyzer::new();
        let mut report = ReviewReport::new(PathBuf::from("test.rs"));
        analyzer.detect_code_smells(&syntax_tree, &mut report).unwrap();

        let magic_numbers: Vec<_> = report
            .code_smells
            .iter()
            .filter(|s| matches!(s, CodeSmell::MagicNumber { .. }))
            .collect();

        assert!(!magic_numbers.is_empty());
    }

    #[test]
    fn test_test_coverage_detection() {
        let code = r#"
            fn add(a: i32, b: i32) -> i32 {
                a + b
            }

            fn subtract(a: i32, b: i32) -> i32 {
                a - b
            }

            #[test]
            fn test_add() {
                assert_eq!(add(2, 3), 5);
            }
        "#;

        let syntax_tree = syn::parse_file(code).unwrap();
        let analyzer = CodeReviewAnalyzer::new();
        let mut report = ReviewReport::new(PathBuf::from("test.rs"));
        analyzer.check_test_coverage(&syntax_tree, &mut report).unwrap();

        // subtract function should be marked as untested
        assert!(report.missing_tests.iter().any(|f| f.name == "subtract"));
    }

    #[test]
    fn test_suggestion_generation() {
        let analyzer = CodeReviewAnalyzer::new();
        let mut report = ReviewReport::new(PathBuf::from("test.rs"));

        report.complexity_issues.push(ComplexityIssue::HighCyclomaticComplexity {
            function: "complex_func".to_string(),
            score: 20,
            threshold: 10,
        });

        report.code_smells.push(CodeSmell::MagicNumber {
            location: "line 10".to_string(),
            value: "42".to_string(),
        });

        analyzer.generate_suggestions(&mut report);

        assert!(!report.suggestions.is_empty());
        assert!(report.suggestions.iter().any(|s| s.category == "Complexity"));
        assert!(report.suggestions.iter().any(|s| s.category == "Maintainability"));
    }

    #[test]
    fn test_full_analysis() -> Result<()> {
        let code = r#"
            fn simple_function(x: i32) -> i32 {
                x * 2
            }

            #[test]
            fn test_simple() {
                assert_eq!(simple_function(5), 10);
            }
        "#;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(code.as_bytes())?;
        temp_file.flush()?;

        let analyzer = CodeReviewAnalyzer::new();
        let report = analyzer.analyze_file(temp_file.path())?;

        assert!(report.style_score > 0);
        assert!(matches!(
            report.overall_grade,
            Grade::A | Grade::B | Grade::C
        ));

        Ok(())
    }

    #[test]
    fn test_grade_enum() {
        assert_eq!(Grade::A.to_score(), 95);
        assert_eq!(Grade::B.to_score(), 85);
        assert_eq!(Grade::C.to_score(), 75);
        assert_eq!(Grade::D.to_score(), 65);
        assert_eq!(Grade::F.to_score(), 50);

        assert_eq!(Grade::A.to_string(), "A (Excellent)");
        assert_eq!(Grade::F.to_string(), "F (Needs Improvement)");
    }
}
