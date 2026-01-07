//! Environment info tool - System and environment information

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;

/// System information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub os_version: Option<String>,
    pub arch: String,
    pub hostname: Option<String>,
    pub cpu_count: usize,
    pub total_memory: Option<u64>,
    pub available_memory: Option<u64>,
}

/// Runtime information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub rust_version: Option<String>,
    pub cargo_version: Option<String>,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub python_version: Option<String>,
    pub go_version: Option<String>,
    pub git_version: Option<String>,
    pub docker_version: Option<String>,
}

/// Shell environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellInfo {
    pub shell: Option<String>,
    pub term: Option<String>,
    pub user: Option<String>,
    pub home: Option<String>,
    pub pwd: Option<String>,
    pub path: Vec<String>,
}

/// Full environment info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    pub system: SystemInfo,
    pub runtime: RuntimeInfo,
    pub shell: ShellInfo,
    pub env_vars: HashMap<String, String>,
}

/// Environment info tool
#[derive(Debug, Clone)]
pub struct EnvironmentTool;

impl Default for EnvironmentTool {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentTool {
    pub const NAME: &'static str = "environment_info";

    pub fn new() -> Self {
        Self
    }

    /// Get full environment information
    pub async fn get_info(&self) -> EnvironmentInfo {
        EnvironmentInfo {
            system: self.get_system_info(),
            runtime: self.get_runtime_info().await,
            shell: self.get_shell_info(),
            env_vars: self.get_env_vars(),
        }
    }

    /// Get system information
    pub fn get_system_info(&self) -> SystemInfo {
        SystemInfo {
            os: env::consts::OS.to_string(),
            os_version: self.get_os_version(),
            arch: env::consts::ARCH.to_string(),
            hostname: hostname::get()
                .ok()
                .map(|h| h.to_string_lossy().to_string()),
            cpu_count: num_cpus::get(),
            total_memory: self.get_total_memory(),
            available_memory: self.get_available_memory(),
        }
    }

    fn get_os_version(&self) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/etc/os-release")
                .ok()
                .and_then(|content| {
                    content
                        .lines()
                        .find(|l| l.starts_with("PRETTY_NAME="))
                        .map(|l| {
                            l.trim_start_matches("PRETTY_NAME=")
                                .trim_matches('"')
                                .to_string()
                        })
                })
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("sw_vers")
                .arg("-productVersion")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        }

        #[cfg(target_os = "windows")]
        {
            Some("Windows".to_string())
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            None
        }
    }

    fn get_total_memory(&self) -> Option<u64> {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|content| {
                    content
                        .lines()
                        .find(|l| l.starts_with("MemTotal:"))
                        .and_then(|l| {
                            l.split_whitespace()
                                .nth(1)
                                .and_then(|v| v.parse::<u64>().ok())
                                .map(|kb| kb * 1024) // Convert to bytes
                        })
                })
        }

        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    fn get_available_memory(&self) -> Option<u64> {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|content| {
                    content
                        .lines()
                        .find(|l| l.starts_with("MemAvailable:"))
                        .and_then(|l| {
                            l.split_whitespace()
                                .nth(1)
                                .and_then(|v| v.parse::<u64>().ok())
                                .map(|kb| kb * 1024)
                        })
                })
        }

        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Get runtime versions
    pub async fn get_runtime_info(&self) -> RuntimeInfo {
        RuntimeInfo {
            rust_version: self.get_command_version("rustc", &["--version"]).await,
            cargo_version: self.get_command_version("cargo", &["--version"]).await,
            node_version: self.get_command_version("node", &["--version"]).await,
            npm_version: self.get_command_version("npm", &["--version"]).await,
            python_version: self
                .get_command_version("python3", &["--version"])
                .await
                .or(self.get_command_version("python", &["--version"]).await),
            go_version: self.get_command_version("go", &["version"]).await,
            git_version: self.get_command_version("git", &["--version"]).await,
            docker_version: self.get_command_version("docker", &["--version"]).await,
        }
    }

    async fn get_command_version(&self, cmd: &str, args: &[&str]) -> Option<String> {
        tokio::process::Command::new(cmd)
            .args(args)
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string()
            })
    }

    /// Get shell information
    pub fn get_shell_info(&self) -> ShellInfo {
        let path = env::var("PATH")
            .map(|p| p.split(':').map(|s| s.to_string()).collect())
            .unwrap_or_default();

        ShellInfo {
            shell: env::var("SHELL").ok(),
            term: env::var("TERM").ok(),
            user: env::var("USER").ok().or_else(|| env::var("USERNAME").ok()),
            home: env::var("HOME")
                .ok()
                .or_else(|| env::var("USERPROFILE").ok()),
            pwd: env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string()),
            path,
        }
    }

    /// Get environment variables (filtered)
    pub fn get_env_vars(&self) -> HashMap<String, String> {
        let safe_vars = [
            "PATH",
            "HOME",
            "USER",
            "SHELL",
            "TERM",
            "LANG",
            "LC_ALL",
            "EDITOR",
            "VISUAL",
            "PAGER",
            "PWD",
            "OLDPWD",
            "TMPDIR",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "RUST_LOG",
            "RUST_BACKTRACE",
            "CARGO_HOME",
            "RUSTUP_HOME",
            "NODE_ENV",
            "NPM_CONFIG_PREFIX",
            "GOPATH",
            "GOROOT",
            "VIRTUAL_ENV",
            "PYTHONPATH",
            "JAVA_HOME",
            "ANDROID_HOME",
        ];

        env::vars()
            .filter(|(k, _)| {
                safe_vars.contains(&k.as_str())
                    || k.starts_with("CARGO_")
                    || k.starts_with("NPM_")
                    || k.starts_with("RUST_")
            })
            .collect()
    }

    /// Get a specific environment variable
    pub fn get_var(&self, name: &str) -> Option<String> {
        env::var(name).ok()
    }

    /// Check if a command exists
    pub async fn command_exists(&self, cmd: &str) -> bool {
        tokio::process::Command::new("which")
            .arg(cmd)
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Get disk usage for a path
    pub async fn disk_usage(&self, path: &str) -> Option<DiskUsage> {
        #[cfg(unix)]
        {
            let output = tokio::process::Command::new("df")
                .args(["-k", path])
                .output()
                .await
                .ok()?;

            if !output.status.success() {
                return None;
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let line = stdout.lines().nth(1)?;
            let parts: Vec<&str> = line.split_whitespace().collect();

            if parts.len() >= 4 {
                let total = parts[1].parse::<u64>().ok()? * 1024;
                let used = parts[2].parse::<u64>().ok()? * 1024;
                let available = parts[3].parse::<u64>().ok()? * 1024;
                let use_percent = if total > 0 {
                    (used as f64 / total as f64 * 100.0) as u8
                } else {
                    0
                };

                return Some(DiskUsage {
                    path: path.to_string(),
                    total,
                    used,
                    available,
                    use_percent,
                });
            }
        }

        None
    }

    /// Format bytes to human readable
    pub fn format_bytes(bytes: u64) -> String {
        const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
        let mut size = bytes as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.2} {}", size, UNITS[unit_index])
        }
    }

    /// Get a summary for display
    pub async fn summary(&self) -> String {
        let info = self.get_info().await;

        let mut summary = String::new();
        summary.push_str(&format!("## System\n"));
        summary.push_str(&format!(
            "- OS: {} ({})\n",
            info.system.os, info.system.arch
        ));
        if let Some(ref version) = info.system.os_version {
            summary.push_str(&format!("- Version: {}\n", version));
        }
        if let Some(ref hostname) = info.system.hostname {
            summary.push_str(&format!("- Hostname: {}\n", hostname));
        }
        summary.push_str(&format!("- CPUs: {}\n", info.system.cpu_count));
        if let Some(mem) = info.system.total_memory {
            summary.push_str(&format!("- Memory: {}\n", Self::format_bytes(mem)));
        }

        summary.push_str(&format!("\n## Runtime\n"));
        if let Some(ref v) = info.runtime.rust_version {
            summary.push_str(&format!("- Rust: {}\n", v));
        }
        if let Some(ref v) = info.runtime.node_version {
            summary.push_str(&format!("- Node: {}\n", v));
        }
        if let Some(ref v) = info.runtime.python_version {
            summary.push_str(&format!("- Python: {}\n", v));
        }
        if let Some(ref v) = info.runtime.go_version {
            summary.push_str(&format!("- Go: {}\n", v));
        }
        if let Some(ref v) = info.runtime.git_version {
            summary.push_str(&format!("- Git: {}\n", v));
        }
        if let Some(ref v) = info.runtime.docker_version {
            summary.push_str(&format!("- Docker: {}\n", v));
        }

        summary.push_str(&format!("\n## Shell\n"));
        if let Some(ref shell) = info.shell.shell {
            summary.push_str(&format!("- Shell: {}\n", shell));
        }
        if let Some(ref user) = info.shell.user {
            summary.push_str(&format!("- User: {}\n", user));
        }
        if let Some(ref pwd) = info.shell.pwd {
            summary.push_str(&format!("- PWD: {}\n", pwd));
        }

        summary
    }
}

/// Disk usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskUsage {
    pub path: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub use_percent: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(EnvironmentTool::format_bytes(0), "0 B");
        assert_eq!(EnvironmentTool::format_bytes(1023), "1023 B");
        assert_eq!(EnvironmentTool::format_bytes(1024), "1.00 KB");
        assert_eq!(EnvironmentTool::format_bytes(1536), "1.50 KB");
        assert_eq!(EnvironmentTool::format_bytes(1048576), "1.00 MB");
        assert_eq!(EnvironmentTool::format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_get_shell_info() {
        let tool = EnvironmentTool::new();
        let info = tool.get_shell_info();
        // At least pwd should be available
        assert!(info.pwd.is_some());
    }

    #[tokio::test]
    async fn test_command_exists() {
        let tool = EnvironmentTool::new();
        // 'ls' should exist on most systems
        assert!(tool.command_exists("ls").await || tool.command_exists("dir").await);
    }
}
