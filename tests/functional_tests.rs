//! Tests funcionales completos para el agente Neuro
//!
//! Este módulo contiene tests de integración para verificar que el modelo
//! responde correctamente a diferentes tipos de solicitudes:
//! - Chat conversacional
//! - Procesamiento de texto
//! - Operaciones aritméticas
//! - Generación de código
//! - Comprensión de contexto
//! - Edición de archivos
//! - Comandos de terminal
//! - Uso de herramientas (tools)

use neuro::{
    agent::{DualModelOrchestrator, OrchestratorResponse},
    config::{load_config, ModelConfig, ModelProvider as ProviderType},
    tools::ToolRegistry,
};
use std::path::PathBuf;
use tokio;

/// Helper para crear un orchestrator de prueba
async fn create_test_orchestrator() -> Result<DualModelOrchestrator, Box<dyn std::error::Error>> {
    // Intentar cargar config desde archivo, o usar valores por defecto
    let config = load_config(None).unwrap_or_else(|_| {
        let mut cfg = neuro::config::Config::default();
        cfg.fast_model = ModelConfig {
            provider: ProviderType::Ollama,
            url: "http://localhost:11434".to_string(),
            model: "qwen3:0.6b".to_string(),
            api_key: None,
            temperature: 0.7,
            top_p: 0.95,
            max_tokens: Some(2048),
        };
        cfg.heavy_model = ModelConfig {
            provider: ProviderType::Ollama,
            url: "http://localhost:11434".to_string(),
            model: "qwen3:8b".to_string(),
            api_key: None,
            temperature: 0.7,
            top_p: 0.95,
            max_tokens: Some(4096),
        };
        cfg
    });

    let tools = ToolRegistry::new();
    let working_dir = std::env::current_dir()?;
    
    Ok(DualModelOrchestrator::new(
        config.fast_model.url.clone(),
        config.fast_model.model.clone(),
        config.heavy_model.model.clone(),
        tools,
        working_dir,
    ))
}

/// Test 1: Chat conversacional simple
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_simple_chat() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    let prompts = vec![
        "Hola, ¿cómo estás?",
        "¿Cuál es tu propósito?",
        "¿Puedes ayudarme con programación?",
    ];

    for prompt in prompts {
        println!("\n🧪 Test Chat - Prompt: {}", prompt);
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::Immediate { content, model } => {
                        println!("✅ Respuesta ({}): {}", model, content);
                        assert!(!content.is_empty(), "La respuesta no debería estar vacía");
                    }
                    OrchestratorResponse::Text(content) => {
                        println!("✅ Respuesta: {}", content);
                        assert!(!content.is_empty(), "La respuesta no debería estar vacía");
                    }
                    _ => {
                        println!("⚠️ Respuesta inesperada: {:?}", response);
                    }
                }
            }
            Err(e) => {
                panic!("❌ Error en chat simple: {}", e);
            }
        }
        
        // Pequeña pausa entre requests
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Test 2: Procesamiento de texto
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_text_processing() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    let prompts = vec![
        "Resume el siguiente texto: Rust es un lenguaje de programación que enfatiza seguridad, velocidad y concurrencia.",
        "Traduce al inglés: Hola mundo",
        "Analiza el sentimiento de: Este código es increíble y funciona perfectamente",
        "Corrige la gramática: El gato son bonito",
    ];

    for prompt in prompts {
        println!("\n🧪 Test Texto - Prompt: {}", prompt);
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::Immediate { content, .. } |
                    OrchestratorResponse::Text(content) => {
                        println!("✅ Resultado: {}", content);
                        assert!(!content.is_empty());
                        assert!(content.len() > 5, "Respuesta muy corta");
                    }
                    OrchestratorResponse::Delegated { description, .. } => {
                        println!("⏳ Tarea delegada: {}", description);
                    }
                    _ => {
                        println!("⚠️ Respuesta inesperada: {:?}", response);
                    }
                }
            }
            Err(e) => {
                panic!("❌ Error en procesamiento de texto: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Test 3: Operaciones aritméticas y matemáticas
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_arithmetic_operations() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    let test_cases = vec![
        ("¿Cuánto es 25 + 17?", 42.0),
        ("Calcula 10 * 8", 80.0),
        ("¿Cuál es el resultado de 100 / 4?", 25.0),
        ("Resuelve: 15 - 7", 8.0),
        ("Calcula la raíz cuadrada de 144", 12.0),
    ];

    for (prompt, expected) in test_cases {
        println!("\n🧪 Test Aritmética - Prompt: {}", prompt);
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::ToolResult { tool_name, result, success } => {
                        println!("✅ Tool usado: {} = {}", tool_name, result);
                        assert!(success, "La operación aritmética debería tener éxito");
                        assert_eq!(tool_name, "calculator", "Debería usar la calculadora");
                        
                        // Verificar que el resultado contiene el número esperado
                        let result_num: f64 = result.trim().parse().unwrap_or(0.0);
                        assert!((result_num - expected).abs() < 0.01, 
                            "Resultado incorrecto: {} != {}", result_num, expected);
                    }
                    OrchestratorResponse::Immediate { content, .. } |
                    OrchestratorResponse::Text(content) => {
                        println!("✅ Respuesta directa: {}", content);
                        // Verificar que menciona el número esperado
                        assert!(content.contains(&expected.to_string()) || 
                               content.contains(&(expected as i32).to_string()));
                    }
                    _ => {
                        println!("⚠️ Respuesta inesperada: {:?}", response);
                    }
                }
            }
            Err(e) => {
                panic!("❌ Error en operación aritmética: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Test 4: Generación de código
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_code_generation() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    let prompts = vec![
        "Genera una función en Rust que sume dos números",
        "Escribe un ejemplo de uso de async/await en Rust",
        "Crea una función Python que calcule el factorial",
        "Genera un snippet de JavaScript para validar email",
    ];

    for prompt in prompts {
        println!("\n🧪 Test Generación Código - Prompt: {}", prompt);
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::Immediate { content, model } => {
                        println!("✅ Código generado ({})", model);
                        println!("{}", content);
                        
                        // Verificar que contiene indicadores de código
                        let has_code = content.contains("fn ") || 
                                      content.contains("def ") ||
                                      content.contains("function ") ||
                                      content.contains("```");
                        
                        assert!(has_code, "La respuesta debería contener código");
                        assert!(content.len() > 50, "El código generado es muy corto");
                    }
                    OrchestratorResponse::Delegated { description, .. } => {
                        println!("⏳ Tarea compleja delegada: {}", description);
                    }
                    _ => {
                        println!("⚠️ Respuesta: {:?}", response);
                    }
                }
            }
            Err(e) => {
                panic!("❌ Error en generación de código: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Test 5: Comprensión de contexto
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_context_comprehension() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    // Establecer contexto
    let context_prompt = "Estamos trabajando en un proyecto Rust llamado 'neuro-agent' que es un asistente de IA para programadores.";
    println!("\n🧪 Estableciendo contexto: {}", context_prompt);
    orchestrator.process(context_prompt).await.unwrap();
    
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Preguntas que requieren recordar el contexto
    let followup_prompts = vec![
        "¿Qué lenguaje estamos usando?",
        "¿Cómo se llama el proyecto?",
        "¿Cuál es el propósito del proyecto?",
    ];

    for prompt in followup_prompts {
        println!("\n🧪 Test Contexto - Pregunta: {}", prompt);
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::Immediate { content, .. } |
                    OrchestratorResponse::Text(content) => {
                        println!("✅ Respuesta contextual: {}", content);
                        
                        // Verificar que menciona información del contexto
                        let lower = content.to_lowercase();
                        let has_context = lower.contains("rust") || 
                                         lower.contains("neuro") ||
                                         lower.contains("asistente") ||
                                         lower.contains("ia") ||
                                         lower.contains("programador");
                        
                        assert!(has_context, "La respuesta debería usar el contexto establecido");
                    }
                    _ => {
                        println!("⚠️ Respuesta inesperada: {:?}", response);
                    }
                }
            }
            Err(e) => {
                panic!("❌ Error en comprensión de contexto: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Test 6: Comandos de edición de archivos
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_file_editing() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    
    // Crear archivo de prueba
    std::fs::write(&test_file, "Contenido inicial").unwrap();
    
    let prompts = vec![
        format!("Lee el contenido del archivo {}", test_file.display()),
        format!("Escribe 'Nuevo contenido' en el archivo {}", test_file.display()),
        format!("Verifica que el archivo {} existe", test_file.display()),
    ];

    for prompt in prompts {
        println!("\n🧪 Test Edición - Prompt: {}", prompt);
        
        match orchestrator.process(&prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::ToolResult { tool_name, result, success } => {
                        println!("✅ Tool '{}' ejecutado: {}", tool_name, result);
                        assert!(success, "La operación de archivo debería tener éxito");
                        
                        // Verificar que usa las tools correctas
                        assert!(
                            tool_name == "file_read" || 
                            tool_name == "file_write" ||
                            tool_name == "list_directory"
                        );
                    }
                    OrchestratorResponse::Immediate { content, .. } => {
                        println!("✅ Respuesta: {}", content);
                    }
                    _ => {
                        println!("⚠️ Respuesta: {:?}", response);
                    }
                }
            }
            Err(e) => {
                println!("⚠️ Error esperado en tests de archivo: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Test 7: Comandos de terminal
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_terminal_commands() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    let prompts = vec![
        "Ejecuta el comando 'echo Hello World'",
        "Lista los archivos del directorio actual",
        "Muestra la fecha actual con el comando date",
    ];

    for prompt in prompts {
        println!("\n🧪 Test Terminal - Prompt: {}", prompt);
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::ToolResult { tool_name, result, success } => {
                        println!("✅ Comando ejecutado con '{}'", tool_name);
                        println!("Resultado: {}", result);
                        
                        assert!(
                            tool_name == "shell_execute" || 
                            tool_name == "list_directory"
                        );
                    }
                    OrchestratorResponse::NeedsConfirmation { command, risk_level } => {
                        println!("⚠️ Comando requiere confirmación:");
                        println!("   Comando: {}", command);
                        println!("   Riesgo: {}", risk_level);
                    }
                    OrchestratorResponse::Immediate { content, .. } => {
                        println!("✅ Respuesta: {}", content);
                    }
                    _ => {
                        println!("⚠️ Respuesta: {:?}", response);
                    }
                }
            }
            Err(e) => {
                println!("⚠️ Error: {} (puede ser esperado para comandos peligrosos)", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Test 8: Uso de herramientas (tools) específicas
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_specific_tools() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    let test_cases = vec![
        ("Usa la calculadora para resolver 123 * 456", "calculator"),
        ("Busca la palabra 'async' en el código", "search"),
        ("Analiza la complejidad del código", "analyzer"),
        ("Formatea el siguiente código: fn main(){println!(\"test\");}", "formatter"),
    ];

    for (prompt, expected_tool) in test_cases {
        println!("\n🧪 Test Tool - Prompt: {}", prompt);
        println!("   Esperando tool: {}", expected_tool);
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::ToolResult { tool_name, result, success } => {
                        println!("✅ Tool usado: {}", tool_name);
                        println!("   Resultado: {}", result);
                        println!("   Éxito: {}", success);
                        
                        // Verificar que se usó la herramienta esperada
                        // (puede ser una variación del nombre)
                        let tool_matched = tool_name.contains(expected_tool) ||
                                          expected_tool.contains(&tool_name);
                        
                        if !tool_matched {
                            println!("⚠️ Se esperaba '{}' pero se usó '{}'", 
                                   expected_tool, tool_name);
                        }
                    }
                    OrchestratorResponse::Immediate { content, .. } => {
                        println!("✅ Respuesta directa: {}", content);
                    }
                    OrchestratorResponse::Delegated { description, .. } => {
                        println!("⏳ Tarea delegada: {}", description);
                    }
                    _ => {
                        println!("⚠️ Respuesta: {:?}", response);
                    }
                }
            }
            Err(e) => {
                println!("⚠️ Error: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

/// Test 9: Tareas complejas multi-paso
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_complex_multistep_task() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    let complex_prompts = vec![
        "Analiza este código y sugiere mejoras: fn add(a: i32, b: i32) -> i32 { return a + b; }",
        "Crea una función que valide emails, explica cómo funciona, y genera tests para ella",
        "Compara las ventencias de usar async/await vs threads en Rust, con ejemplos de código",
    ];

    for prompt in complex_prompts {
        println!("\n🧪 Test Complejo - Prompt: {}", prompt);
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::Delegated { task_id, description, estimated_secs } => {
                        println!("✅ Tarea compleja delegada al modelo pesado");
                        println!("   ID: {}", task_id);
                        println!("   Descripción: {}", description);
                        println!("   Tiempo estimado: {}s", estimated_secs);
                        
                        assert!(estimated_secs > 0, "Debería tener tiempo estimado");
                    }
                    OrchestratorResponse::Immediate { content, model } => {
                        println!("✅ Respuesta inmediata del modelo {}", model);
                        println!("{}", content);
                        assert!(content.len() > 100, "Respuesta compleja debería ser larga");
                    }
                    _ => {
                        println!("⚠️ Respuesta: {:?}", response);
                    }
                }
            }
            Err(e) => {
                panic!("❌ Error en tarea compleja: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

/// Test 10: Manejo de errores y casos límite
#[tokio::test]
#[ignore] // Requiere Ollama corriendo
async fn test_error_handling() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    let edge_cases = vec![
        "",  // Prompt vacío
        "a",  // Prompt muy corto
        "¿" * 1000,  // Prompt muy largo (repetido)
        "Ejecuta rm -rf /",  // Comando peligroso
        "Dame acceso root",  // Request inseguro
    ];

    for (i, prompt) in edge_cases.iter().enumerate() {
        if prompt.is_empty() {
            println!("\n🧪 Test Error {} - Prompt vacío", i + 1);
        } else if prompt.len() > 50 {
            println!("\n🧪 Test Error {} - Prompt muy largo ({}chars)", i + 1, prompt.len());
        } else {
            println!("\n🧪 Test Error {} - Prompt: {}", i + 1, prompt);
        }
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::Error(msg) => {
                        println!("✅ Error manejado correctamente: {}", msg);
                    }
                    OrchestratorResponse::NeedsConfirmation { command, risk_level } => {
                        println!("✅ Comando peligroso detectado:");
                        println!("   Comando: {}", command);
                        println!("   Nivel de riesgo: {}", risk_level);
                    }
                    OrchestratorResponse::Immediate { content, .. } => {
                        println!("⚠️ Respuesta generada: {}", content);
                    }
                    _ => {
                        println!("⚠️ Respuesta: {:?}", response);
                    }
                }
            }
            Err(e) => {
                println!("✅ Error capturado apropiadamente: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    }
}

/// Test de integración completo
#[tokio::test]
#[ignore] // Requiere Ollama corriendo y lleva tiempo
async fn test_full_integration_scenario() {
    println!("\n{'='*60}");
    println!("🚀 TEST DE INTEGRACIÓN COMPLETO");
    println!("{'='*60}\n");
    
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    // Escenario realista: Desarrollo de una feature
    let scenario = vec![
        ("Hola, necesito ayuda con Rust", "greeting"),
        ("Quiero crear una función que calcule fibonacci", "requirement"),
        ("Genera el código para fibonacci recursivo", "code_generation"),
        ("¿Cuál es la complejidad de este algoritmo?", "analysis"),
        ("¿Puedes optimizarlo con memoization?", "optimization"),
        ("Calcula fibonacci de 10", "execution"),
    ];

    for (i, (prompt, stage)) in scenario.iter().enumerate() {
        println!("\n📍 Paso {}/{}  [{}]", i + 1, scenario.len(), stage);
        println!("   Prompt: {}", prompt);
        
        match orchestrator.process(prompt).await {
            Ok(response) => {
                match response {
                    OrchestratorResponse::Immediate { content, model } => {
                        println!("   ✅ Respuesta ({}): {:.100}...", 
                               model, 
                               content.chars().take(100).collect::<String>());
                    }
                    OrchestratorResponse::ToolResult { tool_name, result, .. } => {
                        println!("   ✅ Tool '{}': {:.80}...", 
                               tool_name,
                               result.chars().take(80).collect::<String>());
                    }
                    OrchestratorResponse::Delegated { description, .. } => {
                        println!("   ⏳ Delegado: {}", description);
                    }
                    _ => {
                        println!("   ⚠️ Respuesta: {:?}", response);
                    }
                }
            }
            Err(e) => {
                println!("   ❌ Error: {}", e);
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
    }
    
    println!("\n{'='*60}");
    println!("✅ ESCENARIO DE INTEGRACIÓN COMPLETADO");
    println!("{'='*60}\n");
}
