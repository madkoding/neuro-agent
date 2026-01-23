//! Command scanner for detecting dangerous commands

use regex::Regex;
use std::sync::LazyLock;

/// Risk level for commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// Safe command, no confirmation needed
    Safe,
    /// Low risk, simple confirmation (Y/N)
    Low,
    /// Medium risk, confirmation with description
    Medium,
    /// High risk, requires password
    High,
    /// Critical, blocked by default
    Critical,
}

impl RiskLevel {
    pub fn requires_confirmation(&self) -> bool {
        !matches!(self, RiskLevel::Safe)
    }

    pub fn requires_password(&self) -> bool {
        matches!(self, RiskLevel::High | RiskLevel::Critical)
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self, RiskLevel::Critical)
    }

    pub fn description(&self) -> &'static str {
        match self {
            RiskLevel::Safe => "Safe command",
            RiskLevel::Low => "This command may modify files",
            RiskLevel::Medium => "This command may cause data loss",
            RiskLevel::High => "This command requires elevated privileges",
            RiskLevel::Critical => "This command is blocked for security",
        }
    }
}

/// Scanner for detecting dangerous commands
#[derive(Debug, Clone)]
pub struct CommandScanner {
    critical_patterns: Vec<Regex>,
    high_patterns: Vec<Regex>,
    medium_patterns: Vec<Regex>,
    low_patterns: Vec<Regex>,
}

/// Dangerous command patterns organized by category
static CRITICAL_COMMANDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        r"rm\s+(-[rf]+\s+)*/?$",           // rm -rf /
        r"rm\s+(-[rf]+\s+)*/\*",           // rm -rf /*
        r":\(\)\s*\{\s*:\|:\s*&\s*\}\s*;", // Fork bomb
        r"mkfs\.",                         // Format filesystem
        r"dd\s+if=[^\s]*\sof=/dev/[sh]d[a-z]", // dd to disk (avoid .* backtracking)
        r">\s*/dev/[sh]d[a-z]",            // Redirect to disk
        r"chmod\s+(-R\s+)?777\s+/",        // chmod 777 /
        r"/etc/shadow",                    // Shadow file access
        r"/etc/sudoers",                   // Sudoers access
    ]
});

static HIGH_COMMANDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        r"\bsudo\b",                        // sudo
        r"\bsu\s+-",                        // su -
        r"\bdoas\b",                        // doas
        r"\bpkexec\b",                      // pkexec
        r"dd\s+if=",                        // dd command
        r"systemctl\s+(stop|disable|mask)", // Stop services
        r"service\s+\w+\s+stop",            // Stop services (legacy)
        r"iptables\s+-F",                   // Flush iptables
        r"ufw\s+disable",                   // Disable firewall
        r"shutdown",                        // Shutdown
        r"reboot",                          // Reboot
        r"init\s+[06]",                     // Init level change
        r"halt",                            // Halt
        r"poweroff",                        // Poweroff
    ]
});

static MEDIUM_COMMANDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        r"rm\s+(-[rf]+)",                // rm -rf (not root)
        r"chmod\s+(-R\s+)?[0-7]{3}",     // chmod with octal
        r"chown\s+-R",                   // Recursive chown
        r"git\s+push\s+[^\s]*\s+--force", // Force push (avoid .* backtracking)
        r"git\s+reset\s+--hard",         // Hard reset
        r"git\s+clean\s+-[fd]+",         // Git clean
        r"DROP\s+(DATABASE|TABLE)",      // SQL drop
        r"TRUNCATE\s+TABLE",             // SQL truncate
        r"DELETE\s+FROM\s+\w+\s*;?\s*$", // DELETE without WHERE
        r"docker\s+rm\s+-f",             // Force remove container
        r"docker\s+system\s+prune",      // Docker prune
        r"docker\s+volume\s+rm",         // Remove volume
        r"curl\s+[^\s]*\s*\|\s*(ba)?sh", // Curl pipe to shell (avoid .* backtracking)
        r"wget\s+[^\s]*\s*\|\s*(ba)?sh", // Wget pipe to shell (avoid .* backtracking)
    ]
});

static LOW_COMMANDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        r"rm\s+",                            // Any rm command
        r"mv\s+",                            // Move files
        r"cp\s+.*-[rf]",                     // Copy recursive/force
        r"chmod\s+",                         // Any chmod
        r"chown\s+",                         // Any chown
        r"npm\s+install\s+-g",               // Global npm install
        r"pip\s+install\s+--system",         // System pip install
        r"cargo\s+install",                  // Cargo install
        r"apt\s+(install|remove|purge)",     // Apt operations
        r"apt-get\s+(install|remove|purge)", // Apt-get operations
        r"pacman\s+-[SR]",                   // Pacman operations
        r"brew\s+(install|uninstall)",       // Homebrew operations
    ]
});

impl Default for CommandScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandScanner {
    pub fn new() -> Self {
        let compile_patterns = |patterns: &[&str]| -> Vec<Regex> {
            patterns.iter().filter_map(|p| Regex::new(p).ok()).collect()
        };

        Self {
            critical_patterns: compile_patterns(&CRITICAL_COMMANDS),
            high_patterns: compile_patterns(&HIGH_COMMANDS),
            medium_patterns: compile_patterns(&MEDIUM_COMMANDS),
            low_patterns: compile_patterns(&LOW_COMMANDS),
        }
    }

    /// Scan a command and return its risk level
    pub fn scan(&self, command: &str) -> RiskLevel {
        let cmd_lower = command.to_lowercase();

        // Check from most dangerous to least
        for pattern in &self.critical_patterns {
            if pattern.is_match(&cmd_lower) {
                return RiskLevel::Critical;
            }
        }

        for pattern in &self.high_patterns {
            if pattern.is_match(&cmd_lower) {
                return RiskLevel::High;
            }
        }

        for pattern in &self.medium_patterns {
            if pattern.is_match(&cmd_lower) {
                return RiskLevel::Medium;
            }
        }

        for pattern in &self.low_patterns {
            if pattern.is_match(&cmd_lower) {
                return RiskLevel::Low;
            }
        }

        RiskLevel::Safe
    }

    /// Get a human-readable description of why a command is dangerous
    pub fn get_warning(&self, command: &str) -> Option<String> {
        let risk = self.scan(command);

        if risk == RiskLevel::Safe {
            return None;
        }

        let warning = match risk {
            RiskLevel::Critical => format!(
                "⛔ BLOCKED: This command is extremely dangerous and has been blocked.\n\
                 Command: {}\n\
                 Reason: Could cause irreversible system damage.",
                command
            ),
            RiskLevel::High => format!(
                "🔐 HIGH RISK: This command requires password confirmation.\n\
                 Command: {}\n\
                 Reason: {}",
                command,
                risk.description()
            ),
            RiskLevel::Medium => format!(
                "⚠️ MEDIUM RISK: Please confirm this potentially dangerous command.\n\
                 Command: {}\n\
                 Reason: {}",
                command,
                risk.description()
            ),
            RiskLevel::Low => format!(
                "ℹ️ LOW RISK: This command may modify your system.\n\
                 Command: {}\n\
                 Reason: {}",
                command,
                risk.description()
            ),
            RiskLevel::Safe => unreachable!(),
        };

        Some(warning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critical_commands() {
        let scanner = CommandScanner::new();

        assert_eq!(scanner.scan("rm -rf /"), RiskLevel::Critical);
        assert_eq!(scanner.scan("rm -rf /*"), RiskLevel::Critical);
        assert_eq!(scanner.scan("mkfs.ext4 /dev/sda1"), RiskLevel::Critical);
    }

    #[test]
    fn test_high_commands() {
        let scanner = CommandScanner::new();

        assert_eq!(scanner.scan("sudo apt update"), RiskLevel::High);
        assert_eq!(scanner.scan("shutdown now"), RiskLevel::High);
        assert_eq!(scanner.scan("reboot"), RiskLevel::High);
    }

    #[test]
    fn test_medium_commands() {
        let scanner = CommandScanner::new();

        assert_eq!(scanner.scan("rm -rf ./build"), RiskLevel::Medium);
        assert_eq!(scanner.scan("git push --force"), RiskLevel::Medium);
        assert_eq!(scanner.scan("docker system prune"), RiskLevel::Medium);
    }

    #[test]
    fn test_safe_commands() {
        let scanner = CommandScanner::new();

        assert_eq!(scanner.scan("ls -la"), RiskLevel::Safe);
        assert_eq!(scanner.scan("cat file.txt"), RiskLevel::Safe);
        assert_eq!(scanner.scan("cargo build"), RiskLevel::Safe);
        assert_eq!(scanner.scan("git status"), RiskLevel::Safe);
    }
}
