# Neuro Agent - AI Coding Assistant Instructions

## Architecture Overview

Neuro is a **Rust-based AI programming assistant** combining dual-model orchestration with RAPTOR-powered RAG (Retrieval-Augmented Generation). Built with `rig-core` for LLM abstraction, `ratatui` for TUI, and `fastembed` for embeddings.

### Core Components

1. **RouterOrchestrator** ([../src/agent/router_orchestrator.rs](../src/agent/router_orchestrator.rs)) - **USE THIS**: Simplified router that classifies queries BEFORE execution (optimized for small models)
   - Three operation modes: `Ask` (read-only), `Build` (write ops), `Plan` (generate JSON plan)
   - Classifies into: `DirectResponse`, `ToolExecution`, or `FullPipeline`
   - Configure via `use_router_orchestrator: true` or `NEURO_USE_ROUTER=true`

2. **PlanningOrchestrator** ([../src/agent/planning_orchestrator.rs](../src/agent/planning_orchestrator.rs)) - **DEPRECATED**: Will be removed in v2.0 (Feb 2026)

3. **RAPTOR System** ([../src/raptor/](../src/raptor/)) - Recursive abstractive processing for hierarchical indexing
   - `quick_index_sync()`: Fast in-memory chunking without embeddings
   - `has_full_index()`: Check if embeddings are computed
   - Skips dirs: `target`, `node_modules`, `.git`, `dist`, `.venv`, `.cache`
   - Trigger with `!reindex` command in TUI

4. **Tools** ([../src/tools/](../src/tools/)) - 20+ MCP-compatible tools in `ToolRegistry`
   - Code: `analyzer`, `formatter`, `linter`, `refactor`
   - Search: `search`, `semantic_search`, `raptor_tool`
   - System: `filesystem`, `shell`, `git`, `environment`

5. **Slash Commands** ([../src/agent/slash_commands/](../src/agent/slash_commands/)) - TUI shortcuts
   - Examples: `/analyze`, `/refactor`, `/commit`, `/reindex`, `/mode`, `/help`

### Configuration System

Uses **environment-based JSON configs** ([../src/config/mod.rs](../src/config/mod.rs)):
1. CLI: `--config <path>`
2. Auto: `~/.config/neuro/config.{NEURO_ENV}.json` (NEURO_ENV=production|development|test)
3. Fallback: Defaults

**Provider support**: Ollama (local), OpenAI, Anthropic, Groq - use different providers per model.

Environment overrides: `NEURO_OLLAMA_URL`, `NEURO_FAST_MODEL`, `NEURO_HEAVY_MODEL`, `{OPENAI,ANTHROPIC,GROQ}_API_KEY`

## Development Workflows

### Build & Run
```bash
cargo build --release          # Production build
./target/release/neuro         # Launch TUI
cargo run -- --verbose         # Dev with logging
```

### Testing
```bash
./run_tests.sh                 # Full suite with checks
cargo test                     # Unit/integration tests
cargo test --test router_classification_tests  # Router tests
```

**Prerequisites**: Ollama running with `qwen3:0.6b` and `qwen3:8b` models

### Code Quality
```bash
cargo fmt                      # Format (required before commits)
cargo clippy --all-targets     # Lint (fix warnings)
cargo check                    # Validate compilation after changes
```

**Always run `cargo check` after making code changes to ensure everything compiles correctly.**

### RAPTOR Operations
```bash
# CLI indexing
cargo run -- raptor build ./src --max-chars 2000 --threshold 0.82
cargo run -- raptor query "async rust patterns" --top-k 3

# TUI: Type `!reindex` in chat
```

## Project-Specific Patterns

### Error Handling
- Use `anyhow::Result<T>` for app errors, `thiserror` for library errors
- Logging macros: `log_debug!()`, `log_info!()`, `log_warn!()`, `log_error!()` (custom, not `tracing::` directly)

### Async Conventions
- **Tokio runtime**: All async with `tokio::spawn`, `tokio::sync::Mutex`
- **No blocking in async**: Use `tokio::task::spawn_blocking` for CPU-intensive work
- **RAPTOR indexing**: `yield_low_priority()` to avoid blocking UI

### State Management
- `SharedState = Arc<Mutex<AgentState>>` for conversation history
- `AgentState` tracks: messages, pending tasks, active queries, streaming state
- TUI uses `OrchestratorWrapper` enum to support both orchestrator types

### Tool Development
1. Implement in `src/tools/<name>.rs` with struct + `Tool` trait from `rig-core`
2. Register in `ToolRegistry::new()` ([../src/tools/registry.rs](../src/tools/registry.rs))
3. Tools are auto-discovered by orchestrators

### Slash Command Pattern
```rust
pub struct MyCommand;
#[async_trait::async_trait]
impl SlashCommand for MyCommand {
    fn name(&self) -> &str { "mycommand" }
    fn category(&self) -> CommandCategory { /* ... */ }
    async fn execute(&self, args: &str, ctx: &CommandContext) -> Result<CommandResult> {
        // Access ctx.tools, ctx.state, ctx.working_dir
    }
}
```

Register in `SlashCommandRegistry::new()`.

### I18n
- Use `t!(key)` macro for UI strings, `current_locale()` for conditionals
- Locale set via `NEURO_LANG=en|es` (default: system locale)

## Key Files

- [Cargo.toml](../Cargo.toml): Dependencies (rig-core, ratatui, fastembed, tree-sitter, sqlx)
- [src/main.rs](../src/main.rs): CLI entrypoint, chooses orchestrator based on config
- [src/ui/modern_app.rs](../src/ui/modern_app.rs): TUI app with dual orchestrator support
- [TUI_ROUTER_INTEGRATION.md](../TUI_ROUTER_INTEGRATION.md): Migration guide PlanningOrchestrator → RouterOrchestrator
- [tests/README.md](../tests/README.md): Test categories and examples
- [run_tests.sh](../run_tests.sh): Pre-flight checks for Ollama + models

## Common Tasks

**Add a new tool**: Create `src/tools/mytool.rs`, implement `rig_core::Tool`, register in `registry.rs`

**Modify router classification**: Edit prompt in `RouterOrchestrator::classify_request()`

**Change RAPTOR chunking**: Adjust `max_chars`/`overlap` in `chunker.rs` defaults

**Debug router**: Set `router_debug: true` in config or `--router-debug` CLI flag

**Switch orchestrators**: Toggle `use_router_orchestrator` in config.json
