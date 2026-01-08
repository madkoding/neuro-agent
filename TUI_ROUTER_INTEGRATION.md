# Integración TUI con RouterOrchestrator

## Resumen

Se ha completado la integración del **RouterOrchestrator** con la interfaz TUI (Terminal User Interface), permitiendo que el nuevo sistema de enrutamiento simplificado sea utilizable a través de la interfaz interactiva moderna.

## Cambios Realizados

### 1. Wrapper de Orquestadores (`OrchestratorWrapper`)

Se creó un enum para envolver ambos tipos de orquestadores:

```rust
pub enum OrchestratorWrapper {
    Planning(PlanningOrchestrator),
    Router(RouterOrchestrator),
}
```

Este wrapper permite que `ModernApp` trabaje con cualquiera de los dos orquestadores sin necesidad de duplicar código.

### 2. Nuevos Constructores en `ModernApp`

Se agregaron dos métodos de construcción:

- **`new(orchestrator: PlanningOrchestrator)`**: Constructor original para compatibilidad hacia atrás
- **`new_with_router(orchestrator: RouterOrchestrator)`**: Nuevo constructor para usar RouterOrchestrator
- **`new_internal(orchestrator: OrchestratorWrapper)`**: Constructor interno compartido

### 3. Procesamiento Adaptativo

El método `start_processing()` ahora detecta automáticamente el tipo de orquestrador y llama al método apropiado:

- **PlanningOrchestrator**: Usa `process_with_planning_and_progress()` con seguimiento detallado de tareas
- **RouterOrchestrator**: Usa `process()` simple, ya que el router maneja la lógica internamente

### 4. Comando `!reindex`

Se implementó el comando especial `!reindex` que permite reconstruir el índice RAPTOR:

```rust
// En el chat, escribe:
!reindex
```

**Comportamiento:**
- Con **RouterOrchestrator**: Ejecuta `rebuild_raptor()` en segundo plano y muestra el progreso
- Con **PlanningOrchestrator**: Muestra mensaje de que el comando no está disponible

### 5. Gestión de RAPTOR en Background

El método `start_background_raptor_indexing()` ahora maneja ambos orquestadores:

- **PlanningOrchestrator**: Ejecuta indexación progresiva con actualizaciones de estado
- **RouterOrchestrator**: Reconoce que RAPTOR ya está inicializado (se hace en el constructor)

### 6. Activación en `main.rs`

Se descomentó y activó la función `run_modern_tui_with_router()`:

```rust
if args.simple {
    eprintln!("Simple mode not yet supported with RouterOrchestrator");
    return Ok(());
} else {
    run_modern_tui_with_router(router).await
}
```

## Uso

### Con RouterOrchestrator (recomendado)

1. Configurar en `config.json`:
```json
{
  "use_router_orchestrator": true
}
```

O establecer variable de entorno:
```bash
export NEURO_USE_ROUTER=true
```

2. Ejecutar:
```bash
./target/release/neuro
```

### Con PlanningOrchestrator (deprecado)

1. Configurar en `config.json`:
```json
{
  "use_router_orchestrator": false
}
```

2. Ejecutar:
```bash
./target/release/neuro
```

## Características del TUI

### Modo de Entrada (Ctrl+N)

El TUI soporta 4 modos de entrada que afectan cómo RouterOrchestrator clasifica las consultas:

- **🗨️ Chat (default)**: Conversación general, preguntas de contexto
- **🔨 Build**: Construcción, implementación, refactorización
- **🔍 Ask**: Análisis, explicación, investigación
- **📋 Plan**: Planificación de múltiples tareas

### Comandos Especiales

- **`!reindex`**: Reconstruye el índice RAPTOR (solo con RouterOrchestrator)
- **Tab**: Navegar entre pantallas (Chat → Settings → ModelConfig)
- **Ctrl+Q**: Salir de la aplicación
- **Ctrl+C** (doble): Salir forzado

### Atajos de Teclado

**En Chat:**
- `Enter`: Enviar mensaje
- `Ctrl+N`: Cambiar modo de entrada
- `Tab`: Ir a Settings
- `PageUp/PageDown`: Desplazar chat
- `Home/End`: Ir a inicio/fin del chat

**En Settings:**
- `Up/Down`: Navegar herramientas
- `Space/Enter`: Activar/desactivar herramienta
- `L`: Cambiar idioma (EN ↔ ES)
- `Esc/Tab`: Volver a Chat

## Estado de Compilación

✅ **Compilación exitosa**
- 0 errores
- 69 warnings (deprecación de PlanningOrchestrator, código no usado)
- Binario generado: `target/release/neuro` (45MB)

## Próximos Pasos

1. **Testing**: Probar el TUI con RouterOrchestrator en escenarios reales
2. **Performance**: Medir tiempos de respuesta con clasificación rápida
3. **UX**: Ajustar mensajes de progreso y feedback visual
4. **Cleanup**: Remover PlanningOrchestrator cuando RouterOrchestrator esté completamente validado

## Notas Técnicas

### Diferencias de Arquitectura

| Aspecto | PlanningOrchestrator | RouterOrchestrator |
|---------|---------------------|-------------------|
| Clasificación | Modelo pesado | Modelo ligero (qwen3:0.6b) |
| Inicialización RAPTOR | En background por TUI | En constructor |
| Progreso de tareas | Detallado con `TaskProgressInfo` | Integrado en respuesta |
| Fallback | 4 capas | 3 rutas (DirectResponse, ToolExecution, FullPipeline) |
| Re-clasificación | No | Automática en caso de fallo |

### Ventajas del RouterOrchestrator

1. **Velocidad**: Clasificación <1s vs varios segundos
2. **Simplicidad**: Menos código, lógica más clara
3. **Optimización**: Diseñado para modelos pequeños
4. **RAPTOR**: Indexación rápida + background completo
5. **Mantenibilidad**: Menos capas, menos complejidad

## Referencias

- [src/ui/modern_app.rs](src/ui/modern_app.rs) - Implementación principal del TUI
- [src/agent/router_orchestrator.rs](src/agent/router_orchestrator.rs) - Lógica del router
- [src/main.rs](src/main.rs) - Punto de entrada y selección de orquestrador
- [config.example.json](config.example.json) - Configuración de ejemplo
