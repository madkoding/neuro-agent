//! Database migrations

/// SQL for creating the database schema
pub const INIT_SCHEMA: &str = r#"
-- Sessions table
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    title TEXT,
    fast_model TEXT NOT NULL,
    heavy_model TEXT NOT NULL,
    total_tokens INTEGER DEFAULT 0,
    working_dir TEXT
);

-- Messages table
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system', 'tool')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    model TEXT,
    tool_name TEXT,
    tokens INTEGER DEFAULT 0,
    
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

-- Command executions for auditing
CREATE TABLE IF NOT EXISTS command_executions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    message_id TEXT,
    command TEXT NOT NULL,
    exit_code INTEGER,
    stdout TEXT,
    stderr TEXT,
    executed_at TEXT NOT NULL DEFAULT (datetime('now')),
    retry_count INTEGER DEFAULT 0,
    was_dangerous INTEGER DEFAULT 0,
    risk_level TEXT,
    confirmed_by TEXT,
    
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE SET NULL
);

-- Security configuration
CREATE TABLE IF NOT EXISTS security_config (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    password_hash TEXT,
    require_password_for_dangerous INTEGER DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Create indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
CREATE INDEX IF NOT EXISTS idx_commands_session ON command_executions(session_id);
CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC);

-- ========================================================================
-- PROJECT CONTEXT CACHE TABLES
-- ========================================================================

-- Projects metadata
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    root_path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    language TEXT NOT NULL,
    project_type TEXT,
    description TEXT,
    version TEXT,
    last_indexed_at TEXT NOT NULL,
    last_modified_at TEXT NOT NULL,
    config_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexed files with hash for invalidation
CREATE TABLE IF NOT EXISTS indexed_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    absolute_path TEXT NOT NULL,
    file_hash TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    line_count INTEGER,
    language TEXT,
    file_type TEXT,
    last_modified TEXT NOT NULL,
    indexed_at TEXT NOT NULL,
    is_valid INTEGER DEFAULT 1,

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    UNIQUE(project_id, relative_path)
);

-- Code symbols (functions, structs, classes, etc.)
CREATE TABLE IF NOT EXISTS code_symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    project_id TEXT NOT NULL,
    symbol_name TEXT NOT NULL,
    symbol_type TEXT NOT NULL,
    visibility TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    signature TEXT,
    documentation TEXT,
    complexity INTEGER DEFAULT 1,
    params_json TEXT,
    return_type TEXT,
    is_async INTEGER DEFAULT 0,
    is_test INTEGER DEFAULT 0,
    parent_symbol_id INTEGER,

    FOREIGN KEY (file_id) REFERENCES indexed_files(id) ON DELETE CASCADE,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_symbol_id) REFERENCES code_symbols(id) ON DELETE SET NULL
);

-- Code dependencies (imports/uses)
CREATE TABLE IF NOT EXISTS code_dependencies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    source_file_id INTEGER NOT NULL,
    target_module TEXT NOT NULL,
    import_type TEXT NOT NULL,
    is_external INTEGER NOT NULL,
    line_number INTEGER NOT NULL,

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (source_file_id) REFERENCES indexed_files(id) ON DELETE CASCADE
);

-- Semantic relationships
CREATE TABLE IF NOT EXISTS code_relationships (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    source_symbol_id INTEGER NOT NULL,
    target_symbol_id INTEGER,
    target_name TEXT,
    relationship_type TEXT NOT NULL,
    confidence REAL DEFAULT 1.0,

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (source_symbol_id) REFERENCES code_symbols(id) ON DELETE CASCADE,
    FOREIGN KEY (target_symbol_id) REFERENCES code_symbols(id) ON DELETE CASCADE
);

-- Project-level analysis cache
CREATE TABLE IF NOT EXISTS project_analysis (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    analysis_type TEXT NOT NULL,
    analyzed_at TEXT NOT NULL,
    results_json TEXT NOT NULL,
    summary TEXT,
    score REAL,

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    UNIQUE(project_id, analysis_type)
);

-- Documentation cache
CREATE TABLE IF NOT EXISTS documentation_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    scope_identifier TEXT NOT NULL,
    format TEXT NOT NULL,
    content TEXT NOT NULL,
    generated_at TEXT NOT NULL,
    content_hash TEXT NOT NULL,

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    UNIQUE(project_id, scope, scope_identifier, format)
);

-- Search index for fast lookups
CREATE TABLE IF NOT EXISTS search_index (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id INTEGER NOT NULL,
    search_text TEXT NOT NULL,

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_projects_root_path ON projects(root_path);
CREATE INDEX IF NOT EXISTS idx_indexed_files_project ON indexed_files(project_id);
CREATE INDEX IF NOT EXISTS idx_indexed_files_hash ON indexed_files(file_hash);
CREATE INDEX IF NOT EXISTS idx_code_symbols_project ON code_symbols(project_id);
CREATE INDEX IF NOT EXISTS idx_code_symbols_name ON code_symbols(project_id, symbol_name);
CREATE INDEX IF NOT EXISTS idx_code_dependencies_project ON code_dependencies(project_id);
CREATE INDEX IF NOT EXISTS idx_search_index_text ON search_index(project_id, search_text);

-- ========================================================================
-- SEMANTIC SEARCH TABLES (Embeddings)
-- ========================================================================

-- Code embeddings for semantic search
CREATE TABLE IF NOT EXISTS code_embeddings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    chunk_id TEXT NOT NULL UNIQUE,
    chunk_type TEXT NOT NULL,
    file_id INTEGER NOT NULL,
    symbol_id INTEGER,
    embedding BLOB NOT NULL,
    chunk_text TEXT NOT NULL,
    chunk_summary TEXT,
    line_start INTEGER,
    line_end INTEGER,
    language TEXT,
    indexed_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES indexed_files(id) ON DELETE CASCADE,
    FOREIGN KEY (symbol_id) REFERENCES code_symbols(id) ON DELETE SET NULL
);

-- Embedding metadata per project
CREATE TABLE IF NOT EXISTS embedding_metadata (
    project_id TEXT PRIMARY KEY,
    model_name TEXT NOT NULL,
    model_version TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    last_updated TEXT NOT NULL,
    total_chunks INTEGER NOT NULL DEFAULT 0,

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- Full-text search index (hybrid with vector search)
CREATE VIRTUAL TABLE IF NOT EXISTS code_fts USING fts5(
    chunk_id UNINDEXED,
    chunk_text,
    file_path,
    symbol_name,
    language
);

-- Context cache for LLM
CREATE TABLE IF NOT EXISTS llm_contexts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    context_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    formatted_text TEXT NOT NULL,
    token_count INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    UNIQUE(project_id, context_type, entity_id)
);

-- Generic analysis cache
CREATE TABLE IF NOT EXISTS analysis_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    cache_key TEXT NOT NULL,
    cache_value TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,

    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    UNIQUE(project_id, cache_key)
);

-- Indexes for embeddings
CREATE INDEX IF NOT EXISTS idx_code_embeddings_project ON code_embeddings(project_id);
CREATE INDEX IF NOT EXISTS idx_code_embeddings_file ON code_embeddings(file_id);
CREATE INDEX IF NOT EXISTS idx_code_embeddings_symbol ON code_embeddings(symbol_id);
CREATE INDEX IF NOT EXISTS idx_code_embeddings_type ON code_embeddings(chunk_type);
CREATE INDEX IF NOT EXISTS idx_llm_contexts_project_type ON llm_contexts(project_id, context_type);
CREATE INDEX IF NOT EXISTS idx_analysis_cache_key ON analysis_cache(project_id, cache_key);
"#;
