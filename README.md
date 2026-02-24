# Neuro - AI Programming Assistant

Neuro es un asistente de programación con IA que combina un sistema de orquestación dual de modelos con capacidades avanzadas de análisis de código y RAG (Retrieval-Augmented Generation).

## 🎯 Project Status

**Current Milestone**: ✅ **50% COMPLETE** (Sprint 4)

| Sprint | Status | Features | Lines | Tests | Completion |
|--------|--------|----------|-------|-------|------------|
| Sprint 4 | ✅ **COMPLETE** | 5/5 | 3,236 | 46/46 | **100%** |
| Sprint 5 | 🔄 Planning | TBD | TBD | TBD | 0% |
| **Project** | 🚀 **In Progress** | **5/10** | **3,236+** | **219+** | **50%** |

**Latest Achievement**: Sprint 4 delivered 5 production-ready features including smart error recovery, code review mode, context preloading, performance benchmarking, and production monitoring. All features validated with 100% test pass rate.

📖 See [SPRINT4_FINAL_REPORT.md](SPRINT4_FINAL_REPORT.md) for detailed breakdown.

---

## Características

### Core Features
- 🧠 **Orquestación Dual de Modelos**: Modelo rápido para tareas simples y modelo pesado para tareas complejas
- 🌐 **Múltiples Proveedores**: Soporte para Ollama (local), OpenAI, Anthropic y Groq
- ⚙️ **Configuración JSON**: Sistema flexible de configuración por entorno
- 📊 **Planning Orchestrator**: Sistema de planificación de tareas con ejecución paso a paso
- 🔍 **RAPTOR Integration**: Indexación recursiva para búsqueda semántica mejorada
- 🎨 **TUI Moderna**: Interfaz de terminal con ratatui
- 🛠️ **Múltiples Herramientas**: Análisis de código, linting, git, búsqueda semántica, refactoring y más
- 🌐 **Soporte i18n**: Interfaz multiidioma (inglés/español)

### Sprint 4 Features ✨ (NEW)
- 🔄 **Smart Error Recovery**: Sistema de recuperación automática con retry y rollback (600 lines, 9 tests)
- 🔍 **Code Review Mode**: Análisis AST con detección de complejidad y code smells (887 lines, 10 tests)
- ⚡ **Context Preloading**: Caché LRU para respuestas 10x más rápidas (547 lines, 9 tests)
- 📊 **Performance Benchmarks**: Framework de benchmarking con detección de regresiones (536 lines, 10 tests)
- 🔥 **Production Monitoring**: Sistema de monitoreo con métricas en tiempo real (666 lines, 8 tests)

## Requisitos

- Rust 1.70+
- Uno de los siguientes proveedores de modelos:
  - Ollama server corriendo localmente (recomendado para desarrollo)
  - API key de OpenAI, Anthropic o Groq

## Instalación

```bash
cargo build --release
```

## Configuración

Neuro soporta configuración a través de archivos JSON. La configuración se carga con la siguiente prioridad:

1. Archivo especificado con `--config`
2. `~/.config/neuro/config.{NEURO_ENV}.json` (donde NEURO_ENV=production|development|test)
3. Valores por defecto

### Archivo de Configuración

Crea `~/.config/neuro/config.production.json`:

```json
{
  "fast_model": {
    "provider": "ollama",
    "url": "http://localhost:11434",
    "model": "qwen3:0.6b",
    "temperature": 0.7,
    "top_p": 0.95
  },
  "heavy_model": {
    "provider": "ollama",
    "url": "http://localhost:11434",
    "model": "qwen3:8b",
    "temperature": 0.7,
    "top_p": 0.95
  },
  "heavy_timeout_secs": 1200,
  "max_concurrent_heavy": 2
}
```

### Proveedores Soportados

#### Ollama (Local)

```json
{
  "provider": "ollama",
  "url": "http://localhost:11434",
  "model": "qwen3:8b",
  "temperature": 0.7,
  "top_p": 0.95
}
```

#### OpenAI

```json
{
  "provider": "openai",
  "url": "https://api.openai.com/v1",
  "model": "gpt-4o-mini",
  "api_key": "OPENAI_API_KEY",
  "temperature": 0.7,
  "top_p": 0.95,
  "max_tokens": 4096
}
```

Configura la variable de entorno:
```bash
export OPENAI_API_KEY="sk-..."
```

#### Anthropic

```json
{
  "provider": "anthropic",
  "url": "https://api.anthropic.com/v1",
  "model": "claude-3-5-sonnet-20241022",
  "api_key": "ANTHROPIC_API_KEY",
  "temperature": 0.7,
  "top_p": 0.95,
  "max_tokens": 8192
}
```

Configura la variable de entorno:
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

#### Groq

```json
{
  "provider": "groq",
  "url": "https://api.groq.com/openai/v1",
  "model": "llama-3.3-70b-versatile",
  "api_key": "GROQ_API_KEY",
  "temperature": 0.7,
  "top_p": 0.95,
  "max_tokens": 8192
}
```

Configura la variable de entorno:
```bash
export GROQ_API_KEY="gsk_..."
```

### Variables de Entorno

Las siguientes variables de entorno pueden sobrescribir la configuración:

- `NEURO_ENV`: Entorno de configuración (production|development|test, default: production)
- `NEURO_OLLAMA_URL`: URL del servidor Ollama
- `NEURO_FAST_MODEL`: Nombre del modelo rápido
- `NEURO_HEAVY_MODEL`: Nombre del modelo pesado
- `OPENAI_API_KEY`: API key de OpenAI
- `ANTHROPIC_API_KEY`: API key de Anthropic
- `GROQ_API_KEY`: API key de Groq

### Ejemplo: Configuración Mixta

Puedes usar diferentes proveedores para cada modelo:

```json
{
  "fast_model": {
    "provider": "ollama",
    "url": "http://localhost:11434",
    "model": "qwen3:0.6b",
    "temperature": 0.7,
    "top_p": 0.95
  },
  "heavy_model": {
    "provider": "anthropic",
    "url": "https://api.anthropic.com/v1",
    "model": "claude-3-5-sonnet-20241022",
    "api_key": "ANTHROPIC_API_KEY",
    "temperature": 0.7,
    "top_p": 0.95,
    "max_tokens": 8192
  }
}
```

## Uso

```bash
# Usar configuración por defecto
cargo run --release

# Usar archivo de configuración específico
cargo run --release -- --config ./config.json

# Usar variables de entorno para desarrollo
export NEURO_ENV=development
cargo run --release

# Backward compatibility: parámetros CLI (deprecated)
cargo run --release -- --fast-model qwen3:8b --heavy-model qwen3:8b
```

### Interfaz de Usuario (TUI)

Una vez dentro de la aplicación, puedes navegar entre las diferentes pantallas:

```
┌─────────────────────────────────────────────────┐
│                                                 │
│         Chat (Pantalla Principal)               │
│                                                 │
│  • Interactúa con el asistente                  │
│  • Enter: Enviar mensaje                        │
│  • ↑↓: Scroll                                   │
│                                                 │
│         Tab ↓                                   │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│                                                 │
│         Settings (Herramientas)                 │
│                                                 │
│  • ↑↓: Navegar                                  │
│  • Space/Enter: Toggle herramienta              │
│  • Esc: ← Volver a Chat                         │
│                                                 │
│         Tab ↓                                   │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│                                                 │
│    ⚙️  Model Config (Configuración)            │
│                                                 │
│  • ↑↓: Navegar entre campos                     │
│  • Enter: Editar campo/Activar botón            │
│  • ←→: Cambiar provider                         │
│  • Tab: ← Volver a Chat                         │
│  • Esc: Cancelar edición                        │
│                                                 │
└─────────────────────────────────────────────────┘
```

- **Chat (pantalla principal)**: Interactúa con el asistente
  - `Enter`: Enviar mensaje
  - `↑↓`: Scroll en el chat
  - `Tab`: Ir a Settings (herramientas)
  
- **Settings**: Habilitar/deshabilitar herramientas disponibles
  - `↑↓`: Navegar entre herramientas
  - `Space/Enter`: Activar/desactivar herramienta
  - `Tab`: Ir a Model Config (configuración de modelos)
  - `Esc`: Volver a Chat
  
- **Model Config** ⚙️: Configurar proveedores y modelos interactivamente
  - `↑↓`: Navegar entre campos
  - `Enter`: Editar campo o activar botón
  - `←→`: Cambiar proveedor (en campos de provider)
  - `Tab`: Volver a Chat
  - `Esc`: Cancelar edición o volver a Chat
  - Botones disponibles:
    - 💾 **Save Configuration**: Guardar cambios
    - 🔌 **Test Connection**: Probar conexión con el proveedor

### Configuración de Modelos en UI

Para cambiar los modelos desde la interfaz:

1. Presiona `Tab` en la pantalla principal para ir a **Settings**
2. Presiona `Tab` nuevamente para ir a **Model Config**
3. Navega con `↑↓` entre los campos:
   - Fast Model: Provider, URL, Modelo, API Key, Temperature, Top P
   - Heavy Model: Provider, URL, Modelo, API Key, Temperature, Top P
4. Presiona `Enter` para editar un campo
5. Para cambiar el provider, usa `←→` en el campo "Provider"
6. Presiona `Enter` en "💾 Save Configuration" para guardar
7. Los cambios requieren reiniciar la aplicación para aplicarse

## Arquitectura

- **DualModelOrchestrator**: Orquestación básica con routing inteligente
- **PlanningOrchestrator**: Sistema de planificación y ejecución de tareas
- **RAPTOR**: Indexación jerárquica para RAG
- **Tool Registry**: Sistema extensible de herramientas

## Tests

Neuro incluye una suite completa de tests funcionales para verificar el correcto funcionamiento del sistema:

### 🧪 Suite de Tests

- **36+ tests funcionales** organizados en 3 archivos
- **40+ casos de prueba** cubriendo todas las funcionalidades
- **Tests de integración** end-to-end con modelos reales
- **Tests unitarios** de herramientas individuales
- **Tests de clasificación** y routing inteligente

### 🚀 Ejecutar Tests

```bash
# Verificar configuración
./run_tests.sh check

# Tests rápidos (sin Ollama)
./run_tests.sh fast

# Tests funcionales completos (requiere Ollama)
./run_tests.sh functional

# Test específico
./run_tests.sh chat          # Solo chat conversacional
./run_tests.sh arithmetic    # Solo operaciones matemáticas
./run_tests.sh code          # Solo generación de código
```

### 📚 Documentación de Tests

- **[tests/QUICKSTART.md](tests/QUICKSTART.md)** - Inicio rápido
- **[tests/README.md](tests/README.md)** - Documentación completa
- **[tests/EXAMPLES.md](tests/EXAMPLES.md)** - Ejemplos de código
- **[tests/TEST_SUMMARY.md](tests/TEST_SUMMARY.md)** - Resumen técnico
- **[tests/VISUAL_MAP.md](tests/VISUAL_MAP.md)** - Mapa visual

### Categorías Cubiertas

- ✅ Chat conversacional
- ✅ Procesamiento de texto
- ✅ Operaciones aritméticas
- ✅ Generación de código (Rust, Python, JS)
- ✅ Comprensión de contexto
- ✅ Edición de archivos
- ✅ Comandos de terminal
- ✅ Uso de herramientas (tools)
- ✅ Tareas multi-paso
- ✅ Manejo de errores
- ✅ Seguridad y validaciones

## Licencia

MIT

<!-- AUTO-UPDATE-DATE -->
**Última actualización:** 2026-02-24 11:38:13 -03
