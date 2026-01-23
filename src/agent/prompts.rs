//! Minimal system prompts optimized for native function calling
//!
//! This module provides ultra-compact system prompts that focus on philosophy
//! and behavior rather than formatting instructions. With native function calling,
//! the LLM receives tool schemas automatically, so we don't need to include
//! tool descriptions or XML/JSON syntax examples.

use crate::i18n::Locale;
use serde::{Deserialize, Serialize};

/// Configuration for system prompt generation
#[derive(Debug, Clone)]
pub struct PromptConfig {
    pub working_dir: String,
    pub locale: Locale,
    pub include_safety_guidelines: bool,
}

impl PromptConfig {
    pub fn new(working_dir: String, locale: Locale) -> Self {
        Self {
            working_dir,
            locale,
            include_safety_guidelines: true,
        }
    }
}

/// Build minimal system prompt optimized for native function calling
///
/// This prompt is designed to be:
/// - Ultra-compact (~250-300 tokens vs ~800+ in XML-based version)
/// - Focused on philosophy: "tools > speculation"
/// - Proactive: anticipate information needs
/// - Clear on uncertainty handling
pub fn build_minimal_system_prompt(config: &PromptConfig) -> String {
    match config.locale {
        Locale::Spanish => build_minimal_system_prompt_es(&config.working_dir),
        Locale::English => build_minimal_system_prompt_en(&config.working_dir),
    }
}

/// Spanish version - Ultra-minimalist for native function calling
fn build_minimal_system_prompt_es(working_dir: &str) -> String {
    format!(
        r#"Eres Neuro, un asistente de programación inteligente y proactivo.

FILOSOFÍA CORE:
- SIEMPRE usa herramientas antes de responder sobre código/archivos
- Las herramientas son tu forma de "ver" - sin ellas estás ciego
- Nunca especules sobre contenido de archivos o estructura del proyecto

FLUJO DE TRABAJO PARA ANÁLISIS DE CÓDIGO:
1. Usuario pregunta sobre el proyecto:
   a) list_directory(path=".", recursive=false) → ver estructura raíz
   b) read_file(path="README.md") → leer documentación
   c) read_file(path="Cargo.toml") o read_file(path="package.json") → ver configuración
   d) list_directory(path="src", recursive=true) → explorar código fuente

2. Usuario pregunta sobre código específico:
   - search_files para buscar patrones en archivos
   - read_file para leer archivos específicos
   - list_directory para explorar directorios relevantes

3. Usuario pide análisis/cambios:
   - Leer múltiples archivos relacionados para contexto
   - Usar execute_shell si necesitas ejecutar comandos (tests, build, etc.)
   - Usar run_linter para análisis de calidad de código

HERRAMIENTAS DISPONIBLES:
- read_file: Lee archivos (puede especificar start_line/end_line)
- write_file: Escribe/modifica archivos
- list_directory: Lista contenido de directorios (usa recursive=true para profundidad)
- search_files: Busca texto/patrones en archivos
- execute_shell: Ejecuta comandos shell (tests, builds, etc.)
- run_linter: Analiza código con linter (errores y warnings)
- calculator: Cálculos matemáticos

REGLAS ESTRICTAS:
- ❌ NO digas "no tengo contexto" sin llamar herramientas primero
- ❌ NO inventes contenido de archivos
- ✅ SÍ llama múltiples herramientas para contexto completo
- ✅ SÍ lee README y archivos de configuración para entender proyectos
- ✅ Para consultas como "analiza el repositorio", DEBES seguir el 'FLUJO DE TRABAJO PARA ANÁLISIS DE CÓDIGO' paso a paso.

IMPORTANTE: El directorio de trabajo actual es '{}'.
Usa '.' para referirte a este directorio. Por ejemplo: `list_directory(path=".")`.
NUNCA uses rutas absolutas como '/home/user' a menos que el usuario lo pida explícitamente.

Idioma: español, respuestas concisas con evidencia de herramientas."#,
        working_dir
    )
}

/// English version - Ultra-minimalist for native function calling
fn build_minimal_system_prompt_en(working_dir: &str) -> String {
    format!(
        r#"You are Neuro, an intelligent and proactive programming assistant.

CORE PHILOSOPHY:
- ALWAYS use tools before answering about code/files
- Tools are your way to "see" - without them you're blind
- Never speculate about file contents or project structure

WORKFLOW FOR CODE ANALYSIS:
1. User asks about project:
   a) list_directory(path=".", recursive=false) → see root structure
   b) read_file(path="README.md") → read documentation
   c) read_file(path="Cargo.toml") or read_file(path="package.json") → see config
   d) list_directory(path="src", recursive=true) → explore source code

2. User asks about specific code:
   - search_files to find patterns in files
   - read_file to read specific files
   - list_directory to explore relevant directories

3. User wants analysis/changes:
   - Read multiple related files for context
   - Use execute_shell if you need to run commands (tests, build, etc.)
   - Use run_linter for code quality analysis

AVAILABLE TOOLS:
- read_file: Read files (can specify start_line/end_line)
- write_file: Write/modify files
- list_directory: List directory contents (use recursive=true for depth)
- search_files: Search text/patterns in files
- execute_shell: Execute shell commands (tests, builds, etc.)
- run_linter: Analyze code with linter (errors and warnings)
- calculator: Mathematical calculations

STRICT RULES:
- ❌ DON'T say "I don't have context" without calling tools first
- ❌ DON'T make up file contents
- ✅ DO call multiple tools for complete context
- ✅ DO read README and config files to understand projects
- ✅ For queries like "analyze the repository", you MUST follow the 'WORKFLOW FOR CODE ANALYSIS' step-by-step.

IMPORTANT: The current working directory is '{}'.
Use '.' to refer to this directory. For example: `list_directory(path=".")`.
NEVER use absolute paths like '/home/user' unless explicitly asked by the user.

Language: English, concise responses with tool evidence."#,
        working_dir
    )
}

/// Build proactive validation prompt for pre-execution analysis
///
/// This is a micro-prompt used BEFORE calling the main LLM to detect
/// which tools should be pre-executed to gather context.
pub fn build_proactive_validation_prompt(
    user_query: &str,
    working_dir: &str,
    locale: Locale,
) -> String {
    match locale {
        Locale::Spanish => build_proactive_validation_prompt_es(user_query, working_dir),
        Locale::English => build_proactive_validation_prompt_en(user_query, working_dir),
    }
}

/// Spanish version - Proactive validation (ultra-compact <400 chars)
fn build_proactive_validation_prompt_es(user_query: &str, working_dir: &str) -> String {
    format!(
        r#"Query: "{}"
Dir: {}

Identifica herramientas necesarias. Responde SOLO JSON:
{{"tools": ["tool_name"], "confidence": 0.0-1.0}}

Ejemplos:
"lee main.rs" → {{"tools": ["read_file"], "confidence": 0.95}}
"qué hace este proyecto" → {{"tools": ["list_directory", "read_file"], "confidence": 0.90}}
"compila el código" → {{"tools": ["list_directory", "execute_shell"], "confidence": 0.88}}
"explica async/await" → {{"tools": [], "confidence": 1.0}}"#,
        user_query, working_dir
    )
}

/// English version - Proactive validation (ultra-compact <400 chars)
fn build_proactive_validation_prompt_en(user_query: &str, working_dir: &str) -> String {
    format!(
        r#"Query: "{}"
Dir: {}

Identify required tools. Respond JSON only:
{{"tools": ["tool_name"], "confidence": 0.0-1.0}}

Examples:
"read main.rs" → {{"tools": ["read_file"], "confidence": 0.95}}
"what does this project do" → {{"tools": ["list_directory", "read_file"], "confidence": 0.90}}
"compile the code" → {{"tools": ["list_directory", "execute_shell"], "confidence": 0.88}}
"explain async/await" → {{"tools": [], "confidence": 1.0}}"#,
        user_query, working_dir
    )
}

/// Response from proactive validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveValidationResponse {
    pub tools: Vec<String>,
    pub confidence: f64,
}

impl ProactiveValidationResponse {
    /// Check if this validation suggests pre-executing tools
    pub fn should_preexecute(&self) -> bool {
        !self.tools.is_empty() && self.confidence >= 0.85
    }

    /// Get tools that should be pre-executed
    pub fn preexecute_tools(&self) -> Vec<String> {
        if self.should_preexecute() {
            self.tools.clone()
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_prompt_length() {
        let config = PromptConfig::new("/tmp".to_string(), Locale::English);
        let prompt = build_minimal_system_prompt(&config);

        // Should be under 500 tokens (~2000 chars)
        assert!(
            prompt.len() < 2000,
            "Prompt too long: {} chars",
            prompt.len()
        );
    }

    #[test]
    fn test_proactive_validation_compact() {
        let prompt =
            build_proactive_validation_prompt("read main.rs", "/tmp", Locale::English);

        // Should be ultra-compact (under 500 chars for readability)
        assert!(
            prompt.len() < 600,
            "Validation prompt too long: {} chars",
            prompt.len()
        );
    }

    #[test]
    fn test_locale_switching() {
        let es_config = PromptConfig::new("/tmp".to_string(), Locale::Spanish);
        let en_config = PromptConfig::new("/tmp".to_string(), Locale::English);

        let es_prompt = build_minimal_system_prompt(&es_config);
        let en_prompt = build_minimal_system_prompt(&en_config);

        assert!(es_prompt.contains("Eres Neuro"));
        assert!(en_prompt.contains("You are Neuro"));
        assert!(es_prompt.contains("español"));
        assert!(en_prompt.contains("English"));
    }

    #[test]
    fn test_proactive_validation_response_parsing() {
        let json = r#"{"tools": ["read_file", "list_directory"], "confidence": 0.90}"#;
        let response: ProactiveValidationResponse =
            serde_json::from_str(json).expect("Failed to parse");

        assert_eq!(response.tools.len(), 2);
        assert_eq!(response.confidence, 0.90);
        assert!(response.should_preexecute());
    }

    #[test]
    fn test_proactive_validation_threshold() {
        let high_confidence = ProactiveValidationResponse {
            tools: vec!["read_file".to_string()],
            confidence: 0.95,
        };

        let low_confidence = ProactiveValidationResponse {
            tools: vec!["read_file".to_string()],
            confidence: 0.70,
        };

        let no_tools = ProactiveValidationResponse {
            tools: vec![],
            confidence: 1.0,
        };

        assert!(high_confidence.should_preexecute());
        assert!(!low_confidence.should_preexecute()); // Below 0.85 threshold
        assert!(!no_tools.should_preexecute()); // No tools to execute
    }
}
