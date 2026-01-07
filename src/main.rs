//! Neuro - High-performance CLI AI Agent for programmers
//!
//! Uses dual-model architecture with Ollama:
//! - qwen3:8b for all interactions with tool support

use clap::Parser;
use directories::ProjectDirs;
use neuro::{
    agent::{DualModelOrchestrator, PlanningOrchestrator},
    db::Database,
    i18n::init_locale,
    ui::ModernApp,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Neuro - AI Programming Assistant CLI

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// RAPTOR operations
    Raptor {
        #[command(subcommand)]
        cmd: RaptorCmd,
    },
}

#[derive(clap::Subcommand, Debug)]
enum RaptorCmd {
    /// Build RAPTOR index for a directory
    Build {
        /// Directory to index
        path: PathBuf,
        /// Chunk max chars
        #[arg(long, default_value_t = 2000)]
        max_chars: usize,
        /// Chunk overlap
        #[arg(long, default_value_t = 200)]
        overlap: usize,
        /// clustering threshold (0..1)
        #[arg(long, default_value_t = 0.82_f32)]
        threshold: f32,
    },
    /// Query the RAPTOR index
    Query {
        /// Query text
        text: String,
        /// Number of top summaries to retrieve
        #[arg(long, default_value_t = 3)]
        top_k: usize,
        /// Number of chunks to expand for context
        #[arg(long, default_value_t = 5)]
        expand_k: usize,
        /// Confidence threshold (0..1) to skip chunk fallback
        #[arg(long, default_value_t = 0.95_f32)]
        chunk_threshold: f32,
    },
}

#[derive(Parser, Debug)]
#[command(name = "neuro")]
#[command(author = "Neuro Team")]
#[command(version = "0.1.0")]
#[command(about = "High-performance CLI AI Agent for programmers", long_about = None)]
struct Args {
    /// Ollama API URL
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,

    /// Model for chat and tools (qwen3:8b recommended)
    #[arg(long, default_value = "qwen3:8b")]
    fast_model: String,

    /// Model for complex tasks (same as fast_model by default)
    #[arg(long, default_value = "qwen3:8b")]
    heavy_model: String,

    /// Database path (default: ~/.local/share/neuro/neuro.db)
    #[arg(long)]
    db_path: Option<PathBuf>,

    /// Working directory
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Enable debug logging
    #[arg(short, long)]
    verbose: bool,

    /// Skip TUI and run in simple mode
    #[arg(long)]
    simple: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging (disabled in TUI mode)
    init_logging(args.verbose, !args.simple);

    // Get database path
    let db_path = args.db_path.unwrap_or_else(|| {
        ProjectDirs::from("com", "neuro", "neuro")
            .map(|dirs| dirs.data_dir().join("neuro.db"))
            .unwrap_or_else(|| PathBuf::from("neuro.db"))
    });

    // Initialize database
    tracing::info!("Initializing database at {:?}", db_path);
    let _db = Database::new(&db_path).await?;

    // Initialize orchestrator
    tracing::info!("Connecting to Ollama at {}", args.ollama_url);
    let config = neuro::agent::orchestrator::OrchestratorConfig {
        ollama_url: args.ollama_url.clone(),
        fast_model: args.fast_model.clone(),
        heavy_model: args.heavy_model.clone(),
        heavy_timeout_secs: 1200,
        max_concurrent_heavy: 2,
    };

    let dual_orchestrator = match DualModelOrchestrator::with_config(config).await {
        Ok(orch) => orch,
        Err(e) => {
            eprintln!("❌ Failed to connect to Ollama: {}", e);
            eprintln!("\nMake sure Ollama is running:");
            eprintln!("  ollama serve");
            eprintln!("\nAnd that you have the required models:");
            eprintln!("  ollama pull {}", args.fast_model);
            eprintln!("  ollama pull {}", args.heavy_model);
            return Err(e.into());
        }
    };

    // Get working directory
    let working_dir = args
        .dir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Wrap orchestrator early so we can pass it into RAPTOR routines safely
    let dual_arc = Arc::new(Mutex::new(dual_orchestrator));

    // If a subcommand was provided, handle it and exit
    if let Some(cmd) = args.command {
        match cmd {
            Command::Raptor { cmd } => match cmd {
                RaptorCmd::Build {
                    path,
                    max_chars,
                    overlap,
                    threshold,
                } => {
                    println!("Building RAPTOR tree for {:?}", path);
                    let root = neuro::raptor::builder::build_tree(
                        &path,
                        dual_arc.clone(),
                        max_chars,
                        overlap,
                        threshold,
                    )
                    .await?;
                    println!("RAPTOR root id: {}", root);
                    return Ok(());
                }
                RaptorCmd::Query {
                    text,
                    top_k,
                    expand_k,
                    chunk_threshold,
                } => {
                    println!("Query: {}", text);
                    // Build retriever and run query
                    let embedder = neuro::embedding::EmbeddingEngine::new().await?;
                    let store_guard = neuro::raptor::persistence::GLOBAL_STORE.lock().unwrap();
                    let store = &*store_guard;
                    let retriever = neuro::raptor::retriever::TreeRetriever::new(&embedder, store);
                    let (summaries, chunks) = retriever
                        .retrieve_with_context(&text, top_k, expand_k, chunk_threshold)
                        .await?;

                    println!("Top summaries:");
                    for (id, score, summary) in summaries.iter() {
                        println!("- {} (score: {:.3})", id, score);
                        println!("  summary: {}", summary);
                    }

                    if !chunks.is_empty() {
                        println!("Top chunks (fallback):");
                        for (id, score, chunk) in chunks.iter() {
                            println!("- {} (score: {:.3})", id, score);
                            println!("  chunk: {}", chunk);
                        }
                    }

                    // Build a context and call the orchestrator for final answer
                    let mut context = String::new();
                    for (_, _, summary) in summaries.iter() {
                        context.push_str(summary);
                        context.push_str("\n---\n");
                    }
                    for (_, _, chunk) in chunks.iter() {
                        context.push_str(chunk);
                        context.push_str("\n---\n");
                    }

                    let prompt = format!("Usando este contexto:\n{}\nRESPONDE: {}", context, text);
                    let answer = dual_arc
                        .lock()
                        .await
                        .call_heavy_model_direct(&prompt)
                        .await?;
                    println!("Respuesta: {}", answer);
                    return Ok(());
                }
            },
        }
    }

    // Extract references before wrapping into PlanningOrchestrator
    let tools = {
        let guard = dual_arc.lock().await;
        guard.tools().clone()
    };
    let state = {
        let guard = dual_arc.lock().await;
        guard.state().clone()
    };

    // Wrap in PlanningOrchestrator for multi-step planning
    let orchestrator = PlanningOrchestrator::new(dual_arc, Arc::new(tools), state, working_dir);

    if args.simple {
        eprintln!("Simple mode not yet supported with PlanningOrchestrator");
        return Ok(());
    } else {
        run_modern_tui(orchestrator).await
    }
}

/// Initialize logging
fn init_logging(verbose: bool, tui_mode: bool) {
    // Disable logging in TUI mode to avoid interfering with the interface
    if tui_mode {
        return;
    }

    let filter = if verbose {
        "neuro=debug,info"
    } else {
        "neuro=info,warn"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();
}

/// Run the modern TUI mode
async fn run_modern_tui(orchestrator: PlanningOrchestrator) -> anyhow::Result<()> {
    // Initialize locale
    let locale = init_locale();
    tracing::info!("Using locale: {}", locale.display_name());

    // Create and run modern app
    let mut app = ModernApp::new(orchestrator).await?;
    app.run().await?;

    Ok(())
}
