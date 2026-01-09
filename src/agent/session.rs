//! Session Management - Sistema de persistencia de sesiones de conversación
//!
//! Permite guardar y cargar sesiones de conversación completas con su contexto,
//! incluyendo mensajes, archivos modificados, estado del proyecto, etc.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Contexto de una sesión
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    /// Directorio de trabajo actual
    pub working_dir: PathBuf,
    /// Última tarea ejecutada
    pub last_task: String,
    /// Archivos modificados durante la sesión
    pub files_modified: Vec<PathBuf>,
    /// Rama de git actual (si aplica)
    pub git_branch: Option<String>,
    /// Variables de entorno capturadas
    pub environment: HashMap<String, String>,
}

impl SessionContext {
    /// Crea un contexto vacío
    pub fn new() -> Self {
        Self {
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            last_task: String::new(),
            files_modified: Vec::new(),
            git_branch: None,
            environment: HashMap::new(),
        }
    }

    /// Crea un contexto capturando el estado actual
    pub fn capture() -> Result<Self> {
        let working_dir = std::env::current_dir()?;
        
        // Intentar obtener la rama de git
        let git_branch = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&working_dir)
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .ok()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                } else {
                    None
                }
            });

        // Capturar variables de entorno relevantes
        let mut environment = HashMap::new();
        if let Ok(path) = std::env::var("PATH") {
            environment.insert("PATH".to_string(), path);
        }
        if let Ok(home) = std::env::var("HOME") {
            environment.insert("HOME".to_string(), home);
        }
        if let Ok(shell) = std::env::var("SHELL") {
            environment.insert("SHELL".to_string(), shell);
        }

        Ok(Self {
            working_dir,
            last_task: String::new(),
            files_modified: Vec::new(),
            git_branch,
            environment,
        })
    }

    /// Agrega un archivo a la lista de modificados
    pub fn add_modified_file(&mut self, path: PathBuf) {
        if !self.files_modified.contains(&path) {
            self.files_modified.push(path);
        }
    }
}

impl Default for SessionContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Mensaje en una sesión (SessionMessage para evitar conflicto con state::Message)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    /// Remitente del mensaje ("user" o "assistant")
    pub role: String,
    /// Contenido del mensaje
    pub content: String,
    /// Timestamp del mensaje
    pub timestamp: SystemTime,
}

impl SessionMessage {
    /// Crea un nuevo mensaje
    pub fn new(role: String, content: String) -> Self {
        Self {
            role,
            content,
            timestamp: SystemTime::now(),
        }
    }

    /// Crea un mensaje de usuario
    pub fn user(content: String) -> Self {
        Self::new("user".to_string(), content)
    }

    /// Crea un mensaje del asistente
    pub fn assistant(content: String) -> Self {
        Self::new("assistant".to_string(), content)
    }
}

/// Sesión de conversación completa
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// ID único de la sesión
    pub id: String,
    /// Nombre descriptivo de la sesión
    pub name: String,
    /// Momento de creación
    pub created_at: SystemTime,
    /// Última actualización
    pub updated_at: SystemTime,
    /// Mensajes de la conversación
    pub messages: Vec<SessionMessage>,
    /// Contexto de la sesión
    pub context: SessionContext,
}

impl Session {
    /// Crea una nueva sesión
    pub fn new(name: String) -> Self {
        let now = SystemTime::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            context: SessionContext::new(),
        }
    }

    /// Crea una sesión con contexto capturado
    pub fn with_context(name: String) -> Result<Self> {
        let now = SystemTime::now();
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            context: SessionContext::capture()?,
        })
    }

    /// Agrega un mensaje a la sesión
    pub fn add_message(&mut self, message: SessionMessage) {
        self.messages.push(message);
        self.updated_at = SystemTime::now();
    }

    /// Actualiza el contexto de la sesión
    pub fn update_context(&mut self, context: SessionContext) {
        self.context = context;
        self.updated_at = SystemTime::now();
    }

    /// Guarda la sesión en un archivo JSON
    pub fn save(&self, path: &Path) -> Result<()> {
        // Crear directorio si no existe
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Carga una sesión desde un archivo JSON
    pub fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)
            .context(format!("Failed to read session from {:?}", path))?;
        let session: Session = serde_json::from_str(&json)
            .context("Failed to parse session JSON")?;
        Ok(session)
    }

    /// Genera un resumen de la sesión
    pub fn summary(&self) -> String {
        let duration = self.updated_at
            .duration_since(self.created_at)
            .unwrap_or_default();
        let duration_mins = duration.as_secs() / 60;

        format!(
            "Session '{}' ({} messages, {} files modified, {}m active)",
            self.name,
            self.messages.len(),
            self.context.files_modified.len(),
            duration_mins
        )
    }
}

/// Gestor de sesiones
#[derive(Debug)]
pub struct SessionManager {
    /// Directorio donde se guardan las sesiones
    sessions_dir: PathBuf,
    /// Sesión activa actual
    active_session: Option<Session>,
}

impl SessionManager {
    /// Crea un nuevo gestor de sesiones
    pub fn new(sessions_dir: PathBuf) -> Self {
        Self {
            sessions_dir,
            active_session: None,
        }
    }

    /// Crea un gestor con directorio por defecto (~/.neuro/sessions)
    pub fn default() -> Result<Self> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .context("Could not determine home directory")?;
        let sessions_dir = PathBuf::from(home).join(".neuro").join("sessions");
        Ok(Self::new(sessions_dir))
    }

    /// Crea una nueva sesión
    pub fn create_session(&mut self, name: String) -> Result<&Session> {
        let session = Session::with_context(name)?;
        self.active_session = Some(session);
        Ok(self.active_session.as_ref().unwrap())
    }

    /// Guarda la sesión activa
    pub fn save_active(&self) -> Result<()> {
        if let Some(ref session) = self.active_session {
            let path = self.sessions_dir.join(format!("{}.json", session.id));
            session.save(&path)?;
            Ok(())
        } else {
            Err(anyhow!("No active session to save"))
        }
    }

    /// Carga una sesión por ID
    pub fn load_session(&mut self, session_id: &str) -> Result<&Session> {
        let path = self.sessions_dir.join(format!("{}.json", session_id));
        let session = Session::load(&path)?;
        self.active_session = Some(session);
        Ok(self.active_session.as_ref().unwrap())
    }

    /// Lista todas las sesiones disponibles
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        if !self.sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        
        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(session) = Session::load(&path) {
                    sessions.push(SessionInfo {
                        id: session.id,
                        name: session.name,
                        created_at: session.created_at,
                        updated_at: session.updated_at,
                        message_count: session.messages.len(),
                        files_modified: session.context.files_modified.len(),
                    });
                }
            }
        }

        // Ordenar por fecha de actualización (más reciente primero)
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        
        Ok(sessions)
    }

    /// Obtiene la sesión activa
    pub fn active_session(&self) -> Option<&Session> {
        self.active_session.as_ref()
    }

    /// Obtiene la sesión activa mutable
    pub fn active_session_mut(&mut self) -> Option<&mut Session> {
        self.active_session.as_mut()
    }

    /// Elimina una sesión por ID
    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        let path = self.sessions_dir.join(format!("{}.json", session_id));
        if path.exists() {
            fs::remove_file(path)?;
            Ok(())
        } else {
            Err(anyhow!("Session not found: {}", session_id))
        }
    }

    /// Cierra la sesión activa (sin guardar)
    pub fn close_session(&mut self) {
        self.active_session = None;
    }
}

/// Información resumida de una sesión
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub message_count: usize,
    pub files_modified: usize,
}

impl SessionInfo {
    /// Genera una representación legible
    pub fn display(&self) -> String {
        let duration = self.updated_at
            .duration_since(self.created_at)
            .unwrap_or_default();
        let duration_mins = duration.as_secs() / 60;

        format!(
            "{} - '{}' ({} msgs, {} files, {}m)",
            &self.id[..8],
            self.name,
            self.message_count,
            self.files_modified,
            duration_mins
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_creation() {
        let session = Session::new("Test Session".to_string());
        assert_eq!(session.name, "Test Session");
        assert_eq!(session.messages.len(), 0);
        assert!(!session.id.is_empty());
    }

    #[test]
    fn test_session_add_message() {
        let mut session = Session::new("Test".to_string());
        
        session.add_message(SessionMessage::user("Hello".to_string()));
        session.add_message(SessionMessage::assistant("Hi!".to_string()));
        
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, "user");
        assert_eq!(session.messages[0].content, "Hello");
        assert_eq!(session.messages[1].role, "assistant");
        assert_eq!(session.messages[1].content, "Hi!");
    }

    #[test]
    fn test_session_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("test_session.json");
        
        // Crear y guardar sesión
        let mut session = Session::new("Save Test".to_string());
        session.add_message(SessionMessage::user("Test message".to_string()));
        session.context.last_task = "testing".to_string();
        
        session.save(&session_path).unwrap();
        assert!(session_path.exists());
        
        // Cargar sesión
        let loaded = Session::load(&session_path).unwrap();
        assert_eq!(loaded.name, "Save Test");
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "Test message");
        assert_eq!(loaded.context.last_task, "testing");
    }

    #[test]
    fn test_session_context_capture() {
        let context = SessionContext::capture().unwrap();
        assert!(context.working_dir.exists());
        assert!(context.environment.contains_key("PATH") || context.environment.contains_key("HOME"));
    }

    #[test]
    fn test_session_manager_create() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SessionManager::new(temp_dir.path().to_path_buf());
        
        let session = manager.create_session("New Session".to_string()).unwrap();
        assert_eq!(session.name, "New Session");
        assert!(manager.active_session().is_some());
    }

    #[test]
    fn test_session_manager_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SessionManager::new(temp_dir.path().to_path_buf());
        
        // Crear y guardar
        let session = manager.create_session("Test".to_string()).unwrap();
        let session_id = session.id.clone();
        
        if let Some(active) = manager.active_session_mut() {
            active.add_message(SessionMessage::user("Hello".to_string()));
        }
        
        manager.save_active().unwrap();
        
        // Cerrar y cargar de nuevo
        manager.close_session();
        assert!(manager.active_session().is_none());
        
        manager.load_session(&session_id).unwrap();
        assert!(manager.active_session().is_some());
        assert_eq!(manager.active_session().unwrap().messages.len(), 1);
    }

    #[test]
    fn test_session_manager_list() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SessionManager::new(temp_dir.path().to_path_buf());
        
        // Crear y guardar 2 sesiones
        manager.create_session("Session 1".to_string()).unwrap();
        manager.save_active().unwrap();
        
        manager.create_session("Session 2".to_string()).unwrap();
        manager.save_active().unwrap();
        
        // Listar sesiones
        let sessions = manager.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        
        // Verificar que estén ordenadas por fecha (más reciente primero)
        assert_eq!(sessions[0].name, "Session 2");
        assert_eq!(sessions[1].name, "Session 1");
    }

    #[test]
    fn test_session_context_add_file() {
        let mut context = SessionContext::new();
        
        context.add_modified_file(PathBuf::from("file1.txt"));
        context.add_modified_file(PathBuf::from("file2.txt"));
        context.add_modified_file(PathBuf::from("file1.txt")); // Duplicado
        
        assert_eq!(context.files_modified.len(), 2); // No duplicados
    }

    #[test]
    fn test_session_summary() {
        let mut session = Session::new("Summary Test".to_string());
        session.add_message(SessionMessage::user("Msg 1".to_string()));
        session.add_message(SessionMessage::user("Msg 2".to_string()));
        session.context.add_modified_file(PathBuf::from("file.txt"));
        
        let summary = session.summary();
        assert!(summary.contains("Summary Test"));
        assert!(summary.contains("2 messages"));
        assert!(summary.contains("1 files"));
    }
}
