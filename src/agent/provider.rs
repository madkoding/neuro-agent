//! Model provider abstraction for different LLM APIs
//!
//! Supports:
//! - Ollama (local models)
//! - OpenAI (GPT models)
//! - Anthropic (Claude models)
//! - Groq (fast inference API)
//!
//! # Examples
//!
//! ## Using Ollama Provider
//!
//! ```no_run
//! use neuro::agent::provider::{create_provider, ModelProvider};
//! use neuro::config::{ModelConfig, ModelProvider as ProviderType};

#![allow(dead_code)]
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = ModelConfig {
//!     provider: ProviderType::Ollama,
//!     url: "http://localhost:11434".to_string(),
//!     model: "qwen3:8b".to_string(),
//!     api_key: None,
//!     temperature: 0.7,
//!     top_p: 0.95,
//!     max_tokens: None,
//! };
//!
//! let provider = create_provider(config)?;
//! provider.validate_connection().await?;
//! let response = provider.generate("Hello, world!").await?;
//! println!("Response: {}", response.content);
//! # Ok(())
//! # }
//! ```
//!
//! ## Using OpenAI Provider
//!
//! ```no_run
//! use neuro::agent::provider::create_provider;
//! use neuro::config::{ModelConfig, ModelProvider as ProviderType};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Set API key in environment: export OPENAI_API_KEY=sk-...
//! let config = ModelConfig {
//!     provider: ProviderType::OpenAI,
//!     url: "https://api.openai.com/v1".to_string(),
//!     model: "gpt-4o-mini".to_string(),
//!     api_key: Some("OPENAI_API_KEY".to_string()), // References env var
//!     temperature: 0.7,
//!     top_p: 0.95,
//!     max_tokens: Some(4096),
//! };
//!
//! let provider = create_provider(config)?;
//! let response = provider.generate("Explain Rust ownership").await?;
//! # Ok(())
//! # }
//! ```

use crate::config::{ModelConfig, ModelProvider as ProviderType};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use thiserror::Error;

/// Provider errors
#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Authentication error: {0}")]
    AuthError(String),
    
    #[error("Model error: {0}")]
    ModelError(String),
    
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    
    #[error("Timeout")]
    Timeout,
    
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Response from a model provider
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub content: String,
    pub model: String,
    pub finish_reason: Option<String>,
}

/// Model provider trait
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Generate a completion
    async fn generate(&self, prompt: &str) -> Result<ProviderResponse, ProviderError>;
    
    /// Validate connection to the provider
    async fn validate_connection(&self) -> Result<(), ProviderError>;
    
    /// Get the model name
    fn model_name(&self) -> &str;
    
    /// Get the provider type
    fn provider_type(&self) -> ProviderType;
}

/// Create a model provider from configuration
pub fn create_provider(config: ModelConfig) -> Result<Box<dyn ModelProvider>, ProviderError> {
    match config.provider {
        ProviderType::Ollama => Ok(Box::new(OllamaProvider::new(config))),
        ProviderType::OpenAI => Ok(Box::new(OpenAIProvider::new(config)?)),
        ProviderType::Anthropic => Ok(Box::new(AnthropicProvider::new(config)?)),
        ProviderType::Groq => Ok(Box::new(GroqProvider::new(config)?)),
    }
}

// ============================================================================
// Ollama Provider
// ============================================================================

pub struct OllamaProvider {
    config: ModelConfig,
    client: Client,
}

impl OllamaProvider {
    pub fn new(config: ModelConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_default();
        
        Self { config, client }
    }
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    top_p: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<usize>,
}

#[derive(Deserialize)]
struct OllamaResponse {
    model: String,
    response: String,
    done: bool,
}

// ============================================================================
// Ollama Native Function Calling Support (0.3+)
// ============================================================================

/// Tool definition in Ollama format for native function calling
#[derive(Debug, Clone, Serialize)]
pub struct OllamaTool {
    #[serde(rename = "type")]
    pub tool_type: String, // Always "function"
    pub function: OllamaFunction,
}

/// Function definition with JSON Schema parameters
#[derive(Debug, Clone, Serialize)]
pub struct OllamaFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema object
}

/// Tool call from model response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaToolCall {
    pub function: OllamaFunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaFunctionCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Chat request with native function calling support
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

/// Chat response with tool calls
#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    model: String,
    created_at: String,
    message: OllamaMessage,
    done: bool,
}

/// Message with optional tool calls
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OllamaMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn generate(&self, prompt: &str) -> Result<ProviderResponse, ProviderError> {
        let url = format!("{}/api/generate", self.config.url);
        
        let request = OllamaRequest {
            model: self.config.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
            options: Some(OllamaOptions {
                temperature: self.config.temperature,
                top_p: self.config.top_p,
                num_predict: self.config.max_tokens,
            }),
        };
        
        let response = self.client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(60))  // Add 60-second timeout for regular generation
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(ProviderError::ModelError(
                format!("HTTP {}: {}", response.status(), response.text().await?)
            ));
        }
        
        let ollama_response: OllamaResponse = response.json().await?;
        
        Ok(ProviderResponse {
            content: ollama_response.response,
            model: ollama_response.model,
            finish_reason: Some(if ollama_response.done { "stop" } else { "length" }.to_string()),
        })
    }
    
    async fn validate_connection(&self) -> Result<(), ProviderError> {
        let url = format!("{}/api/tags", self.config.url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(ProviderError::ConnectionError(
                format!("Failed to connect to Ollama at {}", self.config.url)
            ));
        }
        
        Ok(())
    }
    
    fn model_name(&self) -> &str {
        &self.config.model
    }
    
    fn provider_type(&self) -> ProviderType {
        ProviderType::Ollama
    }
}

impl OllamaProvider {
    /// Generate with native function calling support (Ollama 0.3+)
    ///
    /// This method uses the `/api/chat` endpoint with tools array for native
    /// function calling. Returns the message which may contain tool_calls.
    pub async fn generate_with_tools(
        &self,
        messages: Vec<serde_json::Value>,
        tools: Option<Vec<OllamaTool>>,
    ) -> Result<OllamaMessage, ProviderError> {
        let url = format!("{}/api/chat", self.config.url);

        let request = OllamaChatRequest {
            model: self.config.model.clone(),
            messages,
            tools,
            stream: false,
            options: Some(OllamaOptions {
                temperature: self.config.temperature,
                top_p: self.config.top_p,
                num_predict: self.config.max_tokens,
            }),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .timeout(Duration::from_secs(60))  // Add 60-second timeout for tool calls
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(ProviderError::ModelError(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let chat_response: OllamaChatResponse = response.json().await?;
        Ok(chat_response.message)
    }

    /// Check Ollama version to determine if native function calling is supported
    pub async fn supports_native_tools(&self) -> bool {
        // For now, we'll assume support if the endpoint is reachable
        // In production, we'd query /api/version and check >= 0.3.0
        self.validate_connection().await.is_ok()
    }
}

// ============================================================================
// OpenAI Provider
// ============================================================================

pub struct OpenAIProvider {
    config: ModelConfig,
    client: Client,
    api_key: String,
}

impl OpenAIProvider {
    pub fn new(config: ModelConfig) -> Result<Self, ProviderError> {
        let api_key = config.resolve_api_key()
            .ok_or_else(|| ProviderError::AuthError("OpenAI API key not found".to_string()))?;
        
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_default();
        
        Ok(Self { config, client, api_key })
    }
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    temperature: f32,
    top_p: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
}

#[derive(Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    model: String,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessageResponse,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIMessageResponse {
    content: String,
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn generate(&self, prompt: &str) -> Result<ProviderResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.config.url);
        
        let request = OpenAIRequest {
            model: self.config.model.clone(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: self.config.temperature,
            top_p: self.config.top_p,
            max_tokens: self.config.max_tokens,
        };
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(ProviderError::ModelError(
                format!("HTTP {}: {}", response.status(), response.text().await?)
            ));
        }
        
        let openai_response: OpenAIResponse = response.json().await?;
        
        let choice = openai_response.choices.into_iter().next()
            .ok_or_else(|| ProviderError::InvalidResponse("No choices in response".to_string()))?;
        
        Ok(ProviderResponse {
            content: choice.message.content,
            model: openai_response.model,
            finish_reason: choice.finish_reason,
        })
    }
    
    async fn validate_connection(&self) -> Result<(), ProviderError> {
        let url = format!("{}/models", self.config.url);
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionError(e.to_string()))?;
        
        if response.status() == 401 {
            return Err(ProviderError::AuthError("Invalid API key".to_string()));
        }
        
        if !response.status().is_success() {
            return Err(ProviderError::ConnectionError(
                format!("Failed to connect to OpenAI: HTTP {}", response.status())
            ));
        }
        
        Ok(())
    }
    
    fn model_name(&self) -> &str {
        &self.config.model
    }
    
    fn provider_type(&self) -> ProviderType {
        ProviderType::OpenAI
    }
}

// ============================================================================
// Anthropic Provider
// ============================================================================

pub struct AnthropicProvider {
    config: ModelConfig,
    client: Client,
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(config: ModelConfig) -> Result<Self, ProviderError> {
        let api_key = config.resolve_api_key()
            .ok_or_else(|| ProviderError::AuthError("Anthropic API key not found".to_string()))?;
        
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_default();
        
        Ok(Self { config, client, api_key })
    }
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: usize,
    temperature: f32,
    top_p: f32,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    model: String,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn generate(&self, prompt: &str) -> Result<ProviderResponse, ProviderError> {
        let url = format!("{}/messages", self.config.url);
        
        let request = AnthropicRequest {
            model: self.config.model.clone(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens: self.config.max_tokens.unwrap_or(4096),
            temperature: self.config.temperature,
            top_p: self.config.top_p,
        };
        
        let response = self.client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(ProviderError::ModelError(
                format!("HTTP {}: {}", response.status(), response.text().await?)
            ));
        }
        
        let anthropic_response: AnthropicResponse = response.json().await?;
        
        let content = anthropic_response.content.into_iter()
            .map(|c| c.text)
            .collect::<Vec<_>>()
            .join("\n");
        
        Ok(ProviderResponse {
            content,
            model: anthropic_response.model,
            finish_reason: anthropic_response.stop_reason,
        })
    }
    
    async fn validate_connection(&self) -> Result<(), ProviderError> {
        // Anthropic doesn't have a simple health check endpoint
        // We'll do a minimal test request
        let url = format!("{}/messages", self.config.url);
        
        let test_request = json!({
            "model": self.config.model,
            "messages": [{"role": "user", "content": "test"}],
            "max_tokens": 1
        });
        
        let response = self.client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&test_request)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionError(e.to_string()))?;
        
        if response.status() == 401 {
            return Err(ProviderError::AuthError("Invalid API key".to_string()));
        }
        
        if !response.status().is_success() {
            return Err(ProviderError::ConnectionError(
                format!("Failed to connect to Anthropic: HTTP {}", response.status())
            ));
        }
        
        Ok(())
    }
    
    fn model_name(&self) -> &str {
        &self.config.model
    }
    
    fn provider_type(&self) -> ProviderType {
        ProviderType::Anthropic
    }
}

// ============================================================================
// Groq Provider (OpenAI-compatible API)
// ============================================================================

pub struct GroqProvider {
    config: ModelConfig,
    client: Client,
    api_key: String,
}

impl GroqProvider {
    pub fn new(config: ModelConfig) -> Result<Self, ProviderError> {
        let api_key = config.resolve_api_key()
            .ok_or_else(|| ProviderError::AuthError("Groq API key not found".to_string()))?;
        
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_default();
        
        Ok(Self { config, client, api_key })
    }
}

// Groq uses OpenAI-compatible API, so we reuse the same structures

#[async_trait]
impl ModelProvider for GroqProvider {
    async fn generate(&self, prompt: &str) -> Result<ProviderResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.config.url);
        
        let request = OpenAIRequest {
            model: self.config.model.clone(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: self.config.temperature,
            top_p: self.config.top_p,
            max_tokens: self.config.max_tokens,
        };
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionError(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(ProviderError::ModelError(
                format!("HTTP {}: {}", response.status(), response.text().await?)
            ));
        }
        
        let groq_response: OpenAIResponse = response.json().await?;
        
        let choice = groq_response.choices.into_iter().next()
            .ok_or_else(|| ProviderError::InvalidResponse("No choices in response".to_string()))?;
        
        Ok(ProviderResponse {
            content: choice.message.content,
            model: groq_response.model,
            finish_reason: choice.finish_reason,
        })
    }
    
    async fn validate_connection(&self) -> Result<(), ProviderError> {
        let url = format!("{}/models", self.config.url);
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| ProviderError::ConnectionError(e.to_string()))?;
        
        if response.status() == 401 {
            return Err(ProviderError::AuthError("Invalid API key".to_string()));
        }
        
        if !response.status().is_success() {
            return Err(ProviderError::ConnectionError(
                format!("Failed to connect to Groq: HTTP {}", response.status())
            ));
        }
        
        Ok(())
    }
    
    fn model_name(&self) -> &str {
        &self.config.model
    }
    
    fn provider_type(&self) -> ProviderType {
        ProviderType::Groq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let config = ModelConfig::default();
        let provider = OllamaProvider::new(config);
        assert_eq!(provider.model_name(), "qwen3:8b");
        assert_eq!(provider.provider_type(), ProviderType::Ollama);
    }

    #[test]
    fn test_create_provider() {
        let config = ModelConfig::default();
        let provider = create_provider(config).unwrap();
        assert_eq!(provider.provider_type(), ProviderType::Ollama);
    }
}
