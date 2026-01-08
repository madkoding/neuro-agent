# Tests Funcionales de Neuro Agent

Este directorio contiene una suite completa de tests funcionales para verificar que el modelo de IA responde correctamente a diferentes tipos de solicitudes.

## 📋 Estructura de Tests

### 1. `functional_tests.rs` - Tests de Integración Principal
Tests completos end-to-end que verifican la funcionalidad del agente:

- **✅ Test 1: Chat Conversacional** (`test_simple_chat`)
  - Prueba respuestas a saludos y preguntas simples
  - Verifica que el agente puede mantener conversaciones básicas

- **✅ Test 2: Procesamiento de Texto** (`test_text_processing`)
  - Resúmenes de texto
  - Traducción
  - Análisis de sentimiento
  - Corrección gramatical

- **✅ Test 3: Operaciones Aritméticas** (`test_arithmetic_operations`)
  - Suma, resta, multiplicación, división
  - Funciones matemáticas (raíz cuadrada, etc.)
  - Verifica el uso de la tool `calculator`

- **✅ Test 4: Generación de Código** (`test_code_generation`)
  - Funciones en Rust
  - Ejemplos de async/await
  - Código en Python y JavaScript
  - Snippets útiles

- **✅ Test 5: Comprensión de Contexto** (`test_context_comprehension`)
  - Mantener contexto entre mensajes
  - Recordar información previa
  - Respuestas contextuales

- **✅ Test 6: Edición de Archivos** (`test_file_editing`)
  - Leer archivos
  - Escribir contenido
  - Verificar existencia de archivos

- **✅ Test 7: Comandos de Terminal** (`test_terminal_commands`)
  - Ejecución de comandos seguros
  - Detección de comandos peligrosos
  - Confirmación de operaciones riesgosas

- **✅ Test 8: Uso de Herramientas** (`test_specific_tools`)
  - Calculator
  - Search
  - Analyzer
  - Formatter

- **✅ Test 9: Tareas Multi-paso** (`test_complex_multistep_task`)
  - Análisis complejo de código
  - Generación con explicaciones
  - Comparaciones detalladas

- **✅ Test 10: Manejo de Errores** (`test_error_handling`)
  - Prompts vacíos
  - Prompts muy largos
  - Comandos peligrosos
  - Requests inseguros

- **✅ Test 11: Integración Completa** (`test_full_integration_scenario`)
  - Escenario realista de desarrollo
  - Múltiples interacciones secuenciales

### 2. `tool_tests.rs` - Tests de Herramientas
Tests unitarios para cada herramienta individual:

- Calculator Tool
- File Read/Write Tools
- List Directory Tool
- Shell Execute Tool
- Git Operations
- Search Tool
- Formatter Tool
- Analyzer Tool
- Documentation Extraction
- Test Runner
- Context Gathering
- Dependency Analysis

### 3. `classification_tests.rs` - Tests de Clasificación y Routing
Tests del sistema de clasificación inteligente:

- Clasificación de tareas simples
- Clasificación de código
- Clasificación de tareas complejas
- Routing al modelo rápido
- Routing al modelo pesado
- Estimación de tiempos
- Detección de patrones peligrosos
- Balance de carga
- Priorización de tareas

## 🚀 Ejecutar los Tests

### Ejecutar todos los tests
```bash
cargo test
```

### Ejecutar tests específicos
```bash
# Tests de clasificación (no requieren Ollama)
cargo test --test classification_tests

# Tests de herramientas (no requieren Ollama)
cargo test --test tool_tests

# Tests funcionales (requieren Ollama corriendo)
cargo test --test functional_tests -- --ignored
```

### Ejecutar un test individual
```bash
# Test específico de chat
cargo test --test functional_tests test_simple_chat -- --ignored --nocapture

# Test de cálculo aritmético
cargo test --test functional_tests test_arithmetic_operations -- --ignored --nocapture

# Test de generación de código
cargo test --test functional_tests test_code_generation -- --ignored --nocapture
```

### Ver output detallado
```bash
# Mostrar println! en tests
cargo test -- --nocapture

# Mostrar solo tests que fallan
cargo test -- --test-threads=1
```

## ⚙️ Configuración Requerida

### Para Tests Funcionales (marcados con `#[ignore]`)

**Requiere Ollama corriendo:**
```bash
# Iniciar Ollama
ollama serve

# En otra terminal, descargar modelos
ollama pull qwen3:0.6b
ollama pull qwen3:8b
```

**Configuración opcional:**
Crea `config.json` en la raíz del proyecto:
```json
{
  "fast_model": {
    "provider": "ollama",
    "url": "http://localhost:11434",
    "model": "qwen3:0.6b",
    "temperature": 0.7
  },
  "heavy_model": {
    "provider": "ollama",
    "url": "http://localhost:11434",
    "model": "qwen3:8b",
    "temperature": 0.7
  }
}
```

### Para Tests de Herramientas y Clasificación

No requieren servicios externos, se ejecutan automáticamente con:
```bash
cargo test
```

## 📊 Cobertura de Tests

| Categoría | Tests | Estado |
|-----------|-------|--------|
| Chat Simple | 3 prompts | ✅ |
| Procesamiento Texto | 4 escenarios | ✅ |
| Aritmética | 5 operaciones | ✅ |
| Generación Código | 4 lenguajes | ✅ |
| Comprensión Contexto | 3 seguimientos | ✅ |
| Edición Archivos | 3 operaciones | ✅ |
| Comandos Terminal | 3 comandos | ✅ |
| Herramientas | 4 tools | ✅ |
| Multi-paso | 3 escenarios | ✅ |
| Manejo Errores | 5 casos límite | ✅ |
| **Total** | **37+ tests** | ✅ |

## 🐛 Depuración

### Si los tests fallan:

1. **Verificar Ollama está corriendo:**
```bash
curl http://localhost:11434/api/tags
```

2. **Verificar modelos descargados:**
```bash
ollama list
```

3. **Ver logs detallados:**
```bash
RUST_LOG=debug cargo test --test functional_tests -- --ignored --nocapture
```

4. **Test individual con tracing:**
```bash
RUST_LOG=neuro=debug cargo test --test functional_tests test_simple_chat -- --ignored --nocapture
```

## 📝 Agregar Nuevos Tests

### Plantilla para test funcional:
```rust
#[tokio::test]
#[ignore] // Si requiere Ollama
async fn test_mi_funcionalidad() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    
    let prompt = "Mi pregunta al modelo";
    
    match orchestrator.process(prompt).await {
        Ok(response) => {
            match response {
                OrchestratorResponse::Immediate { content, .. } => {
                    assert!(!content.is_empty());
                    // Tus verificaciones aquí
                }
                _ => {}
            }
        }
        Err(e) => {
            panic!("Error: {}", e);
        }
    }
}
```

### Plantilla para test de tool:
```rust
#[tokio::test]
async fn test_mi_tool() {
    // Setup
    let temp_dir = TempDir::new().unwrap();
    
    // Acción
    let result = mi_operacion();
    
    // Verificación
    assert!(result.is_ok());
}
```

### Plantilla para test de clasificación:
```rust
#[test]
fn test_mi_clasificacion() {
    let queries = vec!["pregunta 1", "pregunta 2"];
    
    for query in queries {
        let task_type = classify_by_length_and_keywords(query);
        assert_eq!(task_type, TestTaskType::Expected);
    }
}
```

## 🔍 Notas Importantes

1. **Tests marcados con `#[ignore]`** requieren Ollama corriendo y deben ejecutarse explícitamente con `-- --ignored`

2. **Timeouts:** Los tests con el modelo pesado pueden tardar hasta 60 segundos

3. **Recursos:** Los tests funcionales completos consumen ~500MB de RAM

4. **Orden:** Los tests se ejecutan en paralelo por defecto. Usa `--test-threads=1` para ejecución secuencial

5. **Limpieza:** Los archivos temporales se limpian automáticamente usando `tempfile`

## 📈 CI/CD

Para integración continua, ejecuta solo tests que no requieren Ollama:
```bash
# Tests rápidos (sin Ollama)
cargo test --lib
cargo test --test tool_tests
cargo test --test classification_tests

# Tests completos (con Ollama en CI)
cargo test -- --ignored --test-threads=1
```

## 📚 Referencias

- [Documentación de Cargo Test](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [Testing en Rust Book](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Tokio Testing](https://tokio.rs/tokio/topics/testing)
- [Ollama API](https://github.com/ollama/ollama/blob/main/docs/api.md)

## 🤝 Contribuir

Para agregar tests:
1. Identifica el tipo de test (funcional, tool, clasificación)
2. Sigue la plantilla correspondiente
3. Agrega documentación clara
4. Ejecuta `cargo test` para verificar
5. Actualiza este README si es necesario

---

**Última actualización:** 7 de enero de 2026
**Versión de tests:** 1.0.0
**Compatibilidad:** neuro v0.1.0
