//! Project context tool - Maintain context about the current project

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Project context information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    pub name: String,
    pub root_path: PathBuf,
    pub project_type: ProjectType,
    pub description: Option<String>,
    pub version: Option<String>,
    pub language: PrimaryLanguage,
    pub frameworks: Vec<String>,
    pub dependencies_count: usize,
    pub file_count: usize,
    pub entry_points: Vec<String>,
    pub build_commands: HashMap<String, String>,
    pub test_commands: HashMap<String, String>,
    pub important_files: Vec<ImportantFile>,
    pub directories: DirectoryStructure,
    pub git_info: Option<GitInfo>,
}

/// Primary language of the project
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PrimaryLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    CSharp,
    Cpp,
    Ruby,
    Php,
    Unknown,
}

/// Project type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProjectType {
    Library,
    Binary,
    WebApp,
    Api,
    Cli,
    Monorepo,
    Unknown,
}

/// Important file in project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportantFile {
    pub path: String,
    pub file_type: ImportantFileType,
    pub description: String,
}

/// Type of important file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportantFileType {
    Config,
    EntryPoint,
    Readme,
    License,
    Changelog,
    ManifestPackage,
    BuildScript,
    DockerFile,
    CiConfig,
    Test,
}

/// Directory structure summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryStructure {
    pub source_dirs: Vec<String>,
    pub test_dirs: Vec<String>,
    pub config_dirs: Vec<String>,
    pub asset_dirs: Vec<String>,
    pub doc_dirs: Vec<String>,
}

/// Git information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub branch: String,
    pub remote_url: Option<String>,
    pub last_commit: Option<String>,
    pub is_dirty: bool,
}

/// Context summary for LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    pub brief: String,
    pub tech_stack: String,
    pub key_files: Vec<String>,
    pub structure: String,
}

/// Project context tool
#[derive(Debug, Clone)]
pub struct ProjectContextTool {
    context: Option<ProjectContext>,
}

impl Default for ProjectContextTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectContextTool {
    pub const NAME: &'static str = "project_context";

    pub fn new() -> Self {
        Self { context: None }
    }

    /// Analyze and build project context
    pub async fn analyze(&mut self, path: &str) -> Result<ProjectContext, ContextError> {
        let root = PathBuf::from(path);

        if !root.exists() {
            return Err(ContextError::PathNotFound(path.to_string()));
        }

        let (name, version, description) = self.detect_project_info(&root).await?;
        let language = self.detect_language(&root).await;
        let project_type = self.detect_project_type(&root, &language).await;
        let frameworks = self.detect_frameworks(&root, &language).await;
        let important_files = self.find_important_files(&root).await?;
        let directories = self.analyze_directories(&root).await?;
        let build_commands = self.detect_build_commands(&root, &language);
        let test_commands = self.detect_test_commands(&root, &language);
        let entry_points = self.find_entry_points(&root, &language).await?;
        let git_info = self.get_git_info(&root).await.ok();
        let dependencies_count = self.count_dependencies(&root, &language).await.unwrap_or(0);
        let file_count = self.count_files(&root).await.unwrap_or(0);

        let context = ProjectContext {
            name,
            root_path: root,
            project_type,
            description,
            version,
            language,
            frameworks,
            dependencies_count,
            file_count,
            entry_points,
            build_commands,
            test_commands,
            important_files,
            directories,
            git_info,
        };

        self.context = Some(context.clone());
        Ok(context)
    }

    async fn detect_project_info(
        &self,
        root: &Path,
    ) -> Result<(String, Option<String>, Option<String>), ContextError> {
        // Try Cargo.toml
        let cargo_path = root.join("Cargo.toml");
        if cargo_path.exists() {
            let content = fs::read_to_string(&cargo_path)
                .await
                .map_err(|e| ContextError::IoError(e.to_string()))?;
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                if let Some(package) = parsed.get("package") {
                    let name = package
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let version = package
                        .get("version")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let description = package
                        .get("description")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string());
                    return Ok((name, version, description));
                }
            }
        }

        // Try package.json
        let package_path = root.join("package.json");
        if package_path.exists() {
            let content = fs::read_to_string(&package_path)
                .await
                .map_err(|e| ContextError::IoError(e.to_string()))?;
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                let name = parsed
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let version = parsed
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let description = parsed
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string());
                return Ok((name, version, description));
            }
        }

        // Try pyproject.toml
        let pyproject_path = root.join("pyproject.toml");
        if pyproject_path.exists() {
            let content = fs::read_to_string(&pyproject_path)
                .await
                .map_err(|e| ContextError::IoError(e.to_string()))?;
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                if let Some(project) = parsed
                    .get("project")
                    .or_else(|| parsed.get("tool").and_then(|t| t.get("poetry")))
                {
                    let name = project
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let version = project
                        .get("version")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let description = project
                        .get("description")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string());
                    return Ok((name, version, description));
                }
            }
        }

        // Default to directory name
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Ok((name, None, None))
    }

    async fn detect_language(&self, root: &Path) -> PrimaryLanguage {
        if root.join("Cargo.toml").exists() {
            PrimaryLanguage::Rust
        } else if root.join("go.mod").exists() {
            PrimaryLanguage::Go
        } else if root.join("tsconfig.json").exists() {
            PrimaryLanguage::TypeScript
        } else if root.join("package.json").exists() {
            PrimaryLanguage::JavaScript
        } else if root.join("pyproject.toml").exists()
            || root.join("setup.py").exists()
            || root.join("requirements.txt").exists()
        {
            PrimaryLanguage::Python
        } else if root.join("pom.xml").exists() || root.join("build.gradle").exists() {
            PrimaryLanguage::Java
        } else if root.join("*.csproj").exists() {
            PrimaryLanguage::CSharp
        } else if root.join("Gemfile").exists() {
            PrimaryLanguage::Ruby
        } else if root.join("composer.json").exists() {
            PrimaryLanguage::Php
        } else {
            PrimaryLanguage::Unknown
        }
    }

    async fn detect_project_type(&self, root: &Path, language: &PrimaryLanguage) -> ProjectType {
        match language {
            PrimaryLanguage::Rust => {
                let cargo_path = root.join("Cargo.toml");
                if let Ok(content) = fs::read_to_string(&cargo_path).await {
                    if content.contains("[[bin]]") || content.contains("[bin]") {
                        if content.contains("clap") || content.contains("structopt") {
                            return ProjectType::Cli;
                        }
                        return ProjectType::Binary;
                    }
                    if content.contains("[lib]") {
                        return ProjectType::Library;
                    }
                    if content.contains("actix")
                        || content.contains("axum")
                        || content.contains("rocket")
                    {
                        return ProjectType::Api;
                    }
                }
            }
            PrimaryLanguage::JavaScript | PrimaryLanguage::TypeScript => {
                let package_path = root.join("package.json");
                if let Ok(content) = fs::read_to_string(&package_path).await {
                    if content.contains("next")
                        || content.contains("nuxt")
                        || content.contains("react")
                    {
                        return ProjectType::WebApp;
                    }
                    if content.contains("express")
                        || content.contains("fastify")
                        || content.contains("koa")
                    {
                        return ProjectType::Api;
                    }
                }
                if root.join("lerna.json").exists() || root.join("pnpm-workspace.yaml").exists() {
                    return ProjectType::Monorepo;
                }
            }
            PrimaryLanguage::Python => {
                if root.join("manage.py").exists() {
                    return ProjectType::WebApp;
                }
                if root.join("app.py").exists() || root.join("main.py").exists() {
                    return ProjectType::Api;
                }
            }
            _ => {}
        }

        ProjectType::Unknown
    }

    async fn detect_frameworks(&self, root: &Path, language: &PrimaryLanguage) -> Vec<String> {
        let mut frameworks = Vec::new();

        match language {
            PrimaryLanguage::Rust => {
                if let Ok(content) = fs::read_to_string(root.join("Cargo.toml")).await {
                    let framework_checks = [
                        ("tokio", "Tokio"),
                        ("actix", "Actix"),
                        ("axum", "Axum"),
                        ("rocket", "Rocket"),
                        ("warp", "Warp"),
                        ("serde", "Serde"),
                        ("diesel", "Diesel"),
                        ("sqlx", "SQLx"),
                        ("clap", "Clap"),
                        ("ratatui", "Ratatui"),
                    ];
                    for (check, name) in framework_checks {
                        if content.contains(check) {
                            frameworks.push(name.to_string());
                        }
                    }
                }
            }
            PrimaryLanguage::JavaScript | PrimaryLanguage::TypeScript => {
                if let Ok(content) = fs::read_to_string(root.join("package.json")).await {
                    let framework_checks = [
                        ("react", "React"),
                        ("vue", "Vue"),
                        ("angular", "Angular"),
                        ("next", "Next.js"),
                        ("nuxt", "Nuxt"),
                        ("express", "Express"),
                        ("fastify", "Fastify"),
                        ("nest", "NestJS"),
                        ("prisma", "Prisma"),
                        ("mongoose", "Mongoose"),
                    ];
                    for (check, name) in framework_checks {
                        if content.contains(check) {
                            frameworks.push(name.to_string());
                        }
                    }
                }
            }
            PrimaryLanguage::Python => {
                let paths_to_check = [
                    root.join("pyproject.toml"),
                    root.join("requirements.txt"),
                    root.join("setup.py"),
                ];

                let mut content = String::new();
                for path in &paths_to_check {
                    if let Ok(c) = fs::read_to_string(path).await {
                        content.push_str(&c);
                    }
                }

                let framework_checks = [
                    ("django", "Django"),
                    ("flask", "Flask"),
                    ("fastapi", "FastAPI"),
                    ("sqlalchemy", "SQLAlchemy"),
                    ("pytest", "Pytest"),
                    ("pandas", "Pandas"),
                    ("numpy", "NumPy"),
                    ("torch", "PyTorch"),
                    ("tensorflow", "TensorFlow"),
                ];
                for (check, name) in framework_checks {
                    if content.to_lowercase().contains(check) {
                        frameworks.push(name.to_string());
                    }
                }
            }
            _ => {}
        }

        frameworks
    }

    async fn find_important_files(&self, root: &Path) -> Result<Vec<ImportantFile>, ContextError> {
        let mut files = Vec::new();

        let checks = [
            (
                "README.md",
                ImportantFileType::Readme,
                "Project documentation",
            ),
            (
                "README.rst",
                ImportantFileType::Readme,
                "Project documentation",
            ),
            ("LICENSE", ImportantFileType::License, "License file"),
            ("LICENSE.md", ImportantFileType::License, "License file"),
            (
                "CHANGELOG.md",
                ImportantFileType::Changelog,
                "Change history",
            ),
            ("CHANGES.md", ImportantFileType::Changelog, "Change history"),
            (
                "Cargo.toml",
                ImportantFileType::ManifestPackage,
                "Rust package manifest",
            ),
            (
                "package.json",
                ImportantFileType::ManifestPackage,
                "Node.js package manifest",
            ),
            (
                "pyproject.toml",
                ImportantFileType::ManifestPackage,
                "Python package manifest",
            ),
            (
                "go.mod",
                ImportantFileType::ManifestPackage,
                "Go module file",
            ),
            (
                "Dockerfile",
                ImportantFileType::DockerFile,
                "Docker configuration",
            ),
            (
                "docker-compose.yml",
                ImportantFileType::DockerFile,
                "Docker Compose configuration",
            ),
            (
                ".github/workflows",
                ImportantFileType::CiConfig,
                "GitHub Actions",
            ),
            (".gitlab-ci.yml", ImportantFileType::CiConfig, "GitLab CI"),
            (
                "Makefile",
                ImportantFileType::BuildScript,
                "Build automation",
            ),
            (
                "build.rs",
                ImportantFileType::BuildScript,
                "Rust build script",
            ),
        ];

        for (file, file_type, description) in checks {
            let path = root.join(file);
            if path.exists() {
                files.push(ImportantFile {
                    path: file.to_string(),
                    file_type,
                    description: description.to_string(),
                });
            }
        }

        Ok(files)
    }

    async fn analyze_directories(&self, root: &Path) -> Result<DirectoryStructure, ContextError> {
        let mut structure = DirectoryStructure {
            source_dirs: Vec::new(),
            test_dirs: Vec::new(),
            config_dirs: Vec::new(),
            asset_dirs: Vec::new(),
            doc_dirs: Vec::new(),
        };

        let source_patterns = ["src", "lib", "app", "packages"];
        let test_patterns = ["tests", "test", "__tests__", "spec"];
        let config_patterns = [".config", "config", ".github", ".vscode"];
        let asset_patterns = ["assets", "static", "public", "resources"];
        let doc_patterns = ["docs", "doc", "documentation"];

        let mut read_dir = fs::read_dir(root)
            .await
            .map_err(|e| ContextError::IoError(e.to_string()))?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| ContextError::IoError(e.to_string()))?
        {
            if !entry.path().is_dir() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();

            if source_patterns.iter().any(|p| name.eq_ignore_ascii_case(p)) {
                structure.source_dirs.push(name.clone());
            }
            if test_patterns.iter().any(|p| name.eq_ignore_ascii_case(p)) {
                structure.test_dirs.push(name.clone());
            }
            if config_patterns.iter().any(|p| name.eq_ignore_ascii_case(p)) {
                structure.config_dirs.push(name.clone());
            }
            if asset_patterns.iter().any(|p| name.eq_ignore_ascii_case(p)) {
                structure.asset_dirs.push(name.clone());
            }
            if doc_patterns.iter().any(|p| name.eq_ignore_ascii_case(p)) {
                structure.doc_dirs.push(name.clone());
            }
        }

        Ok(structure)
    }

    fn detect_build_commands(
        &self,
        _root: &Path,
        language: &PrimaryLanguage,
    ) -> HashMap<String, String> {
        let mut commands = HashMap::new();

        match language {
            PrimaryLanguage::Rust => {
                commands.insert("build".to_string(), "cargo build".to_string());
                commands.insert("release".to_string(), "cargo build --release".to_string());
                commands.insert("check".to_string(), "cargo check".to_string());
            }
            PrimaryLanguage::JavaScript | PrimaryLanguage::TypeScript => {
                commands.insert("build".to_string(), "npm run build".to_string());
                commands.insert("dev".to_string(), "npm run dev".to_string());
            }
            PrimaryLanguage::Python => {
                commands.insert("build".to_string(), "python -m build".to_string());
            }
            PrimaryLanguage::Go => {
                commands.insert("build".to_string(), "go build".to_string());
            }
            _ => {}
        }

        commands
    }

    fn detect_test_commands(
        &self,
        _root: &Path,
        language: &PrimaryLanguage,
    ) -> HashMap<String, String> {
        let mut commands = HashMap::new();

        match language {
            PrimaryLanguage::Rust => {
                commands.insert("test".to_string(), "cargo test".to_string());
                commands.insert(
                    "test_verbose".to_string(),
                    "cargo test -- --nocapture".to_string(),
                );
            }
            PrimaryLanguage::JavaScript | PrimaryLanguage::TypeScript => {
                commands.insert("test".to_string(), "npm test".to_string());
            }
            PrimaryLanguage::Python => {
                commands.insert("test".to_string(), "pytest".to_string());
                commands.insert("test_verbose".to_string(), "pytest -v".to_string());
            }
            PrimaryLanguage::Go => {
                commands.insert("test".to_string(), "go test ./...".to_string());
            }
            _ => {}
        }

        commands
    }

    async fn find_entry_points(
        &self,
        root: &Path,
        language: &PrimaryLanguage,
    ) -> Result<Vec<String>, ContextError> {
        let mut entry_points = Vec::new();

        let patterns: &[&str] = match language {
            PrimaryLanguage::Rust => &["src/main.rs", "src/lib.rs"],
            PrimaryLanguage::Python => &["main.py", "app.py", "__main__.py", "src/__main__.py"],
            PrimaryLanguage::JavaScript => &["index.js", "src/index.js", "app.js", "server.js"],
            PrimaryLanguage::TypeScript => &["index.ts", "src/index.ts", "app.ts", "server.ts"],
            PrimaryLanguage::Go => &["main.go", "cmd/main.go"],
            _ => &[],
        };

        for pattern in patterns {
            if root.join(pattern).exists() {
                entry_points.push(pattern.to_string());
            }
        }

        Ok(entry_points)
    }

    async fn get_git_info(&self, root: &Path) -> Result<GitInfo, ContextError> {
        use tokio::process::Command;

        let branch = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(root)
            .output()
            .await
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        let remote_url = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(root)
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let last_commit = Command::new("git")
            .args(["log", "-1", "--format=%s"])
            .current_dir(root)
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(root)
            .output()
            .await
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false);

        Ok(GitInfo {
            branch,
            remote_url,
            last_commit,
            is_dirty: status,
        })
    }

    async fn count_dependencies(
        &self,
        root: &Path,
        language: &PrimaryLanguage,
    ) -> Result<usize, ContextError> {
        match language {
            PrimaryLanguage::Rust => {
                let content = fs::read_to_string(root.join("Cargo.toml"))
                    .await
                    .map_err(|e| ContextError::IoError(e.to_string()))?;
                let parsed: toml::Value = toml::from_str(&content)
                    .map_err(|e| ContextError::ParseError(e.to_string()))?;

                let deps = parsed
                    .get("dependencies")
                    .and_then(|d| d.as_table())
                    .map(|t| t.len())
                    .unwrap_or(0);
                let dev_deps = parsed
                    .get("dev-dependencies")
                    .and_then(|d| d.as_table())
                    .map(|t| t.len())
                    .unwrap_or(0);

                Ok(deps + dev_deps)
            }
            PrimaryLanguage::JavaScript | PrimaryLanguage::TypeScript => {
                let content = fs::read_to_string(root.join("package.json"))
                    .await
                    .map_err(|e| ContextError::IoError(e.to_string()))?;
                let parsed: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| ContextError::ParseError(e.to_string()))?;

                let deps = parsed
                    .get("dependencies")
                    .and_then(|d| d.as_object())
                    .map(|o| o.len())
                    .unwrap_or(0);
                let dev_deps = parsed
                    .get("devDependencies")
                    .and_then(|d| d.as_object())
                    .map(|o| o.len())
                    .unwrap_or(0);

                Ok(deps + dev_deps)
            }
            _ => Ok(0),
        }
    }

    async fn count_files(&self, root: &Path) -> Result<usize, ContextError> {
        let mut count = 0;
        self.count_files_recursive(root, &mut count).await?;
        Ok(count)
    }

    async fn count_files_recursive(
        &self,
        path: &Path,
        count: &mut usize,
    ) -> Result<(), ContextError> {
        let mut read_dir = fs::read_dir(path)
            .await
            .map_err(|e| ContextError::IoError(e.to_string()))?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| ContextError::IoError(e.to_string()))?
        {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden and generated dirs
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "__pycache__"
            {
                continue;
            }

            let entry_path = entry.path();
            if entry_path.is_dir() {
                Box::pin(self.count_files_recursive(&entry_path, count)).await?;
            } else {
                *count += 1;
            }
        }

        Ok(())
    }

    /// Get current context
    pub fn get_context(&self) -> Option<&ProjectContext> {
        self.context.as_ref()
    }

    /// Generate a summary for LLM consumption
    pub fn generate_summary(&self) -> Option<ContextSummary> {
        let context = self.context.as_ref()?;

        let brief = format!(
            "{} is a {:?} {:?} project{}",
            context.name,
            context.language,
            context.project_type,
            context
                .description
                .as_ref()
                .map(|d| format!(": {}", d))
                .unwrap_or_default()
        );

        let tech_stack = if context.frameworks.is_empty() {
            format!("{:?}", context.language)
        } else {
            format!(
                "{:?} with {}",
                context.language,
                context.frameworks.join(", ")
            )
        };

        let key_files: Vec<String> = context
            .important_files
            .iter()
            .map(|f| f.path.clone())
            .collect();

        let structure = format!(
            "Source: {:?}, Tests: {:?}, {} files, {} dependencies",
            context.directories.source_dirs,
            context.directories.test_dirs,
            context.file_count,
            context.dependencies_count
        );

        Some(ContextSummary {
            brief,
            tech_stack,
            key_files,
            structure,
        })
    }
}

/// Context errors
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_context_detection() {
        // Would need actual project for real test
    }
}
