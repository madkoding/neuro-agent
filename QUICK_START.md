# Quick Start Guide - Continuing Implementation

## 🚀 Estado Actual

**Sprint 1**: 60% Completo (4/6 features)
- ✅ Classification cache con fuzzy matching
- ✅ Real-time progress tracking  
- ✅ RouterOrchestrator integration
- ✅ Zero warnings
- 🚧 Parallel tool execution (NEXT)
- 🚧 Streaming responses

---

## 💻 Comandos Rápidos

### Compilar y Verificar
```bash
# Compilación rápida
cargo build

# Compilación release (optimizada)
cargo build --release

# Verificar sin compilar binario
cargo check

# Verificar sin warnings
cargo check 2>&1 | grep -E "^warning:" | grep -v "nom v1.2.4"
# Expected: No output (0 warnings)
```

### Ejecutar Tests
```bash
# Todos los tests rápidos (sin Ollama)
cargo test --lib

# Tests del cache específicamente
cargo test classification_cache --lib

# Tests funcionales (requieren Ollama)
cargo test --test functional_tests -- --ignored

# Ver output detallado
cargo test -- --nocapture
```

### Ejecutar Neuro
```bash
# TUI con RouterOrchestrator
./target/release/neuro

# Con debug logs
RUST_LOG=debug ./target/release/neuro --verbose

# Con router debug
./target/release/neuro --debug

# Simple mode (sin TUI)
./target/release/neuro --simple
```

---

## 🎯 Próximas Tareas (Priorizadas)

### 1. Parallel Tool Execution (2 días) 🔥 HIGH
**Objetivo**: Ejecutar herramientas independientes en paralelo para 2-3x speedup

**Archivos a modificar**:
- `src/agent/router_orchestrator.rs` - Método `process()`
- `src/agent/orchestrator.rs` - Agregar `execute_tools_parallel()`

**Implementación sugerida**:
```rust
// En router_orchestrator.rs - process() method

// Detectar si hay múltiples herramientas independientes
let tools_to_execute = vec![
    ("analyzer", args_analyzer),
    ("linter", args_linter),
    ("formatter", args_formatter),
];

// Ejecutar en paralelo
let handles: Vec<_> = tools_to_execute.iter()
    .map(|(tool_name, args)| {
        let orchestrator = self.orchestrator.clone();
        let tool_name = tool_name.to_string();
        let args = args.clone();
        
        tokio::spawn(async move {
            let mut orch = orchestrator.lock().await;
            orch.execute_tool(&tool_name, &args).await
        })
    })
    .collect();

// Esperar resultados
let results = futures::join_all(handles).await;

// Combinar resultados
let combined_result = results.into_iter()
    .filter_map(|r| r.ok())
    .collect::<Vec<_>>()
    .join("\n\n---\n\n");
```

**Dependencias a agregar** (si no están):
```toml
# En Cargo.toml
futures = "0.3"  # Ya existe
```

**Tests a crear**:
```rust
#[tokio::test]
async fn test_parallel_tool_execution() {
    // Ejecutar 3 tools independientes
    let start = Instant::now();
    let result = router.process_with_tools(vec!["analyzer", "linter", "formatter"]).await;
    let duration = start.elapsed();
    
    // Verificar que tardó menos que secuencial
    assert!(duration < Duration::from_secs(2)); // vs 5s secuencial
}
```

---

### 2. Streaming Responses (2 días) 🔥 HIGH
**Objetivo**: Display responses token-by-token como Claude Code

**Archivos a modificar**:
- `src/agent/orchestrator.rs` - Agregar streaming support
- `src/ui/modern_app.rs` - Handle BackgroundMessage::Chunk
- `src/agent/provider.rs` - Streaming from Ollama

**Implementación sugerida**:

**Paso 1**: Agregar BackgroundMessage::Chunk
```rust
// En modern_app.rs
enum BackgroundMessage {
    Response(Result<OrchestratorResponse, String>),
    Chunk(String), // NEW: streaming chunk
    Thinking(String),
    TaskProgress(TaskProgressInfo),
}
```

**Paso 2**: Modificar orchestrator para streaming
```rust
// En orchestrator.rs
pub async fn process_streaming(
    &mut self,
    prompt: &str,
    chunk_tx: Sender<String>,
) -> Result<String> {
    let mut full_response = String::new();
    
    // Ollama streaming endpoint
    let response = self.provider.generate_streaming(prompt).await?;
    
    // Stream chunks
    while let Some(chunk) = response.next().await {
        let text = chunk?;
        full_response.push_str(&text);
        let _ = chunk_tx.send(text).await;
    }
    
    Ok(full_response)
}
```

**Paso 3**: Update TUI para mostrar chunks
```rust
// En modern_app.rs - check_background_response()
BackgroundMessage::Chunk(text) => {
    // Append to current message
    if let Some(last_msg) = self.messages.last_mut() {
        if last_msg.is_streaming {
            last_msg.content.push_str(&text);
        }
    }
}
```

**Dependencias a agregar**:
```toml
# En Cargo.toml (si no existe)
futures = "0.3"  # Ya existe para streams
```

---

### 3. Auto-include Related Files (3 días) 🌟 MEDIUM
**Objetivo**: Incluir imports y tests automáticamente cuando mencionas un archivo

**Archivos a crear**:
- `src/context/related_files.rs` - Lógica de detección

**Implementación sugerida**:
```rust
// En src/context/related_files.rs

pub struct RelatedFileFinder {
    working_dir: PathBuf,
}

impl RelatedFileFinder {
    /// Find files related to target_file
    pub async fn find_related(&self, target_file: &Path) -> Result<Vec<PathBuf>> {
        let mut related = Vec::new();
        
        // 1. Find imports
        let imports = self.parse_imports(target_file).await?;
        for import in imports {
            if let Some(path) = self.resolve_import(&import).await? {
                related.push(path);
            }
        }
        
        // 2. Find tests
        let test_path = self.find_test_file(target_file).await?;
        if let Some(test) = test_path {
            related.push(test);
        }
        
        // 3. Find docs
        let doc_path = self.find_doc_file(target_file).await?;
        if let Some(doc) = doc_path {
            related.push(doc);
        }
        
        Ok(related)
    }
    
    async fn parse_imports(&self, file: &Path) -> Result<Vec<String>> {
        let content = tokio::fs::read_to_string(file).await?;
        
        // Regex para Rust: use crate::module::*;
        let re = Regex::new(r"use\s+(?:crate|super)::([\w:]+)")?;
        
        let imports = re.captures_iter(&content)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();
        
        Ok(imports)
    }
    
    async fn find_test_file(&self, target: &Path) -> Result<Option<PathBuf>> {
        // Conventions:
        // src/module.rs -> tests/module_tests.rs
        // src/module/mod.rs -> tests/module_tests.rs
        
        let file_stem = target.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        
        let test_candidates = vec![
            self.working_dir.join("tests").join(format!("{}_tests.rs", file_stem)),
            self.working_dir.join("tests").join(format!("{}_test.rs", file_stem)),
            target.parent()
                .map(|p| p.join("tests.rs"))
                .unwrap_or_default(),
        ];
        
        for candidate in test_candidates {
            if candidate.exists() {
                return Ok(Some(candidate));
            }
        }
        
        Ok(None)
    }
}
```

**Integración en RouterOrchestrator**:
```rust
// En router_orchestrator.rs - process()

// Si query menciona un archivo
if let Some(file_path) = extract_file_path(user_query) {
    let finder = RelatedFileFinder::new(&self.config.working_dir);
    let related = finder.find_related(&file_path).await?;
    
    // Agregar al contexto
    for related_file in related {
        let content = tokio::fs::read_to_string(&related_file).await?;
        enriched_query.push_str(&format!(
            "\n\n# Related file: {}\n{}", 
            related_file.display(), 
            content
        ));
    }
}
```

---

## 🔍 Debugging Tips

### Ver decisiones del router
```bash
# Activar debug mode
./target/release/neuro --debug

# Logs mostrarán:
# [ROUTER] DirectResponse mode (confidence: 0.92)
# [ROUTER] ToolExecution mode: Ask (confidence: 0.85)
```

### Ver estadísticas del cache
```rust
// Agregar endpoint temporal en main.rs
let stats = router.cache_stats().await;
println!("Cache hits: {} ({:.1}%)", stats.hits, stats.hit_rate());
```

### Profiling con tokio-console
```bash
# Instalar
cargo install tokio-console

# Compilar con tracing
RUSTFLAGS="--cfg tokio_unstable" cargo build --features tokio-console

# Ejecutar console
tokio-console
```

---

## 📊 Métricas a Monitorear

### Cache Performance
```rust
// Agregar logs cada 100 queries
if query_count % 100 == 0 {
    let stats = self.classification_cache.lock().await.stats();
    log_info!("Cache: {} hits / {} misses ({:.1}%)", 
        stats.hits, stats.misses, stats.hit_rate());
}
```

### Progress Timing
```rust
// Log timing per stage
self.send_progress(
    ProgressStage::Complete,
    format!("✓ Completado en {}ms", elapsed_ms),
    elapsed_ms,
);
```

---

## 🐛 Common Issues & Fixes

### Issue 1: Cache not working
**Symptom**: Queries always take 2-4s
**Debug**:
```rust
let stats = cache.stats();
println!("Hits: {}, Misses: {}", stats.hits, stats.misses);
```
**Fix**: Check normalization is working, verify threshold

### Issue 2: Progress not showing
**Symptom**: UI shows generic spinner
**Debug**: Check channel is set: `router.set_progress_channel(tx)`
**Fix**: Ensure `progress_tx` is Some in RouterOrchestrator

### Issue 3: Compilation warnings
**Command**: `cargo clippy --all-targets`
**Fix**: Remove unused imports, fix suggested improvements

---

## 📚 Resources

### Rust Async
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Async Book](https://rust-lang.github.io/async-book/)

### Testing
- [Cargo Test Docs](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [Tokio Testing](https://tokio.rs/tokio/topics/testing)

### TUI
- [Ratatui Book](https://ratatui.rs/)
- [Ratatui Examples](https://github.com/ratatui-org/ratatui/tree/main/examples)

---

## 🎯 Daily Goals

### Día 1: Parallel Tool Execution
- [ ] Morning: Design parallel execution flow
- [ ] Afternoon: Implement tokio::spawn approach
- [ ] Evening: Add tests, verify 2-3x speedup

### Día 2: Streaming Responses  
- [ ] Morning: Add BackgroundMessage::Chunk
- [ ] Afternoon: Modify orchestrator for streaming
- [ ] Evening: Update TUI to display chunks

### Día 3: Polish & Testing
- [ ] Morning: Run full test suite
- [ ] Afternoon: Performance benchmarks
- [ ] Evening: Update documentation

---

## ✅ Pre-commit Checklist

Antes de cada commit, verificar:

```bash
# 1. No warnings
cargo check 2>&1 | grep "warning:" | grep -v "nom" | wc -l
# Expected: 0

# 2. Tests pass
cargo test --lib

# 3. Clippy happy
cargo clippy --all-targets -- -D warnings

# 4. Formatted
cargo fmt --check

# 5. Docs updated
git diff --name-only | grep -E "\.md$"
```

---

## 🚀 Launch Commands

### Development
```bash
# Watch mode (recompile on save)
cargo watch -x check -x test

# TUI with live reload
cargo watch -x "run -- --debug"
```

### Production
```bash
# Release build
cargo build --release --locked

# Strip symbols (smaller binary)
strip target/release/neuro

# Test production binary
./target/release/neuro --help
```

---

## 🎉 Milestones

- [x] **Sprint 1 - 60%** (Current)
- [ ] **Sprint 1 - 100%** (Parallel + Streaming)
- [ ] **Sprint 2 - Start** (Context intelligence)
- [ ] **Alpha Release** (Week 1)
- [ ] **Beta Release** (Week 3)
- [ ] **v1.0 Release** (Week 7)

---

**Current Status**: ✅ Sprint 1 - 60% Complete  
**Next Task**: Parallel tool execution (2 días ETA)  
**Blocking Issues**: None  

¡Vamos bien! 🚀

