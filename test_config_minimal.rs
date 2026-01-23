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

fn main() {
    // Crear una configuración de prueba
    let config = AppConfig {
        fast_model: ModelConfig {
            provider: ModelProvider::Ollama,
            url: "http://localhost:11434".to_string(),
            model: "glm4:9b-chat-q8_0".to_string(),
            temperature: 0.7,
            top_p: 0.95,
        },
        heavy_model: ModelConfig {
            provider: ModelProvider::Ollama,
            url: "http://localhost:11434".to_string(),
            model: "glm4:9b-chat-q8_0".to_string(),
            temperature: 0.7,
            top_p: 0.95,
        },
        heavy_timeout_secs: 1200,
        max_concurrent_heavy: 2,
        use_router_orchestrator: true,
        language: Some("es".to_string()),
        debug: true,
        experimental: ExperimentalConfig {
            native_function_calling: true,
            fallback_to_xml: true,
            proactive_validation: true,
        },
        min_ollama_version: Some("0.3.0".to_string()),
    };

    // Serializar a JSON
    let json = serde_json::to_string_pretty(&config).unwrap();

    println!("✅ Estructuras compiladas correctamente!");
    println!("Configuración de ejemplo:");
    println!("{}", json);

    // Intentar deserializar
    let parsed: AppConfig = serde_json::from_str(&json).unwrap();
    println!("\n✅ Deserialización exitosa!");
    println!("Experimental features:");
    println!("- Native function calling: {}", parsed.experimental.native_function_calling);
    println!("- Fallback to XML: {}", parsed.experimental.fallback_to_xml);
    println!("- Proactive validation: {}", parsed.experimental.proactive_validation);
    println!("Min Ollama version: {:?}", parsed.min_ollama_version);
}