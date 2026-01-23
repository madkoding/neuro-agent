# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Neuro** is a high-performance AI programming assistant written in Rust (~41k LOC). It combines:
- **Dual-model orchestration**: Fast model (qwen3:0.6b) for simple tasks, heavy model (qwen3:8b) for complex reasoning
- **RAPTOR-powered RAG**: Hierarchical document indexing and retrieval with semantic search
- **20+ tools**: Code analysis, formatting, git, shell execution, semantic search, refactoring
- **TUI interface**: Interactive terminal UI with ratatui framework
- **Multiple LLM providers**: Ollama (local), OpenAI, Anthropic, Groq

## Quick Start Commands

### Build & Run
```bash
cargo build --release          # Production build
./target/release/neuro         # Launch interactive TUI
cargo run --release            # Same as above
RUST_LOG=debug cargo run       # With debug logging
```

### Testing
```bash
./run_tests.sh                 # Full test suite
./run_tests.sh fast            # Fast tests (no Ollama required)
./run_tests.sh functional      # Full functional tests (requires Ollama)
./run_tests.sh check           # Verify Ollama + model setup
cargo test --lib              # Unit tests only
cargo test --test router_classification_tests  # Router tests
```

**Prerequisites for functional tests**: Ollama running with `qwen3:0.6b` and `qwen3:8b` models
```bash
ollama serve                   # Terminal 1
ollama pull qwen3:0.6b         # Terminal 2
ollama pull qwen3:8b           # Terminal 2
```

### Code Quality
```bash
cargo fmt                      # Format code (required before commits)
cargo clippy --all-targets     # Lint check
cargo check                    # Validate compilation after changes
```

## Architecture Overview

### Core Modules

**Agent Orchestration** (`src/agent/`)
- **RouterOrchestrator** (`router_orchestrator.rs`): RECOMMENDED - Classifies queries before execution with three modes: `Ask` (read-only), `Build` (write ops), `Plan` (JSON plan generation). Routes to: `DirectResponse`, `ToolExecution`, or `FullPipeline`.
- **DualModelOrchestrator** (`orchestrator.rs`): Legacy - Intelligent routing between fast and heavy models based on query complexity
- **PlanningOrchestrator** (`planning_orchestrator.rs`): DEPRECATED - Will be removed in v2.0 (Feb 2026)

Configuration: Set `use_router_orchestrator: true` in config or `NEURO_USE_ROUTER=true` environment variable.

**Tool System** (`src/tools/`)
- 20+ MCP-compatible tools in `ToolRegistry`
- Code tools: `analyzer`, `formatter`, `linter`, `refactor`, `calculator`
- Search tools: `search`, `semantic_search`, `raptor_tool`
- System tools: `filesystem`, `shell`, `git`, `environment`, `test_runner`
- Each tool implements `rig_core::Tool` trait

**RAPTOR System** (`src/raptor/`)
- Recursive abstractive processing for hierarchical indexing
- `quick_index_sync()`: Fast in-memory chunking without embeddings
- `has_full_index()`: Check if embeddings computed
- Skips directories: `target`, `node_modules`, `.git`, `dist`, `.venv`, `.cache`
- Indexed via `!reindex` command in TUI

**UI Layer** (`src/ui/`)
- `modern_app.rs`: Main TUI using ratatui
- Three screens: Chat (main), Settings (tools), Model Config (configuration)
- Supports both orchestrator types via `OrchestratorWrapper` enum

**Additional Systems**
- **Embeddings** (`src/embedding/`): FastEmbed-based with LRU caching (10x speedup)
- **Context Management** (`src/context/`): Git awareness, related files detection
- **Error Recovery** (`src/agent/error_recovery.rs`): Auto-retry with rollback
- **Monitoring** (`src/agent/monitoring.rs`): Real-time metrics collection
- **Benchmarking** (`src/agent/benchmarks.rs`): Regression detection
- **Code Review** (`src/agent/code_review.rs`): AST analysis with complexity detection
- **Session Management** (`src/agent/session.rs`): Persistent conversation history

### Configuration System

Uses environment-based JSON configs (`src/config/mod.rs`):

**Priority order:**
1. CLI: `--config <path>`
2. Auto-load: `~/.config/neuro/config.{NEURO_ENV}.json` (NEURO_ENV=production|development|test)
3. Fallback: Built-in defaults

**Provider support**: Ollama (local), OpenAI, Anthropic, Groq - use different providers per model
**Environment overrides**: `NEURO_OLLAMA_URL`, `NEURO_FAST_MODEL`, `NEURO_HEAVY_MODEL`, `{OPENAI,ANTHROPIC,GROQ}_API_KEY`, `NEURO_LANG=en|es`

### Async & Concurrency

- **Tokio runtime**: All async with `tokio::spawn`, `tokio::sync::Mutex`
- **No blocking in async**: Use `tokio::task::spawn_blocking` for CPU-intensive work
- **RAPTOR indexing**: Uses `yield_low_priority()` to avoid UI blocking
- **State management**: `SharedState = Arc<Mutex<AgentState>>` for conversation history
- **Parallel tool execution**: `execute_parallel()` function in `src/agent/parallel_executor.rs` (2-3x speedup)

### Error Handling & Logging

- **Error types**: `anyhow::Result<T>` for app errors, `thiserror` for library errors
- **Logging macros**: `log_debug!()`, `log_info!()`, `log_warn!()`, `log_error!()` (custom macros in `src/logging.rs`)
- **Logging control**: `RUST_LOG` environment variable (e.g., `RUST_LOG=debug,neuro=trace`)

## Testing Strategy

### Test Categories

- **Functional tests** (`tests/functional_tests.rs`): End-to-end with real LLM models (requires Ollama)
- **Tool tests** (`tests/tool_tests.rs`): Unit tests for individual tools (no Ollama)
- **Classification tests** (`tests/router_classification_tests.rs`): Router logic validation (no Ollama)

### Adding Tests

**Functional test template:**
```rust
#[tokio::test]
#[ignore] // Only if requires Ollama
async fn test_my_feature() {
    let orchestrator = create_test_orchestrator().await.unwrap();
    let response = orchestrator.process("test prompt").await.unwrap();
    // Assert on response
}
```

**Tool test:**
```rust
#[tokio::test]
async fn test_my_tool() {
    let result = my_operation().await;
    assert!(result.is_ok());
}
```

**Quick tests (no dependencies):**
```rust
#[test]
fn test_pure_function() {
    let result = pure_logic();
    assert_eq!(result, expected);
}
```

## Common Development Tasks

### Add a New Tool
1. Create `src/tools/mytool.rs` with struct implementing `rig_core::Tool` trait
2. Register in `ToolRegistry::new()` in `src/tools/registry.rs`
3. Add tests in `tests/tool_tests.rs`
4. Tools are auto-discovered by orchestrators

### Modify Router Classification
1. Edit the classification prompt in `RouterOrchestrator::classify_request()` (`src/agent/router_orchestrator.rs`)
2. Test with: `cargo test --test router_classification_tests`
3. Debug with: `router_debug: true` in config or `--router-debug` CLI flag

### Change RAPTOR Chunking
1. Adjust `max_chars`/`overlap` in `src/raptor/chunker.rs` defaults
2. Clear embeddings with `!reindex` command in TUI
3. Rebuild: `cargo run -- raptor build ./src --max-chars 2000 --threshold 0.82`

### Debug Router Behavior
```bash
# Enable router debug logs in config.json
{
  "router_debug": true
}

# Or via CLI
cargo run -- --router-debug

# View logs
RUST_LOG=neuro::agent::router_orchestrator=trace cargo run
```

### Switch Orchestrators
Toggle in config.json: `"use_router_orchestrator": true|false` (default: false, uses DualModelOrchestrator)

### Performance Profiling
```bash
# CPU flamegraph
cargo flamegraph --release

# Memory profiling
valgrind ./target/release/neuro

# Async runtime metrics
TOKIO_CONSOLE=1 cargo run --release
```

## File Structure (Key Locations)

```
src/
├── main.rs                     # CLI entrypoint, orchestrator selection
├── agent/
│   ├── router_orchestrator.rs  # ⭐ RECOMMENDED orchestrator
│   ├── orchestrator.rs         # Dual-model routing (legacy)
│   ├── router.rs               # Query classification logic
│   ├── classifier.rs           # Task complexity detection
│   ├── code_review.rs          # AST analysis for code review
│   ├── benchmarks.rs           # Performance regression detection
│   ├── error_recovery.rs       # Auto-retry + rollback system
│   ├── monitoring.rs           # Real-time metrics
│   ├── session.rs              # Conversation persistence
│   ├── preloader.rs            # Context pre-caching
│   ├── streaming.rs            # Streaming response handling
│   ├── undo_stack.rs           # Undo/redo operations
│   ├── multistep.rs            # Multi-step task execution
│   ├── diff_preview.rs         # Visual change previews
│   └── slash_commands/         # TUI command handlers
├── tools/
│   ├── registry.rs             # Tool registration
│   ├── analyzer.rs             # Code analysis (AST)
│   ├── formatter.rs            # Code formatting
│   ├── linter.rs               # Code linting
│   ├── refactor.rs             # Code refactoring
│   ├── calculator.rs           # Math operations
│   ├── search.rs               # Full-text search
│   ├── raptor_tool.rs          # RAPTOR integration
│   ├── git.rs                  # Git operations
│   ├── shell.rs                # Shell execution
│   ├── filesystem.rs           # File operations
│   └── [11+ more tools]
├── raptor/
│   ├── builder.rs              # Index construction
│   ├── chunker.rs              # Document chunking
│   ├── clustering.rs           # Semantic clustering
│   ├── retriever.rs            # Query retrieval
│   └── summarizer.rs           # Text summarization
├── ui/
│   └── modern_app.rs           # TUI implementation
├── config/
│   └── mod.rs                  # Configuration loading
├── context/
│   ├── git_context.rs          # Git-aware context
│   └── related_files.rs        # Related file detection
├── embedding/
│   └── mod.rs                  # FastEmbed integration + caching
├── db/
│   └── mod.rs                  # SQLite persistence
├── logging.rs                  # Custom logging macros
└── security/
    └── scanner.rs              # Code security analysis
```

## Key Configuration Files

- `.github/copilot-instructions.md`: AI assistant guidelines
- `.github/workflows/ci.yml`: CI/CD pipeline
- `Cargo.toml`: Dependencies and project metadata
- `run_tests.sh`: Test automation script
- `tests/README.md`: Comprehensive test documentation

## Dependencies (Notable)

- **rig-core**: LLM abstraction layer
- **ratatui**: Terminal UI framework
- **tokio**: Async runtime
- **sqlx**: Async database access (SQLite)
- **fastembed**: Fast embeddings with LRU cache
- **tree-sitter**: AST parsing for code analysis
- **serde**: Serialization/deserialization
- **tracing**: Structured logging

## Performance Notes

- **Embedding cache**: LRU cache provides ~10x speedup for repeated queries
- **Parallel tool execution**: 2-3x faster than sequential
- **Context preloading**: Reduces latency via `src/agent/preloader.rs`
- **Embeddings**: FastEmbed is optimized for CPU (SIMD) and GPU

## Development Workflow

1. Make changes to code
2. Run `cargo check` to validate
3. Run `cargo fmt` to format
4. Run relevant tests: `./run_tests.sh fast` or `cargo test --lib`
5. Run `cargo clippy --all-targets` to check for warnings
6. Commit with conventional commits: `feat:`, `fix:`, `refactor:`, etc.

## Troubleshooting

**Compilation errors after changes:**
```bash
cargo clean
cargo check
```

**Tests fail with connection errors:**
```bash
# Verify Ollama is running
curl http://localhost:11434/api/tags

# Verify models
ollama list

# Rebuild indices if needed
cargo run -- raptor build ./src
```

**Router not classifying correctly:**
- Check `router_debug: true` in config
- Review `RouterOrchestrator::classify_request()` prompt
- Test with: `cargo test --test router_classification_tests -- --nocapture`

**Out of memory during RAPTOR indexing:**
- Use `quick_index_sync()` instead of full embedding
- Reduce `max_chars` in chunking: `cargo run -- raptor build ./src --max-chars 1000`

## Sprint Status

**Current**: Sprint 4 ✅ COMPLETE
- 5/5 features delivered (100% test pass rate)
- Features: Error recovery, code review, context preloading, performance benchmarking, production monitoring
- **Next**: Sprint 5 (Planning phase)

See [SPRINT4_FINAL_REPORT.md](SPRINT4_FINAL_REPORT.md) for detailed achievements.
