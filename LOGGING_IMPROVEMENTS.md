# Mejoras en el Sistema de Logging

## Resumen de Cambios

Se ha mejorado completamente el sistema de logging para:
- ✅ Capturar todos los logs en un archivo automáticamente
- ✅ Mantener la pantalla limpia (sin contaminación visual)
- ✅ Agregar detalles técnicos (timestamp, thread, nivel)
- ✅ Facilitar el debugging del congelamiento a 43-44 segundos
- ✅ No requiere configuración especial (RUST_LOG)

---

## Cómo Funciona

### Automático
```bash
./target/release/neuro
```

El programa automáticamente:
1. ✅ Crea el directorio si no existe
2. ✅ Abre el archivo de log
3. ✅ Comienza a registrar todo
4. ✅ Escribe cada log con timestamp, thread, nivel, mensaje

**La pantalla permanece limpia.** Los logs van directamente al archivo.

### Monitoreo
En **otra terminal**, monitorea los logs en tiempo real:

```bash
./monitor_logs.sh follow
```

O manualmente:
```bash
tail -f ~/.local/share/neuro/neuro.log
```

---

## Ubicaciones y Comandos

### Archivo de Log

**Ubicación automática:**
```
~/.local/share/neuro/neuro.log
```

**Ver archivo:**
```bash
cat ~/.local/share/neuro/neuro.log
```

### Script de Monitoreo

**Ubicación:**
```
/home/madkoding/proyectos/neuro-agent/monitor_logs.sh
```

**Usar desde proyecto:**
```bash
./monitor_logs.sh follow    # Ver todo en tiempo real
./monitor_logs.sh timing    # Ver solo timing logs
./monitor_logs.sh task      # Ver solo background task
./monitor_logs.sh loop      # Ver solo event loop
./monitor_logs.sh errors    # Ver errores y warnings
./monitor_logs.sh all       # Ver últimas 50 líneas
```

---

## Ejemplo de Uso Para Debugging del Freeze

### Terminal 1: Ejecutar Neuro
```bash
$ cd /home/madkoding/proyectos/neuro-agent
$ cargo build --release
$ ./target/release/neuro
```

Verás la interfaz normal, limpia, sin logs.

### Terminal 2: Monitorear Logs
```bash
$ tail -f ~/.local/share/neuro/neuro.log | grep "TIMING\|BG-TASK\|EVENT-LOOP"
```

O usar el script:
```bash
$ ./monitor_logs.sh follow
```

### Terminal 3: Observar
En Neuro, escribe:
```
Analiza este repositorio y explicame de que se trata
```

En los logs verás:
```
[2026-01-16 10:30:45.123] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: 🔧 [BG-TASK] Starting background task...
[2026-01-16 10:30:47.456] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 10s
[2026-01-16 10:30:57.789] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 20s
[2026-01-16 10:31:07.012] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 30s
[2026-01-16 10:31:17.345] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 40s
[2026-01-16 10:31:27.678] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: ⏱️ [TIMING] Processing at 50s
[... continúa o se detiene aquí si hay freeze ...]
```

---

## Formato Detallado del Log

### Componentes de Cada Línea

```
[Timestamp] [Nivel] [Thread Info] Nivel: Mensaje
```

**Ejemplo:**
```
[2026-01-16 14:30:45.123] [DEBUG] [Thread: tokio-runtime ID: ThreadId(5)] DEBUG: 🔧 [BG-TASK] Starting background task
```

| Componente | Significado |
|-----------|-----------|
| `2026-01-16 14:30:45.123` | Timestamp con milisegundos |
| `DEBUG` | Nivel de log |
| `tokio-runtime` | Nombre del thread |
| `ThreadId(5)` | ID único del thread |
| `🔧 [BG-TASK] ...` | Mensaje detallado |

### Niveles de Log

| Nivel | Color | Uso |
|-------|-------|-----|
| `ERROR` | 🔴 Rojo | Errores críticos |
| `WARN` | 🟡 Amarillo | Advertencias |
| `INFO` | ⚪ Blanco | Información general |
| `DEBUG` | 🔵 Azul | Debugging |
| `TRACE` | ⚫ Gris | Traza muy detallada |
| `TIMING` | 🟢 Verde | Información de tiempo |
| `EVENT` | 🟣 Morado | Eventos del sistema |

---

## Tipos de Logs Para Debugging

### Background Task Logs
```
🔧 [BG-TASK] Starting background task for query: '...'
🔧 [BG-TASK] Acquired orchestrator lock at 5ms
🔧 [BG-TASK] Calling router_orch.process() at 10ms
🔧 [BG-TASK] router_orch.process() returned after 45000ms (total: 45010ms)
🔧 [BG-TASK] Response received successfully
🔧 [BG-TASK] Background task complete at 45020ms
```

**Si ves:** Indica que el background task está funcionando y cuánto tiempo tarda.

### Event Loop Logs
```
🔄 [EVENT-LOOP] Iteration 100, processing_elapsed: 8s
🔄 [EVENT-LOOP] Iteration 200, processing_elapsed: 16s
```

**Si ves:** El event loop está responsivo y actualiza cada ~8 segundos.
**Si falta:** El event loop está congelado.

### Timing Logs
```
⏱️ [TIMING] Processing at 10s, event: Chunk(...)
⏱️ [TIMING] Processing at 20s, event: Progress(...)
⏱️ [TIMING] Processing at 30s, event: Chunk(...)
```

**Si ves:** Los eventos llegan continuamente cada 10 segundos.
**Si falta:** No hay eventos nuevos (Ollama no responde).

---

## Análisis del Freeze

### Caso 1: No hay Freeze
```
[Logs aparecen regularmente hasta completarse]
...
🔧 [BG-TASK] Background task complete at 50000ms
[Programa vuelve a "Listo"]
```

✅ **Conclusión:** Funciona correctamente.

### Caso 2: Freeze a los 43-44 Segundos
```
...
⏱️ [TIMING] Processing at 40s, event: Chunk(...)
[SIN MÁS LOGS]
[UI congelada]
```

⚠️ **Conclusión:** Los logs se detienen, hay congelamiento.
- Si faltan `EVENT-LOOP` → El event loop está congelado
- Si faltan `TIMING` → No hay eventos nuevos
- Si faltan `BG-TASK` → El background task está colgado

### Caso 3: Timeout a los 120 Segundos
```
...
⏱️ [TIMING] Processing at 110s, event: Chunk(...)
[2026-01-16 14:32:45.xxx] [ERROR] Timeout: El procesamiento tardó más de 120 segundos
[Programa vuelve a "Listo"]
```

✅ **Conclusión:** El timeout wrapper funcionó, previno congelamiento indefinido.

---

## Archivos Modificados

### 1. src/logging.rs
- Agregado import de `thread`
- Mejorada función `log()` con thread info y timestamp más preciso
- Mejorada función `init_logger()` con información detallada
- Agregadas macros `log_trace!`, `log_timing()`, `log_event()`
- Agregada variable `VERBOSE_LOGGING`

### 2. NUEVOS ARCHIVOS
- `LOGGING_GUIDE.md` - Guía completa de logging
- `monitor_logs.sh` - Script para monitorear logs con colores
- `LOGGING_IMPROVEMENTS.md` - Este archivo

### 3. ACTUALIZADOS
- `QUICK_TEST.md` - Instruye usar el archivo de log
- `TESTING_GUIDE_FREEZE_FIX.md` - Instruye monitorear logs

---

## Comparación: Antes vs Después

### Antes
```
❌ Logs en stderr (si RUST_LOG=debug)
❌ Pantalla ensuciada con mensajes de debug
❌ Información limitada (sin thread info)
❌ No hay persistencia
❌ Difícil de revisar después
```

### Después
```
✅ Logs en archivo (~/.local/share/neuro/neuro.log)
✅ Pantalla limpia (solo interfaz)
✅ Información detallada (timestamp, thread, nivel)
✅ Persiste para revisión posterior
✅ Fácil de analizar con grep/tail
✅ Script de monitoreo con colores
✅ Automático (sin RUST_LOG necesario)
```

---

## Comandos Útiles Para Análisis

```bash
# Ver últimas 50 líneas
tail -50 ~/.local/share/neuro/neuro.log

# Seguir en tiempo real
tail -f ~/.local/share/neuro/neuro.log

# Ver solo errores
grep ERROR ~/.local/share/neuro/neuro.log

# Ver secuencia de timing
grep "TIMING" ~/.local/share/neuro/neuro.log | tail -20

# Ver background task completo
grep "BG-TASK" ~/.local/share/neuro/neuro.log

# Ver event loop responsiveness
grep "EVENT-LOOP" ~/.local/share/neuro/neuro.log | tail -10

# Ver logs de una hora específica
grep "14:30" ~/.local/share/neuro/neuro.log

# Contar eventos por tipo
grep "TIMING" ~/.local/share/neuro/neuro.log | wc -l

# Ver logs recientes con contexto
tail -200 ~/.local/share/neuro/neuro.log | head -100
```

---

## Limpiar Logs

```bash
# Ver tamaño actual
ls -lh ~/.local/share/neuro/neuro.log

# Borrar completamente
rm ~/.local/share/neuro/neuro.log

# Mantener solo últimas 1000 líneas
tail -1000 ~/.local/share/neuro/neuro.log > /tmp/neuro_backup.log
> ~/.local/share/neuro/neuro.log
cat /tmp/neuro_backup.log >> ~/.local/share/neuro/neuro.log
```

---

## Resumen

| Aspecto | Valor |
|--------|-------|
| **Archivo de Log** | `~/.local/share/neuro/neuro.log` |
| **Automático** | ✅ Sí |
| **Requiere RUST_LOG** | ❌ No |
| **Pantalla Limpia** | ✅ Sí |
| **Información Detallada** | ✅ Sí (timestamp, thread, nivel) |
| **Monitoreo Real-time** | ✅ `tail -f` o `./monitor_logs.sh` |
| **Facilita Debugging** | ✅ Sí |
| **Script de Ayuda** | ✅ `./monitor_logs.sh` |

---

**¡Los logs están siempre activos! Solo revisa el archivo cuando lo necesites.**

Ahora puedes ejecutar Neuro normalmente y monitorear los logs sin que la pantalla se ensucie.
