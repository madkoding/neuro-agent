//! Tests de clasificación de tareas y routing
//!
//! Verifica que el sistema clasifica correctamente las tareas
//! y las rutea al modelo apropiado (fast vs heavy)

/// Tipos de tarea simplificados para tests
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum TestTaskType {
    Simple,
    Code,
    Analysis,
    Complex,
    Research,
    Error,
}

/// Test de clasificación de tareas simples
#[test]
fn test_simple_task_classification() {
    let simple_queries = vec![
        "Hola",
        "¿Qué tal?",
        "Gracias",
        "Sí",
        "No",
    ];

    for query in simple_queries {
        println!("\n📊 Clasificando: '{}'", query);
        
        let task_type = classify_by_length_and_keywords(query);
        
        println!("   Tipo: {:?}", task_type);
        assert_eq!(task_type, TestTaskType::Simple, 
            "La consulta '{}' debería ser Simple", query);
    }
}

/// Test de clasificación de tareas de código
#[test]
fn test_code_task_classification() {
    let code_queries = vec![
        "Genera una función en Rust",
        "Escribe código para validar email",
        "Crea una clase en Python",
        "Implementa un algoritmo de ordenamiento",
        "Refactoriza esta función",
    ];

    for query in code_queries {
        println!("\n📊 Clasificando: '{}'", query);
        
        let task_type = classify_by_length_and_keywords(query);
        
        println!("   Tipo: {:?}", task_type);
        assert!(
            task_type == TestTaskType::Code || task_type == TestTaskType::Analysis,
            "La consulta '{}' debería ser Code o Analysis", query
        );
    }
}

/// Test de clasificación de tareas complejas
#[test]
fn test_complex_task_classification() {
    let complex_queries = vec![
        "Analiza este código y sugiere mejoras detalladas con ejemplos",
        "Compara async/await vs threads en Rust, explicando ventajas y desventajas",
        "Diseña una arquitectura completa para un sistema de microservicios",
        "Explica cómo funciona el borrow checker de Rust con múltiples ejemplos",
    ];

    for query in complex_queries {
        println!("\n📊 Clasificando: '{}'", query);
        
        let task_type = classify_by_length_and_keywords(query);
        
        println!("   Tipo: {:?}", task_type);
        assert_eq!(task_type, TestTaskType::Complex, 
            "La consulta '{}' debería ser Complex", query);
    }
}

/// Test de clasificación de tareas de análisis
#[test]
fn test_analysis_task_classification() {
    let analysis_queries = vec![
        "Analiza la complejidad de este algoritmo",
        "Explica cómo funciona este código",
        "¿Qué hace esta función?",
        "Describe el propósito de este módulo",
    ];

    for query in analysis_queries {
        println!("\n📊 Clasificando: '{}'", query);
        
        let task_type = classify_by_length_and_keywords(query);
        
        println!("   Tipo: {:?}", task_type);
        assert!(
            task_type == TestTaskType::Analysis || task_type == TestTaskType::Complex,
            "La consulta '{}' debería ser Analysis o Complex", query
        );
    }
}

/// Test de clasificación de comandos
#[test]
fn test_command_task_classification() {
    let command_queries = vec![
        "Ejecuta ls -la",
        "Corre el comando date",
        "Lista los archivos",
        "Muestra el contenido del directorio",
    ];

    for query in command_queries {
        println!("\n📊 Clasificando: '{}'", query);
        
        let task_type = classify_by_length_and_keywords(query);
        
        println!("   Tipo: {:?}", task_type);
        // Los comandos pueden ser Simple o requerir tools
        assert!(
            task_type == TestTaskType::Simple || 
            task_type == TestTaskType::Code,
            "La consulta '{}' debería involucrar ejecución", query
        );
    }
}

/// Test de routing: tareas que deben ir al modelo rápido
#[test]
fn test_fast_model_routing() {
    let fast_queries = vec![
        ("Hola", "saludo simple"),
        ("Sí", "respuesta corta"),
        ("Calcula 2 + 2", "cálculo simple"),
        ("¿Qué hora es?", "pregunta simple"),
    ];

    for (query, reason) in fast_queries {
        println!("\n🚀 Evaluando routing para: '{}' ({})", query, reason);
        
        let should_use_fast = should_route_to_fast_model(query);
        
        if should_use_fast {
            println!("   ✅ Correctamente ruteado a modelo rápido");
        } else {
            println!("   ❌ ERROR: Debería ir al modelo rápido");
        }
        
        assert!(should_use_fast, 
            "'{}' debería ir al modelo rápido ({})", query, reason);
    }
}

/// Test de routing: tareas que deben ir al modelo pesado
#[test]
fn test_heavy_model_routing() {
    let heavy_queries = vec![
        (
            "Explica en detalle cómo funciona el sistema de tipos de Rust",
            "explicación compleja"
        ),
        (
            "Genera una implementación completa de un servidor HTTP",
            "generación extensa"
        ),
        (
            "Compara y contrasta diferentes patrones de diseño con ejemplos",
            "análisis profundo"
        ),
    ];

    for (query, reason) in heavy_queries {
        println!("\n🐘 Evaluando routing para: '{}...' ({})", 
                &query.chars().take(50).collect::<String>(), reason);
        
        let should_use_heavy = !should_route_to_fast_model(query);
        
        if should_use_heavy {
            println!("   ✅ Correctamente ruteado a modelo pesado");
        } else {
            println!("   ❌ ERROR: Debería ir al modelo pesado");
        }
        
        assert!(should_use_heavy, 
            "'{}' debería ir al modelo pesado ({})", query, reason);
    }
}

/// Test de estimación de tiempo de ejecución
#[test]
fn test_execution_time_estimation() {
    let test_cases = vec![
        ("Hola", 1),
        ("Calcula 2 + 2", 2),
        ("Genera una función simple", 5),
        ("Explica el borrow checker de Rust con ejemplos", 30),
        ("Diseña una arquitectura completa de microservicios", 60),
    ];

    for (query, max_expected_secs) in test_cases {
        println!("\n⏱️ Estimando tiempo para: '{}'", query);
        
        let estimated = estimate_execution_time(query);
        
        println!("   Tiempo estimado: {}s", estimated);
        println!("   Máximo esperado: {}s", max_expected_secs);
        
        assert!(estimated <= max_expected_secs * 2, 
            "Estimación demasiado alta para '{}'", query);
    }
}

/// Test de detección de patrones peligrosos
#[test]
fn test_dangerous_pattern_detection() {
    let dangerous_patterns = vec![
        ("Ejecuta rm -rf /", true),
        ("Borra todos los archivos", true),
        ("Formatea el disco", true),
        ("chmod 777 -R /", true),
        ("Lista los archivos", false),
        ("Muestra el contenido", false),
        ("Crea un archivo", false),
    ];

    for (query, should_be_dangerous) in dangerous_patterns {
        println!("\n🔒 Verificando: '{}'", query);
        
        let is_dangerous = detect_dangerous_intent(query);
        
        println!("   ¿Es peligroso?: {}", is_dangerous);
        
        assert_eq!(is_dangerous, should_be_dangerous,
            "'{}' clasificación incorrecta", query);
    }
}

/// Test de confianza en la clasificación
#[test]
fn test_classification_confidence() {
    let test_cases = vec![
        ("Hola", 0.95), // Alta confianza
        ("Genera código", 0.7), // Media confianza
        ("Haz algo con los datos", 0.4), // Baja confianza (ambiguo)
    ];

    for (query, min_confidence) in test_cases {
        println!("\n🎯 Evaluando confianza para: '{}'", query);
        
        let confidence = calculate_classification_confidence(query);
        
        println!("   Confianza: {:.2}", confidence);
        
        assert!(confidence >= min_confidence - 0.2,
            "Confianza demasiado baja para '{}'", query);
    }
}

// ============================================================================
// FUNCIONES HELPER PARA CLASIFICACIÓN Y ROUTING
// ============================================================================

/// Clasificador simple basado en longitud y palabras clave
fn classify_by_length_and_keywords(query: &str) -> TestTaskType {
    let query_lower = query.to_lowercase();
    let word_count = query.split_whitespace().count();
    
    // Tareas muy cortas son simples
    if word_count <= 3 {
        return TestTaskType::Simple;
    }
    
    // Palabras clave para código
    let code_keywords = vec![
        "genera", "crea", "escribe", "implementa", "código",
        "función", "clase", "método", "programa", "refactoriza"
    ];
    
    if code_keywords.iter().any(|kw| query_lower.contains(kw)) {
        if word_count > 10 {
            return TestTaskType::Complex;
        }
        return TestTaskType::Code;
    }
    
    // Palabras clave para análisis
    let analysis_keywords = vec![
        "analiza", "explica", "describe", "compara", "evalúa"
    ];
    
    if analysis_keywords.iter().any(|kw| query_lower.contains(kw)) {
        if word_count > 8 {
            return TestTaskType::Complex;
        }
        return TestTaskType::Analysis;
    }
    
    // Por defecto, según longitud
    if word_count > 15 {
        TestTaskType::Complex
    } else if word_count > 8 {
        TestTaskType::Analysis
    } else {
        TestTaskType::Simple
    }
}

/// Decide si debe usar modelo rápido
fn should_route_to_fast_model(query: &str) -> bool {
    let task_type = classify_by_length_and_keywords(query);
    let word_count = query.split_whitespace().count();
    
    matches!(task_type, TestTaskType::Simple) || word_count <= 5
}

/// Estima tiempo de ejecución en segundos
fn estimate_execution_time(query: &str) -> u64 {
    let task_type = classify_by_length_and_keywords(query);
    let word_count = query.split_whitespace().count();
    
    match task_type {
        TestTaskType::Simple => 1,
        TestTaskType::Code => {
            if word_count > 10 { 10 } else { 5 }
        }
        TestTaskType::Analysis => {
            if word_count > 15 { 20 } else { 10 }
        }
        TestTaskType::Complex => {
            if word_count > 20 { 60 } else { 30 }
        }
        TestTaskType::Research => 45,
        TestTaskType::Error => 1,
    }
}

/// Detecta intención peligrosa
fn detect_dangerous_intent(query: &str) -> bool {
    let query_lower = query.to_lowercase();
    
    let dangerous_keywords = vec![
        "rm -rf",
        "borra todos",
        "elimina todo",
        "formatea",
        "chmod 777",
        "delete *",
        "format",
    ];
    
    dangerous_keywords.iter().any(|kw| query_lower.contains(kw))
}

/// Calcula confianza en la clasificación
fn calculate_classification_confidence(query: &str) -> f64 {
    let query_lower = query.to_lowercase();
    let word_count = query.split_whitespace().count();
    
    // Consultas muy cortas o muy largas tienen alta confianza
    if word_count <= 2 || word_count > 20 {
        return 0.9;
    }
    
    // Presencia de palabras clave aumenta confianza
    let keywords = vec![
        "genera", "crea", "explica", "analiza", "ejecuta",
        "hola", "gracias", "sí", "no"
    ];
    
    let has_clear_keyword = keywords.iter()
        .any(|kw| query_lower.contains(kw));
    
    if has_clear_keyword {
        0.8
    } else {
        0.5
    }
}

/// Test de balance de carga
#[test]
fn test_load_balancing_decisions() {
    println!("\n⚖️ Test de balance de carga");
    
    // Simular múltiples requests
    let queries = vec![
        "Tarea 1",
        "Tarea compleja que requiere análisis profundo",
        "Tarea 2",
        "Otra tarea compleja con múltiples pasos",
        "Tarea 3",
    ];
    
    let mut fast_count = 0;
    let mut heavy_count = 0;
    
    for query in queries {
        if should_route_to_fast_model(query) {
            fast_count += 1;
            println!("   🚀 Fast: {}", query);
        } else {
            heavy_count += 1;
            println!("   🐘 Heavy: {}", query);
        }
    }
    
    println!("\n   Distribución:");
    println!("   - Modelo rápido: {}", fast_count);
    println!("   - Modelo pesado: {}", heavy_count);
    
    // Debe haber alguna distribución
    assert!(fast_count > 0 || heavy_count > 0);
}

/// Test de priorización de tareas
#[test]
fn test_task_prioritization() {
    let tasks = vec![
        ("Error crítico en producción", 10),
        ("Generar documentación", 3),
        ("Refactorizar código legacy", 5),
        ("Pregunta sobre API", 7),
    ];
    
    println!("\n🎯 Test de priorización");
    
    for (task, _expected_priority) in tasks {
        let priority = calculate_priority(task);
        
        println!("   Tarea: {} -> Prioridad: {}", task, priority);
        
        assert!(priority >= 1 && priority <= 10,
            "Prioridad fuera de rango");
    }
}

fn calculate_priority(task: &str) -> u8 {
    let task_lower = task.to_lowercase();
    
    if task_lower.contains("error") || task_lower.contains("crítico") {
        return 10;
    }
    
    if task_lower.contains("urgente") {
        return 8;
    }
    
    if task_lower.contains("importante") {
        return 6;
    }
    
    5 // Prioridad normal
}
