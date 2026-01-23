# Guía de Logging Detallado

## Visión General

El sistema de logging de Neuro ahora captura **toda la actividad detallada en un archivo .log** sin ensuciar la pantalla de la UI.

### Características

✅ **Archivo de log automático** - Se crea y actualiza automáticamente
✅ **Sin contaminación visual** - La pantalla permanece limpia
✅ **Información detallada** - Timestamp, thread, nivel, mensaje
✅ **Always-on** - Captura todo sin necesidad de RUST_LOG
✅ **Fácil de revisar** - Un archivo centralizado con todo

---

## Ubicación del Archivo de Log

### En Linux/macOS:
```bash
~/.local/share/neuro/neuro.log
```

O puede verlo con:
```bash
cat ~/.local/share/neuro/neuro.log
```

### En Windows:
```
%APPDATA%\neuro\neuro.log
```

### En Windows (Git Bash):
```bash
cat $APPDATA/neuro/neuro.log
```

---

## Cómo Monitorear el Log en Tiempo Real

### Opción 1: En la misma terminal (mientras corre Neuro)

**Terminal 1 - Ejecutar Neuro:**
```bash
./target/release/neuro
```

**Terminal 2 - Ver logs en tiempo real:**
```bash
# En Linux/macOS:
tail -f ~/.local/share/neuro/neuro.log

# O con grep para filtrar por nivel:
tail -f ~/.local/share/neuro/neuro.log | grep "DEBUG\|TIMING\|EVENT"
```

### Opción 2: Ver el archivo después de terminar
```bash
cat ~/.local/share/neuro/neuro.log
```

### Opción 3: Buscar patrones específicos
```bash
# Ver solo errores
grep ERROR ~/.local/share/neuro/neuro.log

# Ver solo eventos de freeze diagnosis
grep "BG-TASK\|EVENT-LOOP\|TIMING" ~/.local/share/neuro/neuro.log

# Ver últimas 100 líneas
tail -100 ~/.local/share/neuro/neuro.log

# Ver logs de los últimos 5 minutos
tail -f ~/.local/share/neuro/neuro.log | grep "$(date +'%H:%M')"
```

---

## Formato del Log

### Ejemplo de entrada de log:

```
[2026-01-16 14:30:45.123] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: 🔧 [BG-TASK] Starting background task for query: 'test'
```

**Componentes:**
- `2026-01-16 14:30:45.123` - Timestamp preciso (milisegundos)
- `DEBUG` - Nivel de log
- `tokio-runtime` - Nombre del thread
- `ThreadId(5)` - ID único del thread
- `🔧 [BG-TASK] ...` - Mensaje detallado

### Niveles de Log

| Nivel | Significado | Ejemplo |
|-------|-----------|---------|
| `ERROR` | Error crítico | "Router orchestrator error: timeout" |
| `WARN` | Advertencia | "Low confidence in classification" |
| `INFO` | Información general | "Processing request" |
| `DEBUG` | Información de debugging | "🔧 [BG-TASK] Starting task" |
| `TRACE` | Traza muy detallada | "Lock acquired at Xms" |
| `EVENT` | Eventos del sistema | "[Chunk] received 256 bytes" |
| `TIMING` | Información de tiempo | "Processing took 45000ms" |

---

## Cómo Usar Para Debugging del Freeze

### Paso 1: Ejecutar Neuro Normalmente
```bash
./target/release/neuro
```

### Paso 2: Enviar Query que Cause Freeze
```
Analiza este repositorio y explicame de que se trata
```

### Paso 3: Monitorear Logs en Otra Terminal
```bash
tail -f ~/.local/share/neuro/neuro.log
```

### Paso 4: Buscar Dónde se Detiene

Busca patrones como estos en el archivo:

**Logs esperados cada 10 segundos:**
```
⏱️ [TIMING] Processing at 10s, event:
⏱️ [TIMING] Processing at 20s, event:
⏱️ [TIMING] Processing at 30s, event:
⏱️ [TIMING] Processing at 40s, event:
```

**Si freeze ocurre:**
```
⏱️ [TIMING] Processing at 40s, event: ...
[NO MÁS LOGS DESPUÉS DE ESTO - FREEZE]
```

---

## Análisis de Logs Para Freeze

### Búsqueda Específica

```bash
# Ver todos los logs del background task
grep "BG-TASK" ~/.local/share/neuro/neuro.log

# Ver todos los timing logs
grep "TIMING" ~/.local/share/neuro/neuro.log

# Ver event loop responsiveness
grep "EVENT-LOOP" ~/.local/share/neuro/neuro.log

# Ver en orden cronológico de una sesión
tail -200 ~/.local/share/neuro/neuro.log
```

### Ejemplo: Análisis de Freeze

**Buscar:**
```bash
grep "Processing at" ~/.local/share/neuro/neuro.log | tail -20
```

**Resultado esperado (sin freeze):**
```
[2026-01-16 14:30:50.123] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 10s
[2026-01-16 14:31:00.456] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 20s
[2026-01-16 14:31:10.789] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 30s
[2026-01-16 14:31:20.012] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 40s
[2026-01-16 14:31:30.345] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 50s
```

**Resultado con freeze (logs se detienen):**
```
[2026-01-16 14:30:50.123] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 10s
[2026-01-16 14:31:00.456] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 20s
[2026-01-16 14:31:10.789] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 30s
[2026-01-16 14:31:20.012] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 40s
[SIN MÁS LOGS - PROGRAMA CONGELADO]
```

---

## Limpiar Logs Antiguos

El archivo de log se va acumulando. Para limpiar logs antiguos:

```bash
# Borrar log completamente (cuidado!)
rm ~/.local/share/neuro/neuro.log

# O mantener solo las últimas 1000 líneas
tail -1000 ~/.local/share/neuro/neuro.log > /tmp/neuro_backup.log
> ~/.local/share/neuro/neuro.log
cat /tmp/neuro_backup.log >> ~/.local/share/neuro/neuro.log
```

---

## Tipos de Logs Agregados Para Debugging

### 1. Background Task Logging
```
🔧 [BG-TASK] Starting background task for query: 'xyz'
🔧 [BG-TASK] Acquired orchestrator lock at Xms
🔧 [BG-TASK] Calling router_orch.process() at Xms
🔧 [BG-TASK] router_orch.process() returned after XXXXms
🔧 [BG-TASK] Background task complete at XXXXms
```

### 2. Event Loop Monitoring
```
🔄 [EVENT-LOOP] Iteration 100, processing_elapsed: 8s
🔄 [EVENT-LOOP] Iteration 200, processing_elapsed: 16s
```

### 3. Event Timing
```
⏱️ [TIMING] Processing at 10s, event: Chunk(...)
⏱️ [TIMING] Processing at 20s, event: Progress(...)
⏱️ [TIMING] Processing at 30s, event: Chunk(...)
```

---

## Cómo Reportar Issues Con Logs

Si encuentras un problema, incluye:

1. **El archivo completo de logs:**
   ```bash
   cp ~/.local/share/neuro/neuro.log ~/neuro_logs_date.log
   ```

2. **O las últimas líneas relevantes:**
   ```bash
   tail -100 ~/.local/share/neuro/neuro.log
   ```

3. **Marca de tiempo donde ocurrió el problema:**
   - "Congeló a las 14:32:45"
   - "Timeout a los 120s después de comenzar"

4. **Qué query ejecutaste:**
   - "Analiza este repositorio..."
   - "Explícame este archivo..."

---

## Cambios en el Sistema de Logging

### Antes:
- Logs solo en stderr (si RUST_LOG=debug)
- No hay persistencia automática
- Pantalla ensuciada con logs

### Ahora:
- ✅ Logs siempre se escriben a archivo
- ✅ Información detallada (thread, timestamp, nivel)
- ✅ Pantalla limpia de la UI
- ✅ Captura automática sin configuración
- ✅ Fácil de revisar después

---

## Macros de Logging Disponibles

En el código Rust, puedes usar:

```rust
// Información general
log_info!("Query iniciada: {}", query);

// Debugging
log_debug!("🔧 [SECTION] Detalles del proceso");

// Advertencias
log_warn!("Confidence bajo: {}", confidence);

// Errores
log_error!("Error al procesar: {}", error);

// Traza detallada
log_trace!("Punto específico del código alcanzado");

// Timing
logging::log_timing("Proceso", elapsed_ms);

// Eventos
logging::log_event("Chunk", &format!("Recibidos {} bytes", size));
```

---

## Resumen

| Aspecto | Antes | Ahora |
|--------|-------|-------|
| **Logs** | Pantalla | Archivo .log |
| **Visibilidad** | Contaminada | Limpia |
| **Detalle** | Básico | Muy detallado |
| **Thread Info** | No | Sí |
| **Configuración** | RUST_LOG needed | Automático |
| **Persistencia** | No | Siempre |

**Ubicación del archivo:** `~/.local/share/neuro/neuro.log`

¡Los logs están sempre capturando! Solo revisa el archivo cuando lo necesites.
