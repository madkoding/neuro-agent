use std::fs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelProvider {
    Ollama,
    OpenAI,
    Anthropic,
    Groq,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: ModelProvider,
    pub url: String,
    pub model: String,
    pub temperature: f32,
    pub top_p: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentalConfig {
    pub native_function_calling: bool,
    pub fallback_to_xml: bool,
    pub proactive_validation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub fast_model: ModelConfig,
    pub heavy_model: ModelConfig,
    pub heavy_timeout_secs: u64,
    pub max_concurrent_heavy: usize,
    pub use_router_orchestrator: bool,
    pub language: Option<String>,
    pub debug: bool,
    pub experimental: ExperimentalConfig,
    pub min_ollama_version: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Leer el archivo config.json
    let content = fs::read_to_string("config.json")?;
    println!("Contenido del archivo config.json:");
    println!("{}", content);
    println!("\n--- Intentando deserializar ---\n");

    // Intentar deserializar
    let config: AppConfig = serde_json::from_str(&content)?;
    println!("✅ Deserialización exitosa!");
    println!("Configuración cargada:");
    println!("- Fast model: {} ({})", config.fast_model.model, config.fast_model.provider);
    println!("- Heavy model: {} ({})", config.heavy_model.model, config.heavy_model.provider);
    println!("- Experimental features:");
    println!("  - Native function calling: {}", config.experimental.native_function_calling);
    println!("  - Fallback to XML: {}", config.experimental.fallback_to_xml);
    println!("  - Proactive validation: {}", config.experimental.proactive_validation);
    println!("- Min Ollama version: {:?}", config.min_ollama_version);

    Ok(())
}