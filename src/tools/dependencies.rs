//! Dependency analyzer - Analyze project dependencies

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Dependency info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub version_req: Option<String>,
    pub is_dev: bool,
    pub is_optional: bool,
    pub features: Vec<String>,
    pub source: DependencySource,
}

/// Dependency source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencySource {
    Registry(String), // crates.io, npm, pypi
    Git { url: String, branch: Option<String> },
    Path(String),
    Unknown,
}

/// Dependency analysis output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyAnalysis {
    pub project_type: ProjectType,
    pub dependencies: Vec<Dependency>,
    pub dev_dependencies: Vec<Dependency>,
    pub total_count: usize,
    pub direct_count: usize,
    pub outdated: Vec<OutdatedDependency>,
    pub security_issues: Vec<SecurityIssue>,
    pub duplicate_deps: Vec<String>,
}

/// Project type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Unknown,
}

/// Outdated dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutdatedDependency {
    pub name: String,
    pub current: String,
    pub latest: String,
    pub is_major: bool,
}

/// Security issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssue {
    pub dependency: String,
    pub severity: String,
    pub description: String,
    pub advisory_url: Option<String>,
}

/// Dependency analyzer tool
#[derive(Debug, Clone)]
pub struct DependencyAnalyzerTool;

impl DependencyAnalyzerTool {
    pub const NAME: &'static str = "analyze_dependencies";

    /// Analyze project dependencies
    pub async fn analyze(&self, args: AnalyzeDepsArgs) -> Result<DependencyAnalysis, DepsError> {
        let path = PathBuf::from(&args.path);
        
        if !path.exists() {
            return Err(DepsError::PathNotFound(args.path));
        }

        // Detect project type
        let project_type = detect_project_type(&path);
        
        match project_type {
            ProjectType::Rust => self.analyze_rust(&path).await,
            ProjectType::Node => self.analyze_node(&path).await,
            ProjectType::Python => self.analyze_python(&path).await,
            ProjectType::Go => self.analyze_go(&path).await,
            ProjectType::Unknown => Err(DepsError::UnknownProjectType),
        }
    }

    async fn analyze_rust(&self, path: &Path) -> Result<DependencyAnalysis, DepsError> {
        let cargo_toml = path.join("Cargo.toml");
        let content = fs::read_to_string(&cargo_toml).await
            .map_err(|e| DepsError::IoError(e.to_string()))?;

        let parsed: toml::Value = toml::from_str(&content)
            .map_err(|e| DepsError::ParseError(e.to_string()))?;

        let mut dependencies = Vec::new();
        let mut dev_dependencies = Vec::new();

        // Parse [dependencies]
        if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_table()) {
            for (name, value) in deps {
                let dep = parse_rust_dependency(name, value, false);
                dependencies.push(dep);
            }
        }

        // Parse [dev-dependencies]
        if let Some(deps) = parsed.get("dev-dependencies").and_then(|d| d.as_table()) {
            for (name, value) in deps {
                let dep = parse_rust_dependency(name, value, true);
                dev_dependencies.push(dep);
            }
        }

        let total_count = dependencies.len() + dev_dependencies.len();

        Ok(DependencyAnalysis {
            project_type: ProjectType::Rust,
            dependencies,
            dev_dependencies,
            total_count,
            direct_count: total_count,
            outdated: vec![],
            security_issues: vec![],
            duplicate_deps: vec![],
        })
    }

    async fn analyze_node(&self, path: &Path) -> Result<DependencyAnalysis, DepsError> {
        let package_json = path.join("package.json");
        let content = fs::read_to_string(&package_json).await
            .map_err(|e| DepsError::IoError(e.to_string()))?;

        let parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| DepsError::ParseError(e.to_string()))?;

        let mut dependencies = Vec::new();
        let mut dev_dependencies = Vec::new();

        // Parse dependencies
        if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_object()) {
            for (name, version) in deps {
                dependencies.push(Dependency {
                    name: name.clone(),
                    version: version.as_str().unwrap_or("*").to_string(),
                    version_req: Some(version.as_str().unwrap_or("*").to_string()),
                    is_dev: false,
                    is_optional: false,
                    features: vec![],
                    source: DependencySource::Registry("npm".to_string()),
                });
            }
        }

        // Parse devDependencies
        if let Some(deps) = parsed.get("devDependencies").and_then(|d| d.as_object()) {
            for (name, version) in deps {
                dev_dependencies.push(Dependency {
                    name: name.clone(),
                    version: version.as_str().unwrap_or("*").to_string(),
                    version_req: Some(version.as_str().unwrap_or("*").to_string()),
                    is_dev: true,
                    is_optional: false,
                    features: vec![],
                    source: DependencySource::Registry("npm".to_string()),
                });
            }
        }

        let total_count = dependencies.len() + dev_dependencies.len();

        Ok(DependencyAnalysis {
            project_type: ProjectType::Node,
            dependencies,
            dev_dependencies,
            total_count,
            direct_count: total_count,
            outdated: vec![],
            security_issues: vec![],
            duplicate_deps: vec![],
        })
    }

    async fn analyze_python(&self, path: &Path) -> Result<DependencyAnalysis, DepsError> {
        let mut dependencies = Vec::new();
        let mut dev_dependencies = Vec::new();

        // Check pyproject.toml
        let pyproject = path.join("pyproject.toml");
        if pyproject.exists() {
            let content = fs::read_to_string(&pyproject).await
                .map_err(|e| DepsError::IoError(e.to_string()))?;
            
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                // Poetry style
                if let Some(deps) = parsed
                    .get("tool")
                    .and_then(|t| t.get("poetry"))
                    .and_then(|p| p.get("dependencies"))
                    .and_then(|d| d.as_table())
                {
                    for (name, value) in deps {
                        if name != "python" {
                            let version = match value {
                                toml::Value::String(s) => s.clone(),
                                toml::Value::Table(t) => t.get("version")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("*")
                                    .to_string(),
                                _ => "*".to_string(),
                            };
                            dependencies.push(Dependency {
                                name: name.clone(),
                                version: version.clone(),
                                version_req: Some(version),
                                is_dev: false,
                                is_optional: false,
                                features: vec![],
                                source: DependencySource::Registry("pypi".to_string()),
                            });
                        }
                    }
                }
            }
        }

        // Check requirements.txt
        let requirements = path.join("requirements.txt");
        if requirements.exists() {
            let content = fs::read_to_string(&requirements).await
                .map_err(|e| DepsError::IoError(e.to_string()))?;
            
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                let (name, version) = parse_requirements_line(line);
                dependencies.push(Dependency {
                    name,
                    version: version.clone(),
                    version_req: Some(version),
                    is_dev: false,
                    is_optional: false,
                    features: vec![],
                    source: DependencySource::Registry("pypi".to_string()),
                });
            }
        }

        // Check requirements-dev.txt
        let requirements_dev = path.join("requirements-dev.txt");
        if requirements_dev.exists() {
            let content = fs::read_to_string(&requirements_dev).await
                .map_err(|e| DepsError::IoError(e.to_string()))?;
            
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                let (name, version) = parse_requirements_line(line);
                dev_dependencies.push(Dependency {
                    name,
                    version: version.clone(),
                    version_req: Some(version),
                    is_dev: true,
                    is_optional: false,
                    features: vec![],
                    source: DependencySource::Registry("pypi".to_string()),
                });
            }
        }

        let total_count = dependencies.len() + dev_dependencies.len();

        Ok(DependencyAnalysis {
            project_type: ProjectType::Python,
            dependencies,
            dev_dependencies,
            total_count,
            direct_count: total_count,
            outdated: vec![],
            security_issues: vec![],
            duplicate_deps: vec![],
        })
    }

    async fn analyze_go(&self, path: &Path) -> Result<DependencyAnalysis, DepsError> {
        let go_mod = path.join("go.mod");
        let content = fs::read_to_string(&go_mod).await
            .map_err(|e| DepsError::IoError(e.to_string()))?;

        let mut dependencies = Vec::new();
        let mut in_require = false;

        for line in content.lines() {
            let line = line.trim();
            
            if line.starts_with("require (") {
                in_require = true;
                continue;
            }
            if line == ")" {
                in_require = false;
                continue;
            }

            if in_require || line.starts_with("require ") {
                let parts: Vec<&str> = line
                    .trim_start_matches("require ")
                    .split_whitespace()
                    .collect();
                
                if parts.len() >= 2 {
                    dependencies.push(Dependency {
                        name: parts[0].to_string(),
                        version: parts[1].to_string(),
                        version_req: Some(parts[1].to_string()),
                        is_dev: false,
                        is_optional: false,
                        features: vec![],
                        source: DependencySource::Registry("go".to_string()),
                    });
                }
            }
        }

        let total_count = dependencies.len();

        Ok(DependencyAnalysis {
            project_type: ProjectType::Go,
            dependencies,
            dev_dependencies: vec![],
            total_count,
            direct_count: total_count,
            outdated: vec![],
            security_issues: vec![],
            duplicate_deps: vec![],
        })
    }

    /// Generate a dependency report
    pub fn generate_report(&self, analysis: &DependencyAnalysis) -> String {
        let mut report = String::new();
        
        report.push_str("# Dependency Analysis Report\n\n");
        report.push_str(&format!("**Project Type:** {:?}\n", analysis.project_type));
        report.push_str(&format!("**Total Dependencies:** {}\n", analysis.total_count));
        report.push_str(&format!("**Direct Dependencies:** {}\n\n", analysis.direct_count));

        if !analysis.dependencies.is_empty() {
            report.push_str("## Production Dependencies\n\n");
            for dep in &analysis.dependencies {
                report.push_str(&format!("- **{}** v{}", dep.name, dep.version));
                if !dep.features.is_empty() {
                    report.push_str(&format!(" (features: {})", dep.features.join(", ")));
                }
                report.push('\n');
            }
            report.push('\n');
        }

        if !analysis.dev_dependencies.is_empty() {
            report.push_str("## Development Dependencies\n\n");
            for dep in &analysis.dev_dependencies {
                report.push_str(&format!("- **{}** v{}\n", dep.name, dep.version));
            }
            report.push('\n');
        }

        if !analysis.outdated.is_empty() {
            report.push_str("## Outdated Dependencies\n\n");
            for out in &analysis.outdated {
                let severity = if out.is_major { "⚠️ MAJOR" } else { "ℹ️" };
                report.push_str(&format!("{} **{}**: {} → {}\n", 
                    severity, out.name, out.current, out.latest));
            }
            report.push('\n');
        }

        if !analysis.security_issues.is_empty() {
            report.push_str("## Security Issues\n\n");
            for issue in &analysis.security_issues {
                report.push_str(&format!("🔴 **{}** ({})\n", issue.dependency, issue.severity));
                report.push_str(&format!("   {}\n", issue.description));
                if let Some(ref url) = issue.advisory_url {
                    report.push_str(&format!("   More info: {}\n", url));
                }
            }
        }

        report
    }
}

/// Arguments for analyzing dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeDepsArgs {
    pub path: String,
    pub check_outdated: Option<bool>,
    pub check_security: Option<bool>,
}

/// Dependency analyzer errors
#[derive(Debug, thiserror::Error)]
pub enum DepsError {
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("Unknown project type")]
    UnknownProjectType,
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

fn detect_project_type(path: &Path) -> ProjectType {
    if path.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if path.join("package.json").exists() {
        ProjectType::Node
    } else if path.join("pyproject.toml").exists() || 
              path.join("requirements.txt").exists() ||
              path.join("setup.py").exists() {
        ProjectType::Python
    } else if path.join("go.mod").exists() {
        ProjectType::Go
    } else {
        ProjectType::Unknown
    }
}

fn parse_rust_dependency(name: &str, value: &toml::Value, is_dev: bool) -> Dependency {
    match value {
        toml::Value::String(version) => Dependency {
            name: name.to_string(),
            version: version.clone(),
            version_req: Some(version.clone()),
            is_dev,
            is_optional: false,
            features: vec![],
            source: DependencySource::Registry("crates.io".to_string()),
        },
        toml::Value::Table(table) => {
            let version = table.get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("*")
                .to_string();
            
            let features: Vec<String> = table.get("features")
                .and_then(|f| f.as_array())
                .map(|arr| arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect())
                .unwrap_or_default();
            
            let is_optional = table.get("optional")
                .and_then(|o| o.as_bool())
                .unwrap_or(false);

            let source = if let Some(git) = table.get("git").and_then(|g| g.as_str()) {
                let branch = table.get("branch").and_then(|b| b.as_str()).map(|s| s.to_string());
                DependencySource::Git { url: git.to_string(), branch }
            } else if let Some(path) = table.get("path").and_then(|p| p.as_str()) {
                DependencySource::Path(path.to_string())
            } else {
                DependencySource::Registry("crates.io".to_string())
            };

            Dependency {
                name: name.to_string(),
                version,
                version_req: table.get("version").and_then(|v| v.as_str()).map(|s| s.to_string()),
                is_dev,
                is_optional,
                features,
                source,
            }
        }
        _ => Dependency {
            name: name.to_string(),
            version: "*".to_string(),
            version_req: None,
            is_dev,
            is_optional: false,
            features: vec![],
            source: DependencySource::Unknown,
        },
    }
}

fn parse_requirements_line(line: &str) -> (String, String) {
    // Handle various formats: package==1.0, package>=1.0, package~=1.0, package
    let operators = ["==", ">=", "<=", "~=", "!=", ">", "<"];
    
    for op in &operators {
        if let Some(pos) = line.find(op) {
            let name = line[..pos].trim().to_string();
            let version = line[pos..].to_string();
            return (name, version);
        }
    }
    
    (line.to_string(), "*".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_type_detection() {
        // Would need actual filesystem for real tests
    }

    #[test]
    fn test_requirements_parsing() {
        let (name, version) = parse_requirements_line("requests==2.28.0");
        assert_eq!(name, "requests");
        assert_eq!(version, "==2.28.0");

        let (name, version) = parse_requirements_line("flask>=2.0");
        assert_eq!(name, "flask");
        assert_eq!(version, ">=2.0");
    }
}
