# Diagnóstico: Ollama Muy Lento

## Síntomas
- El programa espera 30+ segundos por respuesta
- Timeout después de 45 segundos
- Spinner gira sin progreso

## Causas Posibles

### 1. **Ollama no está usando GPU**
Si Ollama está infiriendo en CPU en lugar de GPU, será 10-50x más lento.

**Verificar:**
```bash
# Ver logs de Ollama
docker logs ollama 2>&1 | grep -i "gpu\|cuda\|layer"

# Debe mostrar algo como:
# "compute: [NVIDIA GPU]"
# "loaded weights on gpu"
```

**Solucionar:**
```bash
# Revisar que la GPU esté disponible
nvidia-smi

# Debe mostrar tu RTX 3060 como disponible

# Reiniciar Ollama asegurando acceso a GPU
docker stop ollama
docker run -d --gpus all -p 11434:11434 ollama/ollama
```

### 2. **Modelo No Está en Memoria (Se Descarga en Cada Query)**
Si el modelo está en disco y no en memoria, cada query lo descarga (muy lento).

**Verificar:**
```bash
# Ver qué modelos están cargados
docker exec ollama ollama list

# Debe mostrar qwen3:0.6b y qwen3:8b
# Si falta alguno, está siendo descargado cada vez
```

**Solucionar:**
```bash
# Precargar los modelos (hazlo UNA sola vez)
docker exec ollama ollama pull qwen3:0.6b
docker exec ollama ollama pull qwen3:8b

# Ahora están en memoria y son rápidos
```

### 3. **Modelo qwen3:8b es Muy Grande para tu Hardware**
Qwen3:8b ocupa ~8GB de VRAM. Si tu GPU solo tiene 12GB:
- Casi no hay VRAM libre
- Swap lento entre GPU↔RAM
- CPU se usa mucho

**Verificar durante una query:**
```bash
# En otra terminal mientras Neuro está procesando:
watch -n 1 nvidia-smi

# Ver:
# - VRAM disponible
# - % utilización GPU
# - Procesos de ollama
```

**Solucionar:**
```bash
# Opción A: Usar modelo más pequeño
# Edita ~/.config/neuro/config.production.json:
# Cambiar "heavy_model": "qwen3:8b" por "qwen3:7b" o "mistral:7b"

# Opción B: Reducir tamaño de contexto
# Editar en src/agent/orchestrator.rs:
# Reducir MAX_CONTEXT_SIZE de 8192 a 4096

# Opción C: Esperar más (upgrade de GPU)
# RTX 3060 (12GB) es el mínimo para modelos 8B
```

### 4. **Ollama está en Contenedor pero Sin GPU**
El contenedor Docker puede no tener acceso a GPU.

**Verificar:**
```bash
docker inspect ollama | grep -A 20 "Runtime"

# Debe mostrar:
# "Runtime": "nvidia"
```

**Solucionar:**
```bash
# Si usa docker-compose, verificar:
cat docker-compose.yml | grep -A 5 "ollama:"

# Debe tener:
# runtime: nvidia
# environment:
#   - NVIDIA_VISIBLE_DEVICES=all
```

### 5. **RAM del Sistema Está Llena**
Si el sistema no tiene RAM libre, Ollama será lentísimo (usando swap en disco).

**Verificar:**
```bash
free -h

# Debe mostrar al menos 4-5GB libres
```

**Solucionar:**
```bash
# Ver qué está usando memoria
ps aux --sort=-%mem | head -10

# Detener servicios no usados
docker-compose down  # Bajar todos los servicios
docker system prune -a  # Limpiar espacio

# Luego reiniciar solo lo necesario:
docker run -d --gpus all -p 11434:11434 ollama/ollama
```

## Guía de Diagnóstico Paso a Paso

### Paso 1: Verifica Conectividad Básica
```bash
# ¿Ollama responde?
curl http://localhost:11434/api/tags

# Debe devolver JSON con lista de modelos
# Si falla: Ollama no está corriendo o puerto 11434 no es accesible
```

### Paso 2: Prueba Model Simple
```bash
# Prueba el modelo rápido (0.6B) que debería ser instantáneo
curl http://localhost:11434/api/generate \
  -d '{
    "model": "qwen3:0.6b",
    "prompt": "Hola",
    "stream": false
  }' \
  -H "Content-Type: application/json"

# Si tarda >5 segundos: problema de GPU/VRAM
# Si falla: modelo no descargado
```

### Paso 3: Prueba Model Pesado
```bash
# Prueba el modelo pesado (8B)
curl http://localhost:11434/api/generate \
  -d '{
    "model": "qwen3:8b",
    "prompt": "Escribe un poema",
    "stream": false
  }' \
  -H "Content-Type: application/json"

# Medir tiempo exacto:
time curl http://localhost:11434/api/generate \
  -d '{"model":"qwen3:8b","prompt":"test","stream":false}' \
  -H "Content-Type: application/json" > /dev/null

# Si tarda >30 segundos: modelo en CPU o sin GPU
```

### Paso 4: Verifica GPU Durante Query
```bash
# Terminal 1: Inicia query larga
curl http://localhost:11434/api/generate \
  -d '{
    "model": "qwen3:8b",
    "prompt": "Escribe un ensayo sobre inteligencia artificial en 1000 palabras",
    "stream": false
  }' \
  -H "Content-Type: application/json"

# Terminal 2: Monitorea GPU (cada 1 segundo)
watch -n 1 nvidia-smi

# Ver:
# ✅ Bueno: "ollama" usa 8-10GB VRAM, 95-100% GPU util
# ❌ Malo: "ollama" usa poco VRAM, GPU util < 50%
# ❌ Muy malo: No aparece "ollama" en lista
```

## Soluciones Rápidas (Por Urgencia)

### Urgencia 1: Quiero Resultados Ahora
```bash
# Cambiar a modelo más rápido
nano ~/.config/neuro/config.production.json

# Cambiar:
# "heavy_model": "qwen3:8b"
# a:
# "heavy_model": "qwen3:0.6b"  (más rápido pero menos inteligente)
# o:
# "heavy_model": "mistral:7b"  (buen balance)

# Guardar (Ctrl+O, Enter, Ctrl+X)
```

### Urgencia 2: Mejorar Performance Gradualmente
1. Precargar modelos (ver arriba)
2. Verificar que GPU esté activa (ver nvidia-smi)
3. Reducir contexto si es necesario

### Urgencia 3: Solución Completa
```bash
# Nuclear option: Reiniciar todo
docker-compose down
docker system prune -a

# Limpiar config
rm ~/.config/neuro/config.production.json

# Reconstruir todo from scratch
docker-compose up -d ollama surrealdb
docker exec ollama ollama pull qwen3:0.6b
docker exec ollama ollama pull qwen3:8b

# Verificar
curl http://localhost:11434/api/tags
```

## Monitoreo Continuo

Para durante desarrollo, monitorea estas métricas:

```bash
# Terminal A: Neuro
./target/release/neuro

# Terminal B: GPU Monitor
watch -n 1 'nvidia-smi | grep -E "NVIDIA|Processes|ollama"'

# Terminal C: Network/Logs
docker logs -f ollama | grep -i "loading\|loaded\|compute"
```

## Conclusión

**Si Ollama tarda >10 segundos en responder:**
1. No está usando GPU
2. O la GPU es insuficiente para el modelo
3. O el modelo no está precargado

**La solución es garantizar:**
- ✅ GPU disponible y activa
- ✅ Modelo precargado en memoria
- ✅ RAM del sistema con espacio libre

---

**Cambios en Neuro v1.1:**
- ✅ Timeout reducido a 45s (no esperar innecesariamente)
- ✅ Mensaje "Procesando... (Ctrl+C para cancelar)" (claridad)
- ✅ Autoscroll funcional (mejor UX)
- ✅ Error message con hints de diagnóstico

Ejecuta `./target/release/neuro` y si espera >45s, vuelve a este documento.
