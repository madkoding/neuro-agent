//! Configuration system for Neuro
//!
//! Supports loading configuration from:
//! 1. CLI --config argument
//! 2. ~/.config/neuro/config.{NEURO_ENV}.json
//! 3. Default values
//!
//! Where NEURO_ENV can be: production (default), development, test
//!
//! # Examples
//!
//! ## Loading Configuration
//!
//! ```no_run
//! use neuro::config::AppConfig;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load with default priority
//! let config = AppConfig::load(None)?;
//! println!("Fast model: {} via {}", config.fast_model.model, config.fast_model.provider);
//!
//! // Load from specific file
//! let config = AppConfig::load(Some("./my-config.json".as_ref()))?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Creating Configuration
//!
//! ```
//! use neuro::config::{AppConfig, ModelConfig, ModelProvider};
//!
//! let mut config = AppConfig::default();
//! config.fast_model.provider = ModelProvider::OpenAI;
//! config.fast_model.model = "gpt-4o-mini".to_string();
//! config.fast_model.api_key = Some("OPENAI_API_KEY".to_string());
//!
//! // Validate before using
//! config.validate().unwrap();
//! ```
//!
//! ## Environment Variables
//!
//! Environment variables override config file values:
//! - NEURO_OLLAMA_URL
//! - NEURO_FAST_MODEL
//! - NEURO_HEAVY_MODEL
//! - OPENAI_API_KEY
//! - ANTHROPIC_API_KEY
//! - GROQ_API_KEY

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Failed to parse config JSON: {0}")]
    ParseError(#[from] serde_json::Error),
    
    #[error("Invalid configuration: {0}")]
    ValidationError(String),
    
    #[error("Environment not specified")]
    EnvironmentError,
}

/// Supported model providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ModelProvider {
    #[default]
    Ollama,
    OpenAI,
    Anthropic,
    Groq,
}


impl std::fmt::Display for ModelProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAI => write!(f, "openai"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::Groq => write!(f, "groq"),
        }
    }
}

impl std::str::FromStr for ModelProvider {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "openai" => Ok(Self::OpenAI),
            "anthropic" => Ok(Self::Anthropic),
            "groq" => Ok(Self::Groq),
            _ => Err(ConfigError::ValidationError(format!(
                "Unknown provider: {}",
                s
            ))),
        }
    }
}

/// Configuration for a model (fast or heavy)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Provider type
    pub provider: ModelProvider,
    
    /// API URL (for Ollama) or base URL
    #[serde(default = "default_ollama_url")]
    pub url: String,
    
    /// Model name
    pub model: String,
    
    /// API key (can be environment variable name like "OPENAI_API_KEY")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    
    /// Temperature (0.0 - 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    
    /// Top P sampling (0.0 - 1.0)
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_temperature() -> f32 {
    0.2
}

fn default_top_p() -> f32 {
    0.6
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: ModelProvider::Ollama,
            url: default_ollama_url(),
            model: "qwen3:8b".to_string(),
            api_key: None,
            temperature: default_temperature(),
            top_p: default_top_p(),
            max_tokens: None,
        }
    }
}

impl ModelConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate temperature
        if !(0.0..=2.0).contains(&self.temperature) {
            return Err(ConfigError::ValidationError(
                format!("Temperature must be between 0.0 and 2.0, got {}", self.temperature)
            ));
        }
        
        // Validate top_p
        if !(0.0..=1.0).contains(&self.top_p) {
            return Err(ConfigError::ValidationError(
                format!("Top P must be between 0.0 and 1.0, got {}", self.top_p)
            ));
        }
        
        // Validate URL format
        if self.url.is_empty() {
            return Err(ConfigError::ValidationError(
                "URL cannot be empty".to_string()
            ));
        }
        
        // Validate model name
        if self.model.is_empty() {
            return Err(ConfigError::ValidationError(
                "Model name cannot be empty".to_string()
            ));
        }
        
        // Validate API key for non-Ollama providers
        if self.provider != ModelProvider::Ollama && self.api_key.is_none() {
            return Err(ConfigError::ValidationError(
                format!("API key required for {} provider", self.provider)
            ));
        }
        
        Ok(())
    }
    
    /// Resolve API key from environment variable if needed
    pub fn resolve_api_key(&self) -> Option<String> {
        self.api_key.as_ref().and_then(|key| {
            // If the key looks like an env var name, try to resolve it
            if key.chars().all(|c| c.is_uppercase() || c == '_') {
                std::env::var(key).ok()
            } else {
                Some(key.clone())
            }
        })
    }
}

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Fast model configuration (for quick responses and routing)
    pub fast_model: ModelConfig,
    
    /// Heavy model configuration (for complex tasks)
    pub heavy_model: ModelConfig,
    
    /// Timeout for heavy tasks in seconds
    #[serde(default = "default_heavy_timeout")]
    pub heavy_timeout_secs: u64,
    
    /// Maximum concurrent heavy tasks
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_heavy: usize,
    
    /// Use RouterOrchestrator (new simplified router) instead of PlanningOrchestrator
    /// Can be overridden with NEURO_USE_ROUTER environment variable
    #[serde(default = "default_use_router")]
    pub use_router_orchestrator: bool,
    
    /// Preferred language for AI responses ("en" or "es", defaults to system locale)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    
    /// Enable debug logging
    #[serde(default)]
    pub debug: bool,

    /// Experimental features
    #[serde(default)]
    pub experimental: ExperimentalConfig,

    /// Minimum Ollama version required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_ollama_version: Option<String>,
}

/// Experimental features configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentalConfig {
    /// Enable native function calling (Ollama 0.3+)
    #[serde(default)]
    pub native_function_calling: bool,

    /// Fallback to XML format for function calling
    #[serde(default)]
    pub fallback_to_xml: bool,

    /// Enable proactive validation of tool calls
    #[serde(default)]
    pub proactive_validation: bool,
}

impl Default for ExperimentalConfig {
    fn default() -> Self {
        Self {
            native_function_calling: true,
            fallback_to_xml: true,
            proactive_validation: true,
        }
    }
}

fn default_use_router() -> bool {
    true
}

fn default_heavy_timeout() -> u64 {
    1200
}

fn default_max_concurrent() -> usize {
    2
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            fast_model: ModelConfig {
                model: "qwen3:0.6b".to_string(),
                temperature: 0.2,
                top_p: 0.6,
                ..Default::default()
            },
            heavy_model: ModelConfig {
                model: "qwen3:8b".to_string(),
                temperature: 0.3,
                top_p: 0.7,
                ..Default::default()
            },
            heavy_timeout_secs: default_heavy_timeout(),
            max_concurrent_heavy: default_max_concurrent(),
            use_router_orchestrator: default_use_router(),
            language: None, // Will use system locale by default
            debug: false,
            experimental: ExperimentalConfig::default(),
            min_ollama_version: Some("0.3.0".to_string()),
        }
    }
}

impl AppConfig {
    /// Load configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: AppConfig = serde_json::from_str(&content)?;
        
        // Apply environment variable overrides
        config.apply_env_overrides();
        
        // Validate
        config.validate()?;
        
        Ok(config)
    }
    
    /// Load configuration with standard priority:
    /// 1. Explicit path
    /// 2. ~/.config/neuro/config.{NEURO_ENV}.json
    /// 3. Defaults
    pub fn load(explicit_path: Option<&Path>) -> Result<Self, ConfigError> {
        // Try explicit path first
        if let Some(path) = explicit_path {
            if path.exists() {
                tracing::info!("Loading config from: {:?}", path);
                return Self::from_file(path);
            } else {
                return Err(ConfigError::ValidationError(
                    format!("Config file not found: {:?}", path)
                ));
            }
        }
        
        // Try standard location with environment
        let env = std::env::var("NEURO_ENV").unwrap_or_else(|_| "production".to_string());
        
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir
                .join("neuro")
                .join(format!("config.{}.json", env));
            
            if config_path.exists() {
                tracing::info!("Loading config from: {:?}", config_path);
                return Self::from_file(&config_path);
            }
        }
        
        // Fallback to defaults with env overrides
        tracing::info!("Using default configuration with environment overrides");
        let mut config = Self::default();
        config.apply_env_overrides();
        config.validate()?;
        Ok(config)
    }
    
    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // Ollama URL
        if let Ok(url) = std::env::var("NEURO_OLLAMA_URL") {
            if self.fast_model.provider == ModelProvider::Ollama {
                self.fast_model.url = url.clone();
            }
            if self.heavy_model.provider == ModelProvider::Ollama {
                self.heavy_model.url = url;
            }
        }
        
        // Fast model
        if let Ok(model) = std::env::var("NEURO_FAST_MODEL") {
            self.fast_model.model = model;
        }
        
        // Heavy model
        if let Ok(model) = std::env::var("NEURO_HEAVY_MODEL") {
            self.heavy_model.model = model;
        }
        
        // Use router orchestrator
        if let Ok(use_router) = std::env::var("NEURO_USE_ROUTER") {
            self.use_router_orchestrator = use_router.eq_ignore_ascii_case("true") 
                || use_router == "1" 
                || use_router.eq_ignore_ascii_case("yes");
        }
        
        // API keys are resolved on-demand via resolve_api_key()
    }
    
    /// Validate the entire configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.fast_model.validate()?;
        self.heavy_model.validate()?;
        
        if self.heavy_timeout_secs == 0 {
            return Err(ConfigError::ValidationError(
                "heavy_timeout_secs must be greater than 0".to_string()
            ));
        }
        
        if self.max_concurrent_heavy == 0 {
            return Err(ConfigError::ValidationError(
                "max_concurrent_heavy must be greater than 0".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Get the config directory path
    pub fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("neuro"))
    }
    
    /// Save configuration to file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.fast_model.provider, ModelProvider::Ollama);
        assert_eq!(config.heavy_model.provider, ModelProvider::Ollama);
    }

    #[test]
    fn test_model_config_validation() {
        let mut config = ModelConfig::default();
        assert!(config.validate().is_ok());
        
        // Invalid temperature
        config.temperature = 3.0;
        assert!(config.validate().is_err());
        
        config.temperature = 0.7;
        config.top_p = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_provider_from_str() {
        assert_eq!("ollama".parse::<ModelProvider>().unwrap(), ModelProvider::Ollama);
        assert_eq!("openai".parse::<ModelProvider>().unwrap(), ModelProvider::OpenAI);
        assert_eq!("ANTHROPIC".parse::<ModelProvider>().unwrap(), ModelProvider::Anthropic);
        assert!("invalid".parse::<ModelProvider>().is_err());
    }

    #[test]
    fn test_serialize_config() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.fast_model.model, parsed.fast_model.model);
    }
}
