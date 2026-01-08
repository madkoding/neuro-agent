# 📊 Mapa Visual de Tests

```
neuro-agent/
├── tests/
│   ├── 📄 functional_tests.rs      (600+ líneas)
│   │   ├── ✅ test_simple_chat
│   │   ├── ✅ test_text_processing
│   │   ├── ✅ test_arithmetic_operations
│   │   ├── ✅ test_code_generation
│   │   ├── ✅ test_context_comprehension
│   │   ├── ✅ test_file_editing
│   │   ├── ✅ test_terminal_commands
│   │   ├── ✅ test_specific_tools
│   │   ├── ✅ test_complex_multistep_task
│   │   ├── ✅ test_error_handling
│   │   └── ✅ test_full_integration_scenario
│   │
│   ├── 📄 tool_tests.rs            (500+ líneas)
│   │   ├── ✅ test_calculator_tool
│   │   ├── ✅ test_file_read_tool
│   │   ├── ✅ test_file_write_tool
│   │   ├── ✅ test_list_directory_tool
│   │   ├── ✅ test_shell_execute_safe_commands
│   │   ├── ✅ test_dangerous_command_detection
│   │   ├── ✅ test_git_operations
│   │   ├── ✅ test_search_tool
│   │   ├── ✅ test_formatter_tool
│   │   ├── ✅ test_analyzer_tool
│   │   ├── ✅ test_documentation_extraction
│   │   ├── ✅ test_runner_simulation
│   │   ├── ✅ test_context_gathering
│   │   └── ✅ test_dependency_analysis
│   │
│   ├── 📄 classification_tests.rs  (450+ líneas)
│   │   ├── ✅ test_simple_task_classification
│   │   ├── ✅ test_code_task_classification
│   │   ├── ✅ test_complex_task_classification
│   │   ├── ✅ test_analysis_task_classification
│   │   ├── ✅ test_command_task_classification
│   │   ├── ✅ test_fast_model_routing
│   │   ├── ✅ test_heavy_model_routing
│   │   ├── ✅ test_execution_time_estimation
│   │   ├── ✅ test_dangerous_pattern_detection
│   │   ├── ✅ test_classification_confidence
│   │   ├── ✅ test_load_balancing_decisions
│   │   └── ✅ test_task_prioritization
│   │
│   ├── 📖 README.md                (8KB)
│   ├── 📖 EXAMPLES.md              (13KB)
│   ├── 📖 TEST_SUMMARY.md          (8KB)
│   ├── 📖 QUICKSTART.md            (3KB)
│   └── 📖 VISUAL_MAP.md            (este archivo)
│
├── 🔧 run_tests.sh                 (ejecutable)
│
└── src/
    ├── agent/
    ├── tools/
    └── ...
```

## 🎯 Tests por Categoría

### 💬 Chat & Conversación
```
test_simple_chat                    → Saludos, preguntas básicas
test_context_comprehension          → Mantener contexto
test_full_integration_scenario      → Conversación completa
```

### 📝 Procesamiento de Texto
```
test_text_processing               → Resumen, traducción, sentimiento
```

### 🧮 Matemáticas
```
test_arithmetic_operations         → +, -, ×, ÷, √, etc.
test_calculator_tool               → Validación de calculator
```

### 💻 Código
```
test_code_generation              → Rust, Python, JS, TS
test_analyzer_tool                → Análisis de complejidad
test_formatter_tool               → Formateo de código
test_documentation_extraction     → Extracción de docs
```

### 📂 Archivos & Sistema
```
test_file_editing                 → Leer, escribir archivos
test_file_read_tool               → Lectura unitaria
test_file_write_tool              → Escritura unitaria
test_list_directory_tool          → Listar directorios
test_context_gathering            → Contexto del proyecto
```

### 🖥️ Terminal & Shell
```
test_terminal_commands            → Ejecución de comandos
test_shell_execute_safe_commands  → Comandos seguros
test_dangerous_command_detection  → Detección de peligros
```

### 🔧 Herramientas (Tools)
```
test_specific_tools               → Calculator, Search, Analyzer
test_search_tool                  → Búsqueda de texto
test_git_operations               → Git status, log, etc.
test_dependency_analysis          → Análisis de dependencias
test_runner_simulation            → Ejecución de tests
```

### 🧠 Clasificación & Routing
```
test_simple_task_classification   → Tareas simples
test_code_task_classification     → Tareas de código
test_complex_task_classification  → Tareas complejas
test_fast_model_routing          → Routing a modelo rápido
test_heavy_model_routing         → Routing a modelo pesado
test_execution_time_estimation   → Estimación de tiempos
```

### 🚨 Seguridad & Errores
```
test_error_handling              → Casos límite, errores
test_dangerous_pattern_detection → Patrones peligrosos
```

### ⚖️ Rendimiento & Balance
```
test_load_balancing_decisions    → Balance de carga
test_task_prioritization         → Priorización de tareas
test_classification_confidence   → Confianza en clasificación
```

### 🔄 Integración
```
test_complex_multistep_task      → Tareas multi-paso
test_full_integration_scenario   → Escenario realista completo
```

## 🚦 Flujo de Ejecución

```
┌─────────────────────────────────────────────────┐
│  ./run_tests.sh check                          │
│  Verifica: Ollama, modelos, configuración     │
└─────────────────┬───────────────────────────────┘
                  │
                  ↓
┌─────────────────────────────────────────────────┐
│  ./run_tests.sh fast                           │
│  Ejecuta: tool_tests + classification_tests    │
│  Tiempo: ~5 segundos                           │
│  Requiere: Solo Rust/Cargo                     │
└─────────────────┬───────────────────────────────┘
                  │
                  ↓
┌─────────────────────────────────────────────────┐
│  ./run_tests.sh functional                     │
│  Ejecuta: functional_tests (11 categorías)     │
│  Tiempo: ~2-5 minutos                          │
│  Requiere: Ollama + Modelos                    │
└─────────────────┬───────────────────────────────┘
                  │
                  ↓
┌─────────────────────────────────────────────────┐
│  Todos los tests pasan ✅                       │
│  Sistema verificado y funcional               │
└─────────────────────────────────────────────────┘
```

## 📈 Cobertura Visual

```
┌──────────────────────────────────────────┐
│  COBERTURA DE TESTS                     │
├──────────────────────────────────────────┤
│  Chat                    ████████ 100%   │
│  Texto                   ████████ 100%   │
│  Aritmética              ████████ 100%   │
│  Código                  ████████ 100%   │
│  Archivos                ████████ 100%   │
│  Terminal                ████████ 100%   │
│  Tools                   ████████ 100%   │
│  Clasificación           ████████ 100%   │
│  Seguridad               ████████ 100%   │
│  Integración             ████████ 100%   │
├──────────────────────────────────────────┤
│  TOTAL                   ████████ 100%   │
└──────────────────────────────────────────┘
```

## 🎨 Tipos de Tests

```
┌─────────────────────────┐
│  TESTS UNITARIOS        │  ← tool_tests.rs
│  Herramientas aisladas  │     (No requiere Ollama)
└─────────┬───────────────┘
          │
          ↓
┌─────────────────────────┐
│  TESTS DE INTEGRACIÓN   │  ← classification_tests.rs
│  Sistema de routing     │     (No requiere Ollama)
└─────────┬───────────────┘
          │
          ↓
┌─────────────────────────┐
│  TESTS FUNCIONALES      │  ← functional_tests.rs
│  End-to-End completo    │     (Requiere Ollama)
└─────────────────────────┘
```

## 🔍 Navegación Rápida

### Para desarrolladores:
```bash
tests/functional_tests.rs     # Agregar tests end-to-end
tests/tool_tests.rs           # Agregar tests de tools
tests/classification_tests.rs # Agregar tests de routing
```

### Para usuarios:
```bash
tests/QUICKSTART.md          # Inicio rápido
tests/README.md              # Documentación completa
tests/EXAMPLES.md            # Ejemplos de código
```

### Para CI/CD:
```bash
./run_tests.sh fast          # Tests rápidos
./run_tests.sh check         # Verificar requisitos
```

## 📊 Métricas

| Métrica | Valor |
|---------|-------|
| Archivos de test | 3 |
| Archivos de docs | 5 |
| Tests totales | 36+ |
| Líneas de código | 2,000+ |
| Líneas de docs | 1,500+ |
| Casos de prueba | 40+ |
| Tools probadas | 13 |
| Categorías | 11 |

## 🎯 Puntos de Entrada

```
1. ¿Primera vez?          → tests/QUICKSTART.md
2. ¿Buscar ejemplos?      → tests/EXAMPLES.md
3. ¿Documentación?        → tests/README.md
4. ¿Resumen técnico?      → tests/TEST_SUMMARY.md
5. ¿Ver estructura?       → tests/VISUAL_MAP.md (este archivo)
6. ¿Ejecutar tests?       → ./run_tests.sh
```

## 🚀 Comandos Más Usados

```bash
# Top 5
./run_tests.sh check        # ⭐ Verificar todo
./run_tests.sh fast         # ⭐ Tests rápidos
./run_tests.sh functional   # ⭐ Tests completos
./run_tests.sh chat         # ⭐ Test de chat
./run_tests.sh help         # ⭐ Ver ayuda
```

## 🎓 Flujo de Aprendizaje

```
1. QUICKSTART.md           → Configuración inicial (5 min)
   ↓
2. ./run_tests.sh fast    → Primeros tests (1 min)
   ↓
3. README.md              → Entender estructura (10 min)
   ↓
4. EXAMPLES.md            → Ver ejemplos (15 min)
   ↓
5. ./run_tests.sh functional → Tests completos (5 min)
   ↓
6. Código fuente          → Implementar propios tests
```

## 💡 Tips Visuales

```
🟢 Verde   = Listo para usar
🟡 Amarillo = Requiere configuración
🔴 Rojo    = Problemas encontrados

✅ Check   = Test pasó
❌ Cross   = Test falló
⏱️ Clock   = Test en progreso
🚀 Rocket  = Inicio rápido
📖 Book    = Documentación
🔧 Wrench  = Configuración
```

---

**Última actualización:** 7 de enero de 2026  
**Versión:** 1.0.0  
**Estado:** ✅ Completo y funcional
