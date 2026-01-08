# 🚀 Inicio Rápido - Tests Funcionales

## ⚡ En 3 Pasos

### 1️⃣ Verificar Instalación
```bash
./run_tests.sh check
```

**Output esperado:**
```
✅ Cargo instalado: cargo 1.xx.x
✅ Ollama instalado
✅ Ollama está corriendo
✅ Modelo qwen3:0.6b disponible
✅ Modelo qwen3:8b disponible
```

### 2️⃣ Tests Rápidos (Sin Ollama)
```bash
./run_tests.sh fast
```

Ejecuta:
- ✅ Tests de clasificación (12 tests)
- ✅ Tests de herramientas (13 tests)
- ⏱️ Tiempo: ~5 segundos

### 3️⃣ Tests Completos (Con Ollama)
```bash
./run_tests.sh functional
```

Ejecuta:
- ✅ Chat conversacional
- ✅ Procesamiento de texto
- ✅ Operaciones aritméticas
- ✅ Generación de código
- ✅ Y 7 categorías más...
- ⏱️ Tiempo: ~2-5 minutos

## 🎯 Tests Individuales

### Chat Simple
```bash
./run_tests.sh chat
```

### Aritmética
```bash
./run_tests.sh arithmetic
```

### Generación de Código
```bash
./run_tests.sh code
```

### Integración Completa
```bash
./run_tests.sh integration
```

## 📦 Si No Tienes Ollama

### Instalar Ollama
```bash
# Linux/Mac
curl -fsSL https://ollama.ai/install.sh | sh

# Windows
# Descarga desde https://ollama.ai/download
```

### Iniciar Ollama
```bash
ollama serve
```

### Descargar Modelos
```bash
ollama pull qwen3:0.6b
ollama pull qwen3:8b
```

## 🔧 Troubleshooting Rápido

### "Ollama no está corriendo"
```bash
# Terminal 1: Iniciar Ollama
ollama serve

# Terminal 2: Ejecutar tests
./run_tests.sh functional
```

### "Modelos no encontrados"
```bash
ollama pull qwen3:0.6b
ollama pull qwen3:8b
```

### "Permission denied: ./run_tests.sh"
```bash
chmod +x run_tests.sh
```

### Tests muy lentos
```bash
# Ejecutar en serie (más lento pero más estable)
cargo test --test functional_tests -- --ignored --nocapture --test-threads=1
```

## 📚 Más Información

- **README.md** - Documentación completa
- **EXAMPLES.md** - Ejemplos de código
- **TEST_SUMMARY.md** - Resumen de implementación

## 💡 Tips

1. **Empieza con tests rápidos** (`./run_tests.sh fast`)
2. **Verifica requisitos** antes de tests funcionales
3. **Usa tests individuales** para debugging
4. **Revisa el output** con `--nocapture` para ver detalles

## ✅ Checklist Pre-Tests

- [ ] Rust instalado (`rustc --version`)
- [ ] Cargo instalado (`cargo --version`)
- [ ] Ollama instalado (para tests funcionales)
- [ ] Ollama corriendo (para tests funcionales)
- [ ] Modelos descargados (para tests funcionales)
- [ ] Script ejecutable (`chmod +x run_tests.sh`)

## 🎉 ¡Listo!

Ya puedes ejecutar:
```bash
./run_tests.sh fast      # Tests rápidos
./run_tests.sh check     # Verificar todo
./run_tests.sh functional # Tests completos
```

---

**¿Problemas?** Revisa `README.md` o ejecuta `./run_tests.sh help`
