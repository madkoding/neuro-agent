//! Neuro - High-performance CLI AI Agent for programmers
//!
//! Uses dual-model architecture with Ollama:
//! - qwen3:8b for all interactions with tool support

#![allow(deprecated)]

use clap::Parser;
use directories::ProjectDirs;
use neuro::{
    agent::{DualModelOrchestrator, PlanningOrchestrator, RouterOrchestrator, RouterConfig},
    db::Database,
    i18n::{init_locale, init_locale_with, Locale},
    ui::ModernApp,
    log_error, log_info, logging,
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
    /// Configuration file path (overrides defaults)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Ollama API URL (deprecated: use --config)
    #[arg(long)]
    ollama_url: Option<String>,

    /// Model for chat and tools (deprecated: use --config)
    #[arg(long)]
    fast_model: Option<String>,

    /// Model for complex tasks (deprecated: use --config)
    #[arg(long)]
    heavy_model: Option<String>,

    /// Database path (default: ~/.local/share/neuro/neuro.db)
    #[arg(long)]
    db_path: Option<PathBuf>,

    /// Working directory
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Enable debug logging
    #[arg(short, long)]
    verbose: bool,
    
    /// Enable router debug logs
    #[arg(long)]
    debug: bool,

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

    // Load configuration
    let mut app_config = neuro::config::AppConfig::load(args.config.as_deref())?;
    
    // Initialize locale based on configuration
    if let Some(ref lang) = app_config.language {
        let locale = match lang.as_str() {
            "es" | "español" | "spanish" => Locale::Spanish,
            "en" | "english" | "inglés" => Locale::English,
            _ => Locale::detect(),
        };
        init_locale_with(locale);
    } else {
        init_locale();
    }
    
    // Apply CLI overrides (for backward compatibility)
    if let Some(url) = args.ollama_url {
        if app_config.fast_model.provider == neuro::config::ModelProvider::Ollama {
            app_config.fast_model.url = url.clone();
        }
        if app_config.heavy_model.provider == neuro::config::ModelProvider::Ollama {
            app_config.heavy_model.url = url;
        }
    }
    if let Some(model) = args.fast_model {
        app_config.fast_model.model = model;
    }
    if let Some(model) = args.heavy_model {
        app_config.heavy_model.model = model;
    }
    
    // Validate configuration
    app_config.validate()?;

    // Initialize orchestrator (using old OrchestratorConfig for now - will refactor later)
    tracing::info!(
        "Connecting to {} at {}",
        app_config.fast_model.provider,
        app_config.fast_model.url
    );
    
    let config = neuro::agent::orchestrator::OrchestratorConfig {
        ollama_url: app_config.fast_model.url.clone(),
        fast_model: app_config.fast_model.model.clone(),
        heavy_model: app_config.heavy_model.model.clone(),
        heavy_timeout_secs: app_config.heavy_timeout_secs,
        max_concurrent_heavy: app_config.max_concurrent_heavy,
    };

    // Test connection first
    let _test_orch = match DualModelOrchestrator::with_config(config.clone()).await {
        Ok(orch) => orch,
        Err(e) => {
            log_error!("❌ Failed to connect to model provider: {}", e);
            log_error!("\nFor Ollama, make sure it's running:");
            log_error!("  ollama serve");
            log_error!("\nAnd that you have the required models:");
            log_error!("  ollama pull {}", app_config.fast_model.model);
            log_error!("  ollama pull {}", app_config.heavy_model.model);
            log_error!("\nFor other providers, check your API keys and configuration.");
            return Err(e.into());
        }
    };

    // Get working directory
    let working_dir = args
        .dir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // If a subcommand was provided, handle it and exit
    if let Some(cmd) = args.command {
        // Create orchestrator for subcommands
        let dual_orchestrator = DualModelOrchestrator::with_config(config.clone()).await?;
        let dual_arc = Arc::new(Mutex::new(dual_orchestrator));
        
        match cmd {
            Command::Raptor { cmd } => match cmd {
                RaptorCmd::Build {
                    path,
                    max_chars,
                    overlap,
                    threshold,
                } => {
                    log_info!("Building RAPTOR tree for {:?}", path);
                    let root = neuro::raptor::builder::build_tree(
                        &path,
                        dual_arc.clone(),
                        max_chars,
                        overlap,
                        threshold,
                    )
                    .await?;
                    log_info!("RAPTOR root id: {}", root);
                    return Ok(());
                }
                RaptorCmd::Query {
                    text,
                    top_k,
                    expand_k,
                    chunk_threshold,
                } => {
                    log_info!("Query: {}", text);
                    // Build retriever and run query
                    let embedder = neuro::embedding::EmbeddingEngine::new().await?;
                    
                    // Clone store to avoid holding lock across await
                    let store_clone = {
                        let store_guard = neuro::raptor::persistence::GLOBAL_STORE.lock().unwrap();
                        store_guard.clone()
                    }; // Lock is released here
                    
                    // Now perform async operation without holding the lock
                    let retriever = neuro::raptor::retriever::TreeRetriever::new(&embedder, &store_clone);
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

    // Choose orchestrator based on configuration
    if app_config.use_router_orchestrator {
        // Use new RouterOrchestrator (simplified, optimized for small models)
        tracing::info!("Using RouterOrchestrator (new simplified router)");
        
        let router_config = RouterConfig {
            ollama_url: app_config.fast_model.url.clone(),
            fast_model: app_config.fast_model.model.clone(),
            heavy_model: app_config.heavy_model.model.clone(),
            classification_timeout_secs: 30,
            min_confidence: 0.8,
            working_dir: working_dir.to_string_lossy().to_string(),
            locale: init_locale(),
            debug: args.debug,
        };
        
        // Create new DualModelOrchestrator for RouterOrchestrator
        let dual_for_router = DualModelOrchestrator::with_config(config).await?;
        let router = RouterOrchestrator::new(router_config, dual_for_router).await?;
        
        // Initialize RAPTOR index
        router.initialize_raptor().await?;
        
        if args.simple {
            eprintln!("Simple mode not yet supported with RouterOrchestrator");
            return Ok(());
        } else {
            run_modern_tui_with_router(router).await
        }
    } else {
        // Use legacy PlanningOrchestrator (deprecated)
        eprintln!("⚠ PlanningOrchestrator deprecated - use RouterOrchestrator");
        eprintln!("  Set use_router_orchestrator: true in config or NEURO_USE_ROUTER=true");
        eprintln!("  (Using legacy orchestrator for now as UI is integrated)");
        
        // Create DualModelOrchestrator for PlanningOrchestrator
        let dual_orchestrator = DualModelOrchestrator::with_config(config).await?;
        let dual_arc = Arc::new(Mutex::new(dual_orchestrator));
        
        // Extract references before wrapping into PlanningOrchestrator
        let tools = {
            let guard = dual_arc.lock().await;
            guard.tools().clone()
        };
        let state = {
            let guard = dual_arc.lock().await;
            guard.state().clone()
        };
        
        let orchestrator = PlanningOrchestrator::new(dual_arc, Arc::new(tools), state, working_dir);

        if args.simple {
            eprintln!("Simple mode not yet supported with PlanningOrchestrator");
            return Ok(());
        } else {
            run_modern_tui(orchestrator).await
        }
    }
}

/// Initialize logging
fn init_logging(verbose: bool, tui_mode: bool) {
    // In TUI mode, use file logging to avoid interfering with the interface
    if tui_mode {
        let _ = logging::init_logger();
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

/// Run the modern TUI mode with RouterOrchestrator
async fn run_modern_tui_with_router(router: RouterOrchestrator) -> anyhow::Result<()> {
    // Initialize locale
    let locale = init_locale();
    tracing::info!("Using locale: {}", locale.display_name());

    // Create and run modern app with router
    let mut app = ModernApp::new_with_router(router).await?;
    app.run().await?;

    Ok(())
}
