//! Internationalization module - Spanish and English support

use std::sync::{Mutex, OnceLock};

static CURRENT_LOCALE: OnceLock<Mutex<Locale>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    English,
    Spanish,
}

impl Locale {
    /// Detect locale from system environment
    pub fn detect() -> Self {
        // Check LANG, LC_ALL, LC_MESSAGES environment variables
        let lang = std::env::var("LANG")
            .or_else(|_| std::env::var("LC_ALL"))
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .unwrap_or_default()
            .to_lowercase();

        if lang.starts_with("es") {
            Locale::Spanish
        } else {
            Locale::English
        }
    }

    /// Get the locale code for LLM prompts
    pub fn code(&self) -> &'static str {
        match self {
            Locale::English => "en",
            Locale::Spanish => "es",
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Locale::English => "English",
            Locale::Spanish => "Español",
        }
    }
}

/// Initialize the global locale
pub fn init_locale() -> Locale {
    let locale = Locale::detect();
    let _ = CURRENT_LOCALE.set(Mutex::new(locale));
    locale
}

/// Initialize with specific locale
pub fn init_locale_with(locale: Locale) -> Locale {
    let _ = CURRENT_LOCALE.set(Mutex::new(locale));
    locale
}

/// Get current locale
pub fn current_locale() -> Locale {
    CURRENT_LOCALE
        .get()
        .and_then(|m| m.lock().ok())
        .map(|l| *l)
        .unwrap_or(Locale::English)
}

/// Set current locale
pub fn set_locale(locale: Locale) {
    if let Some(mutex) = CURRENT_LOCALE.get() {
        if let Ok(mut current) = mutex.lock() {
            *current = locale;
        }
    }
}

/// Translation keys
#[derive(Debug, Clone, Copy)]
pub enum Text {
    // App titles and headers
    AppTitle,
    SettingsTitle,
    ToolsTitle,
    ChatTitle,
    StatusTitle,

    // Status messages
    Ready,
    Thinking,
    Processing,
    Executing,
    Completed,
    Error,
    Cancelled,

    // Tool names and descriptions
    ToolFileRead,
    ToolFileReadDesc,
    ToolFileWrite,
    ToolFileWriteDesc,
    ToolListDir,
    ToolListDirDesc,
    ToolShellExec,
    ToolShellExecDesc,
    ToolLinter,
    ToolLinterDesc,
    // New tools
    ToolIndexer,
    ToolIndexerDesc,
    ToolSearch,
    ToolSearchDesc,
    ToolGit,
    ToolGitDesc,
    ToolAnalyzer,
    ToolAnalyzerDesc,
    ToolDependencies,
    ToolDependenciesDesc,
    ToolHttp,
    ToolHttpDesc,
    ToolShellAdvanced,
    ToolShellAdvancedDesc,
    ToolTestRunner,
    ToolTestRunnerDesc,
    ToolDocumentation,
    ToolDocumentationDesc,
    ToolFormatter,
    ToolFormatterDesc,
    ToolRefactor,
    ToolRefactorDesc,
    ToolSnippets,
    ToolSnippetsDesc,
    ToolContext,
    ToolContextDesc,
    ToolEnvironment,
    ToolEnvironmentDesc,
    ToolPlanner,
    ToolPlannerDesc,

    // UI Elements
    InputPlaceholder,
    PressEnterToSend,
    PressEscToCancel,
    PressTabForSettings,
    PressQToQuit,
    ToolsEnabled,
    ToolsDisabled,
    ToggleTool,
    BackToChat,

    // Confirmations
    ConfirmCommand,
    DangerousCommand,
    EnterPassword,
    PasswordRequired,

    // Errors
    ConnectionError,
    TimeoutError,
    ToolError,
    UnknownError,

    // Prompts for LLM
    SystemPromptIntro,
    LanguageInstruction,
}

impl Text {
    pub fn get(&self) -> &'static str {
        match current_locale() {
            Locale::English => self.english(),
            Locale::Spanish => self.spanish(),
        }
    }

    fn english(&self) -> &'static str {
        match self {
            // App titles
            Text::AppTitle => "neuro - AI Programming Assistant",
            Text::SettingsTitle => "⚙ Settings",
            Text::ToolsTitle => "🔧 Available Tools",
            Text::ChatTitle => "💬 Chat",
            Text::StatusTitle => "Status",

            // Status
            Text::Ready => "Ready",
            Text::Thinking => "Thinking",
            Text::Processing => "Processing",
            Text::Executing => "Executing",
            Text::Completed => "Completed",
            Text::Error => "Error",
            Text::Cancelled => "Cancelled",

            // Tools
            Text::ToolFileRead => "File Reader",
            Text::ToolFileReadDesc => "Read file contents with line ranges",
            Text::ToolFileWrite => "File Writer",
            Text::ToolFileWriteDesc => "Write or append to files",
            Text::ToolListDir => "Directory Listing",
            Text::ToolListDirDesc => "List directory contents recursively",
            Text::ToolShellExec => "Shell Executor",
            Text::ToolShellExecDesc => "Execute shell commands safely",
            Text::ToolLinter => "Code Linter",
            Text::ToolLinterDesc => "Run cargo clippy/check for diagnostics",
            // New tools
            Text::ToolIndexer => "Project Indexer",
            Text::ToolIndexerDesc => "Index project files for context",
            Text::ToolSearch => "File Search",
            Text::ToolSearchDesc => "Search patterns in files (grep)",
            Text::ToolGit => "Git Operations",
            Text::ToolGitDesc => "Git status, diff, log, commit, blame",
            Text::ToolAnalyzer => "Code Analyzer",
            Text::ToolAnalyzerDesc => "Analyze code metrics and complexity",
            Text::ToolDependencies => "Dependency Analyzer",
            Text::ToolDependenciesDesc => "Analyze project dependencies",
            Text::ToolHttp => "HTTP Client",
            Text::ToolHttpDesc => "Make HTTP requests to APIs",
            Text::ToolShellAdvanced => "Advanced Shell",
            Text::ToolShellAdvancedDesc => "Shell with streaming output",
            Text::ToolTestRunner => "Test Runner",
            Text::ToolTestRunnerDesc => "Run tests (cargo, pytest, jest)",
            Text::ToolDocumentation => "Documentation",
            Text::ToolDocumentationDesc => "Generate code documentation",
            Text::ToolFormatter => "Code Formatter",
            Text::ToolFormatterDesc => "Format code in multiple languages",
            Text::ToolRefactor => "Refactoring",
            Text::ToolRefactorDesc => "Rename, extract, inline code",
            Text::ToolSnippets => "Code Snippets",
            Text::ToolSnippetsDesc => "Code templates and snippets",
            Text::ToolContext => "Project Context",
            Text::ToolContextDesc => "Get full project context",
            Text::ToolEnvironment => "Environment Info",
            Text::ToolEnvironmentDesc => "System and environment info",
            Text::ToolPlanner => "Task Planner",
            Text::ToolPlannerDesc => "Plan and break down tasks",

            // UI
            Text::InputPlaceholder => "Type your message...",
            Text::PressEnterToSend => "Enter to send",
            Text::PressEscToCancel => "Esc to cancel",
            Text::PressTabForSettings => "Tab for settings",
            Text::PressQToQuit => "Q to quit",
            Text::ToolsEnabled => "enabled",
            Text::ToolsDisabled => "disabled",
            Text::ToggleTool => "Space to toggle",
            Text::BackToChat => "Tab to return",

            // Confirmations
            Text::ConfirmCommand => "Confirm command execution?",
            Text::DangerousCommand => "⚠ Dangerous command detected",
            Text::EnterPassword => "Enter password:",
            Text::PasswordRequired => "Password required for this action",

            // Errors
            Text::ConnectionError => "Connection error - check Ollama",
            Text::TimeoutError => "Request timed out",
            Text::ToolError => "Tool execution failed",
            Text::UnknownError => "Unknown error occurred",

            // LLM Prompts
            Text::SystemPromptIntro => "You are a helpful AI programming assistant.",
            Text::LanguageInstruction => "Always respond in English.",
        }
    }

    fn spanish(&self) -> &'static str {
        match self {
            // App titles
            Text::AppTitle => "neuro - Asistente de Programación IA",
            Text::SettingsTitle => "⚙ Configuración",
            Text::ToolsTitle => "🔧 Herramientas Disponibles",
            Text::ChatTitle => "💬 Chat",
            Text::StatusTitle => "Estado",

            // Status
            Text::Ready => "Listo",
            Text::Thinking => "Pensando",
            Text::Processing => "Procesando",
            Text::Executing => "Ejecutando",
            Text::Completed => "Completado",
            Text::Error => "Error",
            Text::Cancelled => "Cancelado",

            // Tools
            Text::ToolFileRead => "Lector de Archivos",
            Text::ToolFileReadDesc => "Leer contenido con rangos de líneas",
            Text::ToolFileWrite => "Escritor de Archivos",
            Text::ToolFileWriteDesc => "Escribir o agregar a archivos",
            Text::ToolListDir => "Listar Directorio",
            Text::ToolListDirDesc => "Listar contenidos recursivamente",
            Text::ToolShellExec => "Ejecutor de Comandos",
            Text::ToolShellExecDesc => "Ejecutar comandos de forma segura",
            Text::ToolLinter => "Analizador de Código",
            Text::ToolLinterDesc => "Ejecutar cargo clippy/check",
            // New tools
            Text::ToolIndexer => "Indexador de Proyecto",
            Text::ToolIndexerDesc => "Indexar archivos del proyecto",
            Text::ToolSearch => "Búsqueda en Archivos",
            Text::ToolSearchDesc => "Buscar patrones (grep)",
            Text::ToolGit => "Operaciones Git",
            Text::ToolGitDesc => "Git status, diff, log, commit, blame",
            Text::ToolAnalyzer => "Analizador de Código",
            Text::ToolAnalyzerDesc => "Analizar métricas y complejidad",
            Text::ToolDependencies => "Analizador de Dependencias",
            Text::ToolDependenciesDesc => "Analizar dependencias del proyecto",
            Text::ToolHttp => "Cliente HTTP",
            Text::ToolHttpDesc => "Hacer peticiones HTTP a APIs",
            Text::ToolShellAdvanced => "Shell Avanzado",
            Text::ToolShellAdvancedDesc => "Shell con salida en streaming",
            Text::ToolTestRunner => "Ejecutor de Tests",
            Text::ToolTestRunnerDesc => "Ejecutar tests (cargo, pytest, jest)",
            Text::ToolDocumentation => "Documentación",
            Text::ToolDocumentationDesc => "Generar documentación de código",
            Text::ToolFormatter => "Formateador de Código",
            Text::ToolFormatterDesc => "Formatear código en varios lenguajes",
            Text::ToolRefactor => "Refactorización",
            Text::ToolRefactorDesc => "Renombrar, extraer, inline",
            Text::ToolSnippets => "Snippets de Código",
            Text::ToolSnippetsDesc => "Templates y snippets de código",
            Text::ToolContext => "Contexto del Proyecto",
            Text::ToolContextDesc => "Obtener contexto completo del proyecto",
            Text::ToolEnvironment => "Info del Entorno",
            Text::ToolEnvironmentDesc => "Información del sistema y entorno",
            Text::ToolPlanner => "Planificador de Tareas",
            Text::ToolPlannerDesc => "Planificar y dividir tareas",

            // UI
            Text::InputPlaceholder => "Escribe tu mensaje...",
            Text::PressEnterToSend => "Enter para enviar",
            Text::PressEscToCancel => "Esc para cancelar",
            Text::PressTabForSettings => "Tab para ajustes",
            Text::PressQToQuit => "Q para salir",
            Text::ToolsEnabled => "activado",
            Text::ToolsDisabled => "desactivado",
            Text::ToggleTool => "Espacio para cambiar",
            Text::BackToChat => "Tab para volver",

            // Confirmations
            Text::ConfirmCommand => "¿Confirmar ejecución del comando?",
            Text::DangerousCommand => "⚠ Comando peligroso detectado",
            Text::EnterPassword => "Ingresa contraseña:",
            Text::PasswordRequired => "Se requiere contraseña para esta acción",

            // Errors
            Text::ConnectionError => "Error de conexión - verifica Ollama",
            Text::TimeoutError => "Tiempo de espera agotado",
            Text::ToolError => "Error en ejecución de herramienta",
            Text::UnknownError => "Error desconocido",

            // LLM Prompts
            Text::SystemPromptIntro => "Eres un asistente de programación IA útil.",
            Text::LanguageInstruction => "Siempre responde en español.",
        }
    }
}

/// Shorthand for getting translated text
pub fn t(text: Text) -> &'static str {
    text.get()
}

/// Get language instruction for LLM
pub fn llm_language_instruction() -> &'static str {
    t(Text::LanguageInstruction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_detection() {
        // This test depends on system locale
        let locale = Locale::detect();
        assert!(matches!(locale, Locale::English | Locale::Spanish));
    }

    #[test]
    fn test_translations_exist() {
        // Ensure all translations have both versions
        let texts = [
            Text::AppTitle,
            Text::Ready,
            Text::ToolFileRead,
            Text::InputPlaceholder,
        ];

        for text in texts {
            assert!(!text.english().is_empty());
            assert!(!text.spanish().is_empty());
        }
    }
}
