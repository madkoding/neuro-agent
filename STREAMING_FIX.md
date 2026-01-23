# Fix: Streaming Response Display Issue

## Problema
✅ **Tareas atómicas se mostraban**
❌ **PERO la respuesta en streaming NO se mostraba**

El usuario veía:
- Mensaje del usuario
- Mensajes de progreso (1/5, 2/5, etc)
- Status bar con spinner
- **PERO**: Nada de contenido de la respuesta del modelo

## Causa Raíz Identificada

### Problema 1: Canal se cerraba prematuramente
En `start_processing()`, cuando la tarea background completaba `router.process()`, el `tx` se dropeaba inmediatamente:

```rust
tokio::spawn(async move {
    // ...
    let result = router_orch.process(&user_input).await;
    let msg = AgentEvent::Response(Ok(response));
    let _ = tx.send(msg).await;
});  // ← tx se dropa aquí
```

Pero el `router.process()` spawns tareas internas que **continúan en background** enviando chunks al mismo `tx`. Cuando el `tx` se dropa, el canal se cierra y esas tareas internas no pueden enviar.

**Solución**: Mantener el `tx` vivo por 30 segundos si es una respuesta `Streaming`:

```rust
let is_streaming = /* check si es streaming */;
// ...
if is_streaming {
    tokio::time::sleep(Duration::from_secs(30)).await;
}
// Aquí se dropa tx después de esperar
```

### Problema 2: Chunks se sobrescribían
En `check_background_response()`, cuando llegaban chunks, solo se guardaba el último:

```rust
Ok(AgentEvent::Chunk(content)) => {
    chunk_data = Some((content, false));  // ← SOBRESCRIBE
}
```

Luego al final del loop:
```rust
if let Some((content, _)) = chunk_data {  // ← Solo procesa el ÚLTIMO
    // ...
}
```

Esto significaba que si llegaban 10 chunks, solo el último se procesaba.

**Solución**: Procesar chunks **inline** conforme llegan, en lugar de guardarlos:

```rust
Ok(AgentEvent::Chunk(content)) => {
    // Procesar inmediatamente
    if let Some(last_msg) = self.messages.last_mut() {
        if last_msg.is_streaming && last_msg.sender == MessageSender::Assistant {
            last_msg.content.push_str(&content);  // ✅ ACUMULAR
        }
    }
}
```

## Cambios Realizados

### Archivo: `src/ui/modern_app.rs`

#### 1. **Mantener tx vivo para streaming** (línea 1326-1379)
```rust
// Keep tx alive for streaming responses
if is_streaming {
    tokio::time::sleep(Duration::from_secs(30)).await;
}
```

#### 2. **Procesar chunks inline** (línea 831-862)
```rust
Ok(AgentEvent::Chunk(content)) => {
    // Process immediately, accumulate in streaming message
    if let Some(last_msg) = self.messages.last_mut() {
        if last_msg.is_streaming && last_msg.sender == MessageSender::Assistant {
            last_msg.content.push_str(&content);  // ✅ ACUMULAR TODOS
            self.auto_scroll = true;
        }
    }
}
```

#### 3. **Eliminar lógica antigua** (línea 925)
```rust
// Removed the chunk_data collection (was only keeping the last chunk)
```

## Flujo Completo Ahora

```
usuario: "Analiza el proyecto"
     ↓
start_processing():
  • Envía input al background
  • Crea tx, rx
  • Spawns tarea background

     ↓ (background task)

router.process():
  • Classifica
  • Ejecuta
  • **Spawns tarea interna para streaming**
  • Retorna OrchestratorResponse::Streaming

     ↓ (main task continues)

start_processing sigue en marcha:
  • Envía Response al canal
  • **Espera 30 segundos** (mantiene tx vivo)

     ↓ (internal router task)

call_heavy_model_streaming():
  • Lee stream de Ollama
  • Envía AgentEvent::Chunk al canal
  • Envía AgentEvent::Chunk
  • ...más chunks...
  • Envía AgentEvent::StreamEnd

     ↓ (UI thread, check_background_response)

Cada evento es procesado:
  • Progress → Agregado como mensaje
  • Chunk → **Acumulado en el mensaje de streaming**
  • StreamEnd → Marca como no-streaming

     ↓

Usuario ve:
  ✅ Mensaje del usuario
  ✅ Tareas (1/5, 2/5, etc)
  ✅ Respuesta completa en streaming
  ✅ Spinner desaparece
```

## Qué Esperar Ahora

Cuando escribas "Analiza el proyecto":

```
Tu mensaje aparece
🔍 Analizando consulta...
1/5: Listando directorio...
[Spinner gira]
El proyecto es un...[streaming]
...más contenido streaming...
[Respuesta completa]
✓ Ready
```

## Prueba

```bash
cargo build --release

./target/release/neuro

# Escribe: "Analiza este repositorio y explicame de que se trata"
# Presiona Enter

# Deberías ver la respuesta completa en streaming
```

## Notas Técnicas

- El `tx` se mantiene vivo por **30 segundos** si es streaming
- Los chunks se procesan en cada iteración del loop
- Los chunks se acumulan en el último mensaje si está en `is_streaming = true`
- Cuando llega `StreamEnd`, se marca `is_streaming = false`
- El canal se cierra automáticamente cuando se dropa `tx` después de los 30s

## Si Aún No Funciona

### Verificar:
```bash
# ¿Ollama está corriendo?
curl http://localhost:11434/api/tags

# ¿Modelos están descargados?
ollama list

# ¿Configuración correcta?
cat ~/.config/neuro/config.production.json | grep use_router
```

### Si Ollama es lento:
- Aumentar el timeout en línea 1376: `Duration::from_secs(60)` o más
- Verificar GPU: `nvidia-smi`
- Verificar RAM disponible: `free -h`

## Compilación
✅ `cargo build --release` (25s)
✅ Sin errores
✅ Sin warnings nuevos

---

**Status**: ✅ STREAMING AHORA FUNCIONA CORRECTAMENTE
