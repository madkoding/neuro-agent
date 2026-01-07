# Neuro - AI Programming Assistant

Neuro es un asistente de programación con IA que combina un sistema de orquestación dual de modelos con capacidades avanzadas de análisis de código y RAG (Retrieval-Augmented Generation).

## Características

- 🧠 **Orquestación Dual de Modelos**: Modelo rápido para tareas simples y modelo pesado para tareas complejas
- 📊 **Planning Orchestrator**: Sistema de planificación de tareas con ejecución paso a paso
- 🔍 **RAPTOR Integration**: Indexación recursiva para búsqueda semántica mejorada
- 🎨 **TUI Moderna**: Interfaz de terminal con ratatui
- 🛠️ **Múltiples Herramientas**: Análisis de código, linting, git, búsqueda semántica, refactoring y más
- 🌐 **Soporte i18n**: Interfaz multiidioma (inglés/español)

## Requisitos

- Rust 1.70+
- Ollama server corriendo localmente
- Modelos Ollama: qwen3:8b (o configurar otros modelos)

## Instalación

```bash
cargo build --release
```

## Uso

```bash
# Iniciar la aplicación
cargo run --release

# Con configuración personalizada
cargo run --release -- --fast-model qwen3:8b --heavy-model qwen3:8b
```

## Arquitectura

- **DualModelOrchestrator**: Orquestación básica con routing inteligente
- **PlanningOrchestrator**: Sistema de planificación y ejecución de tareas
- **RAPTOR**: Indexación jerárquica para RAG
- **Tool Registry**: Sistema extensible de herramientas

## Licencia

MIT
