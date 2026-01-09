//! Smart Error Recovery System
//!
//! Sistema de recuperación automática de errores con:
//! - Retry con backoff exponencial
//! - Rollback automático de operaciones de archivos
//! - Fallback a providers alternativos
//! - Simplificación de prompts en caso de complejidad
//! - Detección de patrones de error

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use super::monitoring::MetricsCollector;

/// Tipo de error detectado
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorType {
    /// Error de red/conexión
    Network,
    /// Timeout
    Timeout,
    /// Error de parsing/formato
    Parse,
    /// Prompt demasiado complejo
    ComplexityExceeded,
    /// Provider no disponible
    ProviderUnavailable,
    /// Error de I/O (archivos)
    IoError,
    /// Rate limit
    RateLimit,
    /// Otro error
    Other(String),
}

impl std::fmt::Display for ErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorType::Network => write!(f, "NetworkError"),
            ErrorType::Timeout => write!(f, "TimeoutError"),
            ErrorType::Parse => write!(f, "ParseError"),
            ErrorType::ComplexityExceeded => write!(f, "ComplexityExceeded"),
            ErrorType::ProviderUnavailable => write!(f, "ProviderUnavailable"),
            ErrorType::IoError => write!(f, "IoError"),
            ErrorType::RateLimit => write!(f, "RateLimit"),
            ErrorType::Other(s) => write!(f, "Other({})", s),
        }
    }
}

/// Estrategia de retry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryStrategy {
    /// Retry inmediato sin delay
    Immediate,
    /// Backoff exponencial con base en ms
    ExponentialBackoff { base_ms: u64 },
    /// Cambiar a provider alternativo
    AlternativeProvider,
    /// Simplificar el prompt
    SimplifiedPrompt,
}

/// Resultado de un intento de operación
#[derive(Debug)]
pub struct RetryAttempt<T> {
    /// Número de intento (1-indexed)
    pub attempt: usize,
    /// Duración del intento
    pub duration_ms: u64,
    /// Resultado del intento
    pub result: Result<T>,
}

/// Operación de rollback para deshacer cambios
#[derive(Debug, Clone)]
pub enum RollbackOperation {
    /// Restaurar un archivo
    RestoreFile {
        path: PathBuf,
        original_content: String,
    },
    /// Eliminar un archivo creado
    DeleteFile { path: PathBuf },
    /// No-op (sin rollback)
    None,
}

impl RollbackOperation {
    /// Ejecuta el rollback
    pub async fn execute(&self) -> Result<()> {
        match self {
            RollbackOperation::RestoreFile {
                path,
                original_content,
            } => {
                tokio::fs::write(path, original_content)
                    .await
                    .context("Failed to restore file")?;
                Ok(())
            }
            RollbackOperation::DeleteFile { path } => {
                tokio::fs::remove_file(path)
                    .await
                    .context("Failed to delete file")?;
                Ok(())
            }
            RollbackOperation::None => Ok(()),
        }
    }
}

/// Patrón de error detectado
#[derive(Debug, Clone)]
pub struct ErrorPattern {
    /// Tipo de error
    pub error_type: ErrorType,
    /// Número de ocurrencias consecutivas
    pub consecutive_count: usize,
    /// Última ocurrencia
    pub last_occurrence: std::time::Instant,
}

/// Sistema de recuperación de errores
pub struct ErrorRecovery {
    /// Número máximo de reintentos
    max_retries: usize,
    /// Estrategias de retry por tipo de error
    retry_strategies: HashMap<ErrorType, RetryStrategy>,
    /// Manejadores de rollback
    rollback_stack: Vec<RollbackOperation>,
    /// Patrones de error detectados
    error_patterns: HashMap<ErrorType, ErrorPattern>,
    /// Métricas (opcional)
    metrics: Option<Arc<MetricsCollector>>,
}

impl ErrorRecovery {
    /// Crea un nuevo sistema de recuperación
    pub fn new(max_retries: usize) -> Self {
        let mut retry_strategies = HashMap::new();
        
        // Estrategias por defecto
        retry_strategies.insert(ErrorType::Network, RetryStrategy::ExponentialBackoff { base_ms: 100 });
        retry_strategies.insert(ErrorType::Timeout, RetryStrategy::ExponentialBackoff { base_ms: 200 });
        retry_strategies.insert(ErrorType::RateLimit, RetryStrategy::ExponentialBackoff { base_ms: 1000 });
        retry_strategies.insert(ErrorType::Parse, RetryStrategy::Immediate);
        retry_strategies.insert(ErrorType::ComplexityExceeded, RetryStrategy::SimplifiedPrompt);
        retry_strategies.insert(ErrorType::ProviderUnavailable, RetryStrategy::AlternativeProvider);
        
        Self {
            max_retries,
            retry_strategies,
            rollback_stack: Vec::new(),
            error_patterns: HashMap::new(),
            metrics: None,
        }
    }

    /// Configuración por defecto (3 reintentos)
    pub fn default() -> Self {
        Self::new(3)
    }

    /// Configura las métricas
    pub fn with_metrics(mut self, metrics: Arc<MetricsCollector>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Registra una operación de rollback
    pub fn register_rollback(&mut self, operation: RollbackOperation) {
        self.rollback_stack.push(operation);
    }

    /// Ejecuta todos los rollbacks
    pub async fn rollback_all(&mut self) -> Result<()> {
        let mut errors = Vec::new();
        
        // Ejecutar rollbacks en orden inverso (LIFO)
        while let Some(operation) = self.rollback_stack.pop() {
            if let Err(e) = operation.execute().await {
                errors.push(e);
            }
        }
        
        if !errors.is_empty() {
            anyhow::bail!(
                "Rollback failed with {} errors: {:?}",
                errors.len(),
                errors
            );
        }
        
        Ok(())
    }

    /// Detecta el tipo de error a partir del mensaje
    pub fn detect_error_type(&self, error: &anyhow::Error) -> ErrorType {
        let error_str = error.to_string().to_lowercase();
        
        if error_str.contains("network") || error_str.contains("connection") {
            ErrorType::Network
        } else if error_str.contains("timeout") || error_str.contains("timed out") {
            ErrorType::Timeout
        } else if error_str.contains("parse") || error_str.contains("invalid") {
            ErrorType::Parse
        } else if error_str.contains("complexity") || error_str.contains("too complex") {
            ErrorType::ComplexityExceeded
        } else if error_str.contains("unavailable") || error_str.contains("not available") {
            ErrorType::ProviderUnavailable
        } else if error_str.contains("io") || error_str.contains("file") {
            ErrorType::IoError
        } else if error_str.contains("rate limit") || error_str.contains("too many requests") {
            ErrorType::RateLimit
        } else {
            ErrorType::Other(error_str)
        }
    }

    /// Actualiza los patrones de error
    fn update_error_pattern(&mut self, error_type: &ErrorType) {
        let pattern = self.error_patterns.entry(error_type.clone()).or_insert(ErrorPattern {
            error_type: error_type.clone(),
            consecutive_count: 0,
            last_occurrence: std::time::Instant::now(),
        });
        
        pattern.consecutive_count += 1;
        pattern.last_occurrence = std::time::Instant::now();
    }

    /// Resetea el contador de un patrón de error (después de éxito)
    fn reset_error_pattern(&mut self, error_type: &ErrorType) {
        if let Some(pattern) = self.error_patterns.get_mut(error_type) {
            pattern.consecutive_count = 0;
        }
    }

    /// Obtiene el patrón de error actual
    pub fn get_error_pattern(&self, error_type: &ErrorType) -> Option<&ErrorPattern> {
        self.error_patterns.get(error_type)
    }

    /// Calcula el delay para exponential backoff
    fn calculate_backoff_delay(&self, attempt: usize, base_ms: u64) -> Duration {
        let delay_ms = base_ms * 2_u64.pow((attempt - 1) as u32);
        Duration::from_millis(delay_ms.min(30_000)) // Max 30 segundos
    }

    /// Ejecuta una operación con retry automático
    pub async fn retry<F, T>(&mut self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>> + Send,
        T: Send,
    {
        let mut last_error = None;
        let mut last_error_type = None;

        for attempt in 1..=self.max_retries {
            let start = std::time::Instant::now();
            
            // Ejecutar operación
            let result = operation().await;
            
            let duration_ms = start.elapsed().as_millis() as u64;

            match result {
                Ok(value) => {
                    // Éxito: resetear patrones de error
                    if let Some(error_type) = last_error_type {
                        self.reset_error_pattern(&error_type);
                    }
                    
                    // Registrar métrica de éxito
                    if let Some(ref metrics) = self.metrics {
                        metrics.record_query(duration_ms);
                    }
                    
                    return Ok(value);
                }
                Err(e) => {
                    // Detectar tipo de error
                    let error_type = self.detect_error_type(&e);
                    
                    // Actualizar patrón
                    self.update_error_pattern(&error_type);
                    
                    // Registrar métrica de error
                    if let Some(ref metrics) = self.metrics {
                        metrics.record_error(&error_type.to_string());
                    }
                    
                    // Si es el último intento, fallar
                    if attempt >= self.max_retries {
                        last_error = Some(e);
                        break;
                    }
                    
                    // Obtener estrategia de retry
                    let strategy = self.retry_strategies
                        .get(&error_type)
                        .cloned()
                        .unwrap_or(RetryStrategy::Immediate);
                    
                    // Aplicar estrategia
                    match strategy {
                        RetryStrategy::Immediate => {
                            // Sin delay
                        }
                        RetryStrategy::ExponentialBackoff { base_ms } => {
                            let delay = self.calculate_backoff_delay(attempt, base_ms);
                            sleep(delay).await;
                        }
                        RetryStrategy::AlternativeProvider | RetryStrategy::SimplifiedPrompt => {
                            // Estas estrategias requieren modificar el input,
                            // por ahora solo hacemos un delay corto
                            sleep(Duration::from_millis(100)).await;
                        }
                    }
                    
                    last_error = Some(e);
                    last_error_type = Some(error_type);
                }
            }
        }

        // Todos los reintentos fallaron
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Operation failed after {} retries", self.max_retries)))
    }

    /// Ejecuta una operación de archivo con rollback automático
    pub async fn with_rollback<F, T>(&mut self, path: PathBuf, operation: F) -> Result<T>
    where
        F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>> + Send,
        T: Send,
    {
        // Leer contenido original si existe
        let original_content = if path.exists() {
            Some(tokio::fs::read_to_string(&path).await?)
        } else {
            None
        };

        // Registrar rollback
        if let Some(content) = original_content {
            self.register_rollback(RollbackOperation::RestoreFile {
                path: path.clone(),
                original_content: content,
            });
        } else {
            self.register_rollback(RollbackOperation::DeleteFile { path: path.clone() });
        }

        // Ejecutar operación
        let result = operation().await;

        match result {
            Ok(value) => {
                // Éxito: remover rollback de la pila
                self.rollback_stack.pop();
                Ok(value)
            }
            Err(e) => {
                // Error: ejecutar rollback
                self.rollback_all().await?;
                Err(e)
            }
        }
    }

    /// Simplifica un prompt complejo
    pub fn simplify_prompt(&self, prompt: &str) -> String {
        // Estrategias de simplificación:
        // 1. Limitar longitud
        let max_chars = 2000;
        let mut simplified = if prompt.len() > max_chars {
            prompt.chars().take(max_chars).collect()
        } else {
            prompt.to_string()
        };

        // 2. Remover ejemplos extensos
        if simplified.contains("Example:") {
            if let Some(idx) = simplified.find("Example:") {
                simplified.truncate(idx);
            }
        }

        // 3. Reducir contexto excesivo
        if simplified.contains("Context:") && simplified.contains("Question:") {
            // Mantener solo pregunta
            if let Some(idx) = simplified.find("Question:") {
                simplified = simplified[idx..].to_string();
            }
        }

        simplified.trim().to_string()
    }

    /// Obtiene estadísticas de recuperación
    pub fn stats(&self) -> RecoveryStats {
        RecoveryStats {
            total_patterns: self.error_patterns.len(),
            active_patterns: self.error_patterns.values()
                .filter(|p| p.consecutive_count > 0)
                .count(),
            rollback_stack_size: self.rollback_stack.len(),
        }
    }
}

/// Estadísticas de recuperación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStats {
    /// Total de patrones de error conocidos
    pub total_patterns: usize,
    /// Patrones activos (con errores recientes)
    pub active_patterns: usize,
    /// Tamaño de la pila de rollback
    pub rollback_stack_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_immediate_retry() {
        let mut recovery = ErrorRecovery::new(3);
        let mut attempt_count = 0;

        let result: Result<&str> = recovery.retry(|| {
            attempt_count += 1;
            Box::pin(async move {
                if attempt_count < 2 {
                    anyhow::bail!("Network error")
                } else {
                    Ok("Success")
                }
            })
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Success");
        assert_eq!(attempt_count, 2);
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let mut recovery = ErrorRecovery::new(3);
        
        // Test que el backoff se calcula correctamente
        let delay1 = recovery.calculate_backoff_delay(1, 100);
        let delay2 = recovery.calculate_backoff_delay(2, 100);
        let delay3 = recovery.calculate_backoff_delay(3, 100);
        
        assert_eq!(delay1.as_millis(), 100);   // 100 * 2^0
        assert_eq!(delay2.as_millis(), 200);   // 100 * 2^1
        assert_eq!(delay3.as_millis(), 400);   // 100 * 2^2
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        let mut recovery = ErrorRecovery::new(3);
        let mut attempt_count = 0;

        let result: Result<()> = recovery.retry(|| {
            attempt_count += 1;
            Box::pin(async move {
                anyhow::bail!("Persistent error")
            })
        }).await;

        assert!(result.is_err());
        assert_eq!(attempt_count, 3); // Max retries
    }

    #[tokio::test]
    async fn test_error_pattern_detection() {
        let mut recovery = ErrorRecovery::new(3);
        
        // Simular varios errores del mismo tipo
        let error_type = ErrorType::Network;
        recovery.update_error_pattern(&error_type);
        recovery.update_error_pattern(&error_type);
        recovery.update_error_pattern(&error_type);
        
        let pattern = recovery.get_error_pattern(&error_type).unwrap();
        assert_eq!(pattern.consecutive_count, 3);
    }

    #[tokio::test]
    async fn test_error_type_detection() {
        let recovery = ErrorRecovery::new(3);
        
        let network_error = anyhow::anyhow!("Network connection failed");
        assert_eq!(recovery.detect_error_type(&network_error), ErrorType::Network);
        
        let timeout_error = anyhow::anyhow!("Request timed out");
        assert_eq!(recovery.detect_error_type(&timeout_error), ErrorType::Timeout);
        
        let parse_error = anyhow::anyhow!("Failed to parse JSON");
        assert_eq!(recovery.detect_error_type(&parse_error), ErrorType::Parse);
    }

    #[tokio::test]
    async fn test_rollback_on_failure() {
        let mut recovery = ErrorRecovery::new(3);
        let temp_file = std::env::temp_dir().join("test_rollback.txt");
        
        // Crear archivo inicial
        tokio::fs::write(&temp_file, "original content").await.unwrap();
        
        // Operación que falla
        let result: Result<()> = recovery.with_rollback(temp_file.clone(), || {
            Box::pin(async move {
                anyhow::bail!("Operation failed")
            })
        }).await;
        
        assert!(result.is_err());
        
        // Verificar que el contenido fue restaurado
        let content = tokio::fs::read_to_string(&temp_file).await.unwrap();
        assert_eq!(content, "original content");
        
        // Cleanup
        let _ = tokio::fs::remove_file(&temp_file).await;
    }

    #[tokio::test]
    async fn test_rollback_delete_new_file() {
        let mut recovery = ErrorRecovery::new(3);
        let temp_file = std::env::temp_dir().join("test_rollback_new.txt");
        
        // Asegurar que no existe
        let _ = tokio::fs::remove_file(&temp_file).await;
        
        // Crear archivo y luego fallar
        tokio::fs::write(&temp_file, "new content").await.unwrap();
        
        recovery.register_rollback(RollbackOperation::DeleteFile { path: temp_file.clone() });
        
        // Ejecutar rollback
        recovery.rollback_all().await.unwrap();
        
        // Verificar que fue eliminado
        assert!(!temp_file.exists());
    }

    #[test]
    fn test_prompt_simplification() {
        let recovery = ErrorRecovery::new(3);
        
        // Test 1: Remover ejemplos
        let with_example = "Question: What is the answer? Example: Here's a detailed example.";
        let simplified = recovery.simplify_prompt(with_example);
        assert!(simplified.contains("Question:"));
        assert!(!simplified.contains("Example:"));
        
        // Test 2: Extraer solo pregunta cuando hay contexto
        let with_context = "Context: This is a very long context. Question: What is the answer?";
        let simplified = recovery.simplify_prompt(with_context);
        assert!(simplified.contains("Question:"));
        assert!(!simplified.contains("Context:"));
        
        // Test 3: Truncar prompts muy largos
        let long_prompt = "A".repeat(3000);
        let simplified = recovery.simplify_prompt(&long_prompt);
        assert!(simplified.len() <= 2000);
    }

    #[tokio::test]
    async fn test_metrics_integration() {
        let metrics = Arc::new(MetricsCollector::new());
        let mut recovery = ErrorRecovery::new(3).with_metrics(metrics.clone());
        
        let mut attempt_count = 0;
        let _ = recovery.retry(|| {
            attempt_count += 1;
            Box::pin(async move {
                if attempt_count < 2 {
                    anyhow::bail!("Network error")
                } else {
                    Ok("Success")
                }
            })
        }).await;
        
        // Verificar que se registró el error
        let snapshot = metrics.snapshot();
        assert!(snapshot.errors_by_type.contains_key("NetworkError"));
    }
}
