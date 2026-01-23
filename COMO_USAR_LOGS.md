# Cómo Usar los Logs - Guía Simple

## Situación Actual

El programa ahora **guarda TODOS los detalles en un archivo de log** sin ensuciar la pantalla.

## Dónde Está el Archivo

```
~/.local/share/neuro/neuro.log
```

Se crea automáticamente la primera vez que ejecutas el programa.

## Cómo Ejecutar

### 1️⃣ Compilar (si no lo hiciste)
```bash
cd /home/madkoding/proyectos/neuro-agent
cargo build --release
```

### 2️⃣ Ejecutar Neuro (Pantalla Limpia)
```bash
./target/release/neuro
```

Verás la interfaz normal sin logs contaminando.

### 3️⃣ Monitorear Logs (Otra Terminal)
```bash
# El más simple:
tail -f ~/.local/share/neuro/neuro.log

# O con el script (con colores):
./monitor_logs.sh follow
```

### 4️⃣ Prueba del Freeze
En la app:
```
Analiza este repositorio y explicame de que se trata
```

En los logs verás algo así:
```
[tiempo] ... ⏱️ [TIMING] Processing at 10s
[tiempo] ... ⏱️ [TIMING] Processing at 20s
[tiempo] ... ⏱️ [TIMING] Processing at 30s
[tiempo] ... ⏱️ [TIMING] Processing at 40s
[tiempo] ... ⏱️ [TIMING] Processing at 50s
... continúa o se detiene aquí si hay freeze
```

## Verificar Si Hay Freeze

### Comando Mágico
```bash
grep "Processing at" ~/.local/share/neuro/neuro.log | tail -10
```

Deberías ver logs cada 10 segundos.

Si ves:
```
Processing at 10s
Processing at 20s
Processing at 30s
Processing at 40s
```

Y luego nada... **allí está el freeze.**

## Script de Ayuda

Hay un script que hace todo más fácil:

```bash
# Ver todo en tiempo real (coloreado)
./monitor_logs.sh follow

# Ver solo timing logs (para debug del freeze)
./monitor_logs.sh timing

# Ver solo background task
./monitor_logs.sh task

# Ver solo event loop
./monitor_logs.sh loop

# Ver solo errores
./monitor_logs.sh errors
```

## Formatos Que Verás en los Logs

### 🔧 Background Task
```
🔧 [BG-TASK] Starting background task
🔧 [BG-TASK] Calling router_orch.process()
🔧 [BG-TASK] Background task complete
```

Indica que el background está corriendo.

### 🔄 Event Loop
```
🔄 [EVENT-LOOP] Iteration 100, processing_elapsed: 8s
🔄 [EVENT-LOOP] Iteration 200, processing_elapsed: 16s
```

Indica que la UI está responsiva.

### ⏱️ Timing
```
⏱️ [TIMING] Processing at 10s
⏱️ [TIMING] Processing at 20s
```

Indica que los eventos llegan cada 10 segundos.

## Si Hay Freeze

### Busca en los logs:
```bash
grep "Processing at" ~/.local/share/neuro/neuro.log | tail -5
```

**Si ves:**
- `10s, 20s, 30s, 40s, 50s...` = **OK ✅**
- `10s, 20s, 30s, 40s` = **FREEZE aquí ⚠️**

## Limpiar Logs Viejos

Si el archivo crece mucho:
```bash
# Borrarlo todo
rm ~/.local/share/neuro/neuro.log

# O guardar solo los últimos
tail -1000 ~/.local/share/neuro/neuro.log > /tmp/backup.log
rm ~/.local/share/neuro/neuro.log
cp /tmp/backup.log ~/.local/share/neuro/neuro.log
```

## Comandos Útiles

| Comando | Qué Hace |
|---------|----------|
| `tail -f ~/.local/share/neuro/neuro.log` | Ver logs en tiempo real |
| `tail -50 ~/.local/share/neuro/neuro.log` | Ver últimas 50 líneas |
| `grep ERROR ~/.local/share/neuro/neuro.log` | Ver solo errores |
| `grep TIMING ~/.local/share/neuro/neuro.log` | Ver timing logs |
| `./monitor_logs.sh follow` | Ver con colores |
| `wc -l ~/.local/share/neuro/neuro.log` | Contar líneas |

## Resumen

```
┌─────────────────────────────────────────┐
│  Terminal 1: Ejecutar                   │
│  $ ./target/release/neuro               │
│  (Pantalla limpia, sin logs)            │
├─────────────────────────────────────────┤
│  Terminal 2: Monitorear                 │
│  $ tail -f ~/.local/share/neuro/neuro.log
│  (Ver logs en tiempo real)              │
├─────────────────────────────────────────┤
│  Terminal 3: Analizar                   │
│  $ grep "Processing at" neuro.log       │
│  (Buscar dónde está el freeze)          │
└─────────────────────────────────────────┘
```

## ¡Listo!

Los logs están **automáticamente capturados** en:
```
~/.local/share/neuro/neuro.log
```

Solo ejecuta el programa y revisa los logs cuando lo necesites. 🎉
