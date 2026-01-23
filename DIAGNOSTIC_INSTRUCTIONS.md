# Instrucciones de Diagnóstico para el Problema de Streaming

El programa ha sido recompilado con logs de debugging detallados. Sigue estos pasos:

## Paso 1: Asegúrate que Ollama está corriendo

```bash
# En terminal 1
ollama serve
```

## Paso 2: Ejecuta Neuro con los logs visibles

```bash
# En terminal 2, desde /home/madkoding/proyectos/neuro-agent
./target/release/neuro 2>&1
```

**IMPORTANTE**: El `2>&1` redirige los logs de error a la salida estándar para que los veas.

## Paso 3: Escribe un mensaje en Neuro

Escribe algo como:
```
Hola, ¿quién eres?
```

Presiona Enter.

## Paso 4: Observa los logs

Deberías ver algo como:

```
🔍 DEBUG: Received Response event
🔍 DEBUG: Response type: Streaming
🔍 DEBUG: NOT closing channel for streaming response
🔍 DEBUG: Creating empty streaming message for chunks
🔍 DEBUG: Received Chunk: 123 bytes
🔍 DEBUG: Appending to existing streaming message
🔍 DEBUG: Received Chunk: 456 bytes
🔍 DEBUG: Appending to existing streaming message
...
```

## Significado de los Logs

| Log | Significa | Problema si... |
|-----|-----------|----------------|
| `Received Response event` | Se recibió la respuesta del router | NO APARECE: El router nunca retorna |
| `Response type: Streaming` | La respuesta es de tipo streaming | Aparece otro tipo (Text, Error, etc) |
| `NOT closing channel for streaming response` | El canal se mantiene abierto | NO APARECE: El código no reconoce streaming |
| `Creating empty streaming message for chunks` | Se preparó un lugar para los chunks | NO APARECE: No se entra en handle_orchestrator_response |
| `Received Chunk: XXX bytes` | Se recibió un chunk de la respuesta | NO APARECE: Los chunks nunca llegan |
| `Appending to existing streaming message` | El chunk se agregó al mensaje | NO APARECE: El mensaje está cerrado |

## Posibles Problemas y Soluciones

### Escenario 1: Solo veo "Response event" pero NO "Streaming"
```
🔍 DEBUG: Received Response event
🔍 DEBUG: Response type: Text (o Immediate, o Error)
```

**Problema**: El router no está clasificando como RepositoryAnalysis, está usando otro tipo de respuesta.

**Solución**: El router debe estar forzando RepositoryAnalysis. Revisa el log de DEBUG del router (debería aparecer algo como `[ROUTER] RepositoryAnalysis mode`).

### Escenario 2: Veo "Streaming" pero NO "creating empty streaming message"
```
🔍 DEBUG: Received Response event
🔍 DEBUG: Response type: Streaming
🔍 DEBUG: NOT closing channel for streaming response
[NADA MÁS]
```

**Problema**: `handle_orchestrator_response()` no se está llamando o el Streaming no entra en el match.

**Solución**: Revisar si `orch_response` se está procesando correctamente.

### Escenario 3: Veo "creating empty streaming message" pero NO "Received Chunk"
```
🔍 DEBUG: Creating empty streaming message for chunks
[NADA DE CHUNKS]
```

**Problema**: El canal está muerto o los chunks nunca se envían desde el router.

**Solución**:
- El `tx` podría haberse cerrado demasiado pronto
- La tarea interna del router que envía chunks podría estar fallando
- Ollama podría no estar respondiendo

Vuelca el router con `RUST_LOG=debug` para ver qué está pasando internamente.

### Escenario 4: Veo "Received Chunk" pero NO aparece en el chat
```
🔍 DEBUG: Received Chunk: 123 bytes
🔍 DEBUG: Creating NEW streaming message
[PERO el chat no muestra nada]
```

**Problema**: Problema de rendering en la TUI, no de lógica de eventos.

**Solución**: Verificar que `auto_scroll = true` está siendo seteado y que el draw() es llamado.

## Cómo Reportar

Por favor, copia y pega:
1. Los logs que ves (de 🔍 DEBUG en adelante)
2. Qué es lo ÚLTIMO que ves en los logs
3. Si ves un mensaje vacío en el chat o completamente nada

Con eso podré diagnosticar exactamente dónde se rompe el flujo.

## Alternativa: Verbose con RUST_LOG

Si quieres aún más detalle:

```bash
RUST_LOG=debug ./target/release/neuro 2>&1 | grep "DEBUG\|🔍"
```

Esto filtra solo los logs relevantes.
