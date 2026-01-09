//! Streaming support for RouterOrchestrator
//!
//! Provides streaming response capabilities for real-time token generation.

use super::router_orchestrator::{RouterConfig, RouterOrchestrator};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

/// Streaming chunk message
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub content: String,
    pub done: bool,
}

/// Ollama streaming request
#[derive(Serialize)]
struct OllamaStreamRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
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

/// Ollama streaming response (one chunk)
#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    created_at: String,
    message: OllamaStreamMessage,
    done: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamMessage {
    #[allow(dead_code)]
    role: String,
    content: String,
}

impl RouterOrchestrator {
    /// Process query with streaming responses using Ollama's native streaming API
    pub async fn process_streaming(
        &self,
        user_query: &str,
        chunk_tx: Sender<StreamChunk>,
        config: &RouterConfig,
    ) -> Result<String> {
        use futures::StreamExt;
        
        // Classify query first (non-streaming)
        let _decision = self.classify(user_query).await?;
        
        // Build URL for Ollama streaming endpoint
        let url = format!("{}/api/chat", config.ollama_url);
        
        // Build messages
        let messages = vec![
            serde_json::json!({
                "role": "user",
                "content": user_query
            })
        ];
        
        // Build streaming request
        let request = OllamaStreamRequest {
            model: config.heavy_model.clone(),
            messages,
            tools: None, // For now, no tools in streaming mode
            stream: true, // Enable streaming
            options: Some(OllamaOptions {
                temperature: 0.7,
                top_p: 0.95,
                num_predict: Some(4096),
            }),
        };
        
        // Create HTTP client
        let client = reqwest::Client::new();
        
        // Send streaming request
        let response = client
            .post(&url)
            .json(&request)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Ollama streaming failed: HTTP {} - {}",
                status,
                error_text
            ));
        }
        
        // Process stream
        let mut full_response = String::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        
        while let Some(chunk_result) = stream.next().await {
            let chunk_bytes = chunk_result?;
            let chunk_str = String::from_utf8_lossy(&chunk_bytes);
            buffer.push_str(&chunk_str);
            
            // Process complete JSON lines
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer.drain(..=newline_pos).collect::<String>();
                let line = line.trim();
                
                if line.is_empty() {
                    continue;
                }
                
                // Parse JSON chunk
                match serde_json::from_str::<OllamaStreamChunk>(line) {
                    Ok(stream_chunk) => {
                        let content = stream_chunk.message.content;
                        
                        if !content.is_empty() {
                            full_response.push_str(&content);
                            
                            // Send chunk to UI
                            let _ = chunk_tx.send(StreamChunk {
                                content: content.clone(),
                                done: false,
                            }).await;
                        }
                        
                        // Check if done
                        if stream_chunk.done {
                            // Send final done signal
                            let _ = chunk_tx.send(StreamChunk {
                                content: String::new(),
                                done: true,
                            }).await;
                            break;
                        }
                    }
                    Err(e) => {
                        // Log parse error but continue
                        if config.debug {
                            eprintln!("Failed to parse streaming chunk: {} - Line: {}", e, line);
                        }
                    }
                }
            }
        }
        
        Ok(full_response)
    }
}
