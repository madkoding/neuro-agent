//! Test runner tool - Discover and execute tests

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// Test framework
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TestFramework {
    Cargo,   // Rust
    Pytest,  // Python
    Jest,    // JavaScript/TypeScript
    Mocha,   // JavaScript
    Go,      // Go
    PHPUnit, // PHP
    RSpec,   // Ruby
    JUnit,   // Java
    Unknown,
}

/// Test result status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Error,
}

/// Individual test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub name: String,
    pub status: TestStatus,
    pub duration_ms: Option<u64>,
    pub message: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

/// Test run summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub framework: TestFramework,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub duration_ms: u64,
    pub success: bool,
}

/// Test run output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestOutput {
    pub summary: TestSummary,
    pub tests: Vec<TestCase>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Test run arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestArgs {
    pub path: String,
    pub filter: Option<String>,
    pub framework: Option<TestFramework>,
    pub verbose: Option<bool>,
    pub coverage: Option<bool>,
    pub watch: Option<bool>,
    pub parallel: Option<bool>,
}

/// Test runner tool
#[derive(Debug, Clone)]
pub struct TestRunnerTool;

impl Default for TestRunnerTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TestRunnerTool {
    pub const NAME: &'static str = "run_tests";

    pub fn new() -> Self {
        Self
    }

    /// Detect test framework for a project
    pub fn detect_framework(&self, path: &Path) -> TestFramework {
        if path.join("Cargo.toml").exists() {
            TestFramework::Cargo
        } else if path.join("pytest.ini").exists()
            || path.join("pyproject.toml").exists()
            || path.join("setup.py").exists()
        {
            TestFramework::Pytest
        } else if path.join("jest.config.js").exists() || path.join("jest.config.ts").exists() {
            TestFramework::Jest
        } else if path.join("package.json").exists() {
            // Check package.json for test framework
            if let Ok(content) = std::fs::read_to_string(path.join("package.json")) {
                if content.contains("\"jest\"") {
                    return TestFramework::Jest;
                } else if content.contains("\"mocha\"") {
                    return TestFramework::Mocha;
                }
            }
            TestFramework::Jest // Default for Node projects
        } else if path.join("go.mod").exists() {
            TestFramework::Go
        } else if path.join("phpunit.xml").exists() {
            TestFramework::PHPUnit
        } else if path.join("Gemfile").exists() {
            TestFramework::RSpec
        } else if path.join("pom.xml").exists() || path.join("build.gradle").exists() {
            TestFramework::JUnit
        } else {
            TestFramework::Unknown
        }
    }

    /// Run tests
    pub async fn run(&self, args: TestArgs) -> Result<TestOutput, TestError> {
        let path = PathBuf::from(&args.path);

        if !path.exists() {
            return Err(TestError::PathNotFound(args.path));
        }

        let framework = args
            .framework
            .clone()
            .unwrap_or_else(|| self.detect_framework(&path));

        match framework {
            TestFramework::Cargo => self.run_cargo_tests(&path, &args).await,
            TestFramework::Pytest => self.run_pytest(&path, &args).await,
            TestFramework::Jest => self.run_jest(&path, &args).await,
            TestFramework::Mocha => self.run_mocha(&path, &args).await,
            TestFramework::Go => self.run_go_tests(&path, &args).await,
            TestFramework::PHPUnit => self.run_phpunit(&path, &args).await,
            TestFramework::RSpec => self.run_rspec(&path, &args).await,
            TestFramework::JUnit => self.run_junit(&path, &args).await,
            TestFramework::Unknown => Err(TestError::UnknownFramework),
        }
    }

    async fn run_cargo_tests(&self, path: &Path, args: &TestArgs) -> Result<TestOutput, TestError> {
        let mut cmd = Command::new("cargo");
        cmd.arg("test");

        if args.verbose.unwrap_or(false) {
            cmd.arg("--verbose");
        }

        if let Some(ref filter) = args.filter {
            cmd.arg(filter);
        }

        // Always show output
        cmd.arg("--").arg("--nocapture");

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let output = cmd
            .output()
            .await
            .map_err(|e| TestError::ExecutionError(e.to_string()))?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Parse Rust test output
        let (tests, summary) = parse_cargo_output(&stdout, &stderr, duration_ms);

        Ok(TestOutput {
            summary,
            tests,
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_pytest(&self, path: &Path, args: &TestArgs) -> Result<TestOutput, TestError> {
        let mut cmd = Command::new("python");
        cmd.args(["-m", "pytest"]);

        if args.verbose.unwrap_or(false) {
            cmd.arg("-v");
        }

        if let Some(ref filter) = args.filter {
            cmd.arg("-k").arg(filter);
        }

        if args.coverage.unwrap_or(false) {
            cmd.args(["--cov", "."]);
        }

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let output = cmd
            .output()
            .await
            .map_err(|e| TestError::ExecutionError(e.to_string()))?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let (tests, summary) = parse_pytest_output(&stdout, duration_ms);

        Ok(TestOutput {
            summary,
            tests,
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_jest(&self, path: &Path, args: &TestArgs) -> Result<TestOutput, TestError> {
        let mut cmd = Command::new("npx");
        cmd.arg("jest");

        if args.verbose.unwrap_or(false) {
            cmd.arg("--verbose");
        }

        if let Some(ref filter) = args.filter {
            cmd.arg("--testNamePattern").arg(filter);
        }

        if args.coverage.unwrap_or(false) {
            cmd.arg("--coverage");
        }

        cmd.arg("--json");
        cmd.arg("--outputFile=/tmp/jest-results.json");

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let output = cmd
            .output()
            .await
            .map_err(|e| TestError::ExecutionError(e.to_string()))?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let (tests, summary) = parse_jest_output(&stdout, duration_ms);

        Ok(TestOutput {
            summary,
            tests,
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_mocha(&self, path: &Path, args: &TestArgs) -> Result<TestOutput, TestError> {
        let mut cmd = Command::new("npx");
        cmd.arg("mocha");

        if let Some(ref filter) = args.filter {
            cmd.arg("--grep").arg(filter);
        }

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let output = cmd
            .output()
            .await
            .map_err(|e| TestError::ExecutionError(e.to_string()))?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let success = output.status.success();
        let summary = TestSummary {
            framework: TestFramework::Mocha,
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            errors: 0,
            duration_ms,
            success,
        };

        Ok(TestOutput {
            summary,
            tests: vec![],
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_go_tests(&self, path: &Path, args: &TestArgs) -> Result<TestOutput, TestError> {
        let mut cmd = Command::new("go");
        cmd.arg("test");

        if args.verbose.unwrap_or(false) {
            cmd.arg("-v");
        }

        if let Some(ref filter) = args.filter {
            cmd.arg("-run").arg(filter);
        }

        cmd.arg("./...");

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let output = cmd
            .output()
            .await
            .map_err(|e| TestError::ExecutionError(e.to_string()))?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let (tests, summary) = parse_go_output(&stdout, duration_ms);

        Ok(TestOutput {
            summary,
            tests,
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_phpunit(&self, path: &Path, args: &TestArgs) -> Result<TestOutput, TestError> {
        let mut cmd = Command::new("./vendor/bin/phpunit");

        if let Some(ref filter) = args.filter {
            cmd.arg("--filter").arg(filter);
        }

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let output = cmd
            .output()
            .await
            .map_err(|e| TestError::ExecutionError(e.to_string()))?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let success = output.status.success();
        let summary = TestSummary {
            framework: TestFramework::PHPUnit,
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            errors: 0,
            duration_ms,
            success,
        };

        Ok(TestOutput {
            summary,
            tests: vec![],
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_rspec(&self, path: &Path, args: &TestArgs) -> Result<TestOutput, TestError> {
        let mut cmd = Command::new("bundle");
        cmd.args(["exec", "rspec"]);

        if let Some(ref filter) = args.filter {
            cmd.arg("--example").arg(filter);
        }

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let output = cmd
            .output()
            .await
            .map_err(|e| TestError::ExecutionError(e.to_string()))?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let success = output.status.success();
        let summary = TestSummary {
            framework: TestFramework::RSpec,
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            errors: 0,
            duration_ms,
            success,
        };

        Ok(TestOutput {
            summary,
            tests: vec![],
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_junit(&self, path: &Path, _args: &TestArgs) -> Result<TestOutput, TestError> {
        // Check for Maven or Gradle
        let (cmd_name, cmd_args) = if path.join("pom.xml").exists() {
            ("mvn", vec!["test"])
        } else {
            ("./gradlew", vec!["test"])
        };

        let mut cmd = Command::new(cmd_name);
        cmd.args(&cmd_args);

        cmd.current_dir(path);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = std::time::Instant::now();
        let output = cmd
            .output()
            .await
            .map_err(|e| TestError::ExecutionError(e.to_string()))?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let success = output.status.success();
        let summary = TestSummary {
            framework: TestFramework::JUnit,
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            errors: 0,
            duration_ms,
            success,
        };

        Ok(TestOutput {
            summary,
            tests: vec![],
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// List available tests without running them
    pub async fn list_tests(
        &self,
        path: &str,
        framework: Option<TestFramework>,
    ) -> Result<Vec<String>, TestError> {
        let path = PathBuf::from(path);
        let framework = framework.unwrap_or_else(|| self.detect_framework(&path));

        match framework {
            TestFramework::Cargo => {
                let output = Command::new("cargo")
                    .args(["test", "--", "--list"])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| TestError::ExecutionError(e.to_string()))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                Ok(stdout
                    .lines()
                    .filter(|l| l.ends_with(": test"))
                    .map(|l| l.trim_end_matches(": test").to_string())
                    .collect())
            }
            TestFramework::Pytest => {
                let output = Command::new("python")
                    .args(["-m", "pytest", "--collect-only", "-q"])
                    .current_dir(&path)
                    .output()
                    .await
                    .map_err(|e| TestError::ExecutionError(e.to_string()))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                Ok(stdout
                    .lines()
                    .filter(|l| l.contains("::"))
                    .map(|l| l.to_string())
                    .collect())
            }
            _ => Ok(vec![]),
        }
    }
}

fn parse_cargo_output(
    stdout: &str,
    _stderr: &str,
    duration_ms: u64,
) -> (Vec<TestCase>, TestSummary) {
    let mut tests = Vec::new();
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for line in stdout.lines() {
        if line.starts_with("test ")
            && (line.contains(" ... ok")
                || line.contains(" ... FAILED")
                || line.contains(" ... ignored"))
        {
            let parts: Vec<&str> = line.split(" ... ").collect();
            if parts.len() == 2 {
                let name = parts[0].trim_start_matches("test ").to_string();
                let status = match parts[1].trim() {
                    "ok" => {
                        passed += 1;
                        TestStatus::Passed
                    }
                    "FAILED" => {
                        failed += 1;
                        TestStatus::Failed
                    }
                    "ignored" => {
                        skipped += 1;
                        TestStatus::Skipped
                    }
                    _ => TestStatus::Error,
                };
                tests.push(TestCase {
                    name,
                    status,
                    duration_ms: None,
                    message: None,
                    file: None,
                    line: None,
                });
            }
        }
    }

    let total = tests.len();
    let summary = TestSummary {
        framework: TestFramework::Cargo,
        total,
        passed,
        failed,
        skipped,
        errors: 0,
        duration_ms,
        success: failed == 0,
    };

    (tests, summary)
}

fn parse_pytest_output(stdout: &str, duration_ms: u64) -> (Vec<TestCase>, TestSummary) {
    let mut tests = Vec::new();
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for line in stdout.lines() {
        if line.contains("PASSED") {
            passed += 1;
            let name = line.split("PASSED").next().unwrap_or("").trim().to_string();
            tests.push(TestCase {
                name,
                status: TestStatus::Passed,
                duration_ms: None,
                message: None,
                file: None,
                line: None,
            });
        } else if line.contains("FAILED") {
            failed += 1;
            let name = line.split("FAILED").next().unwrap_or("").trim().to_string();
            tests.push(TestCase {
                name,
                status: TestStatus::Failed,
                duration_ms: None,
                message: None,
                file: None,
                line: None,
            });
        } else if line.contains("SKIPPED") {
            skipped += 1;
        }
    }

    let total = tests.len();
    let summary = TestSummary {
        framework: TestFramework::Pytest,
        total,
        passed,
        failed,
        skipped,
        errors: 0,
        duration_ms,
        success: failed == 0,
    };

    (tests, summary)
}

fn parse_jest_output(stdout: &str, duration_ms: u64) -> (Vec<TestCase>, TestSummary) {
    // Jest outputs JSON, try to parse it
    let mut passed = 0;
    let mut failed = 0;

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(num_passed) = json.get("numPassedTests").and_then(|v| v.as_u64()) {
            passed = num_passed as usize;
        }
        if let Some(num_failed) = json.get("numFailedTests").and_then(|v| v.as_u64()) {
            failed = num_failed as usize;
        }
    }

    let total = passed + failed;
    let summary = TestSummary {
        framework: TestFramework::Jest,
        total,
        passed,
        failed,
        skipped: 0,
        errors: 0,
        duration_ms,
        success: failed == 0,
    };

    (vec![], summary)
}

fn parse_go_output(stdout: &str, duration_ms: u64) -> (Vec<TestCase>, TestSummary) {
    let mut tests = Vec::new();
    let mut passed = 0;
    let mut failed = 0;

    for line in stdout.lines() {
        if line.starts_with("--- PASS:") {
            passed += 1;
            let name = line.split_whitespace().nth(2).unwrap_or("").to_string();
            tests.push(TestCase {
                name,
                status: TestStatus::Passed,
                duration_ms: None,
                message: None,
                file: None,
                line: None,
            });
        } else if line.starts_with("--- FAIL:") {
            failed += 1;
            let name = line.split_whitespace().nth(2).unwrap_or("").to_string();
            tests.push(TestCase {
                name,
                status: TestStatus::Failed,
                duration_ms: None,
                message: None,
                file: None,
                line: None,
            });
        }
    }

    let total = tests.len();
    let summary = TestSummary {
        framework: TestFramework::Go,
        total,
        passed,
        failed,
        skipped: 0,
        errors: 0,
        duration_ms,
        success: failed == 0,
    };

    (tests, summary)
}

/// Test runner errors
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("Unknown test framework")]
    UnknownFramework,
    #[error("Execution error: {0}")]
    ExecutionError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cargo_output_parsing() {
        let stdout = r#"
running 3 tests
test tests::test_one ... ok
test tests::test_two ... FAILED
test tests::test_three ... ignored
"#;
        let (tests, summary) = parse_cargo_output(stdout, "", 1000);
        assert_eq!(tests.len(), 3);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 1);
    }
}
