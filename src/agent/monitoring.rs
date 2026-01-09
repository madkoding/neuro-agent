//! Production Monitoring System
//!
//! Framework de monitoreo para observability en producción:
//! - Structured logging con niveles configurables
//! - Métricas con contadores atómicos (thread-safe)
//! - Error tracking por tipo
//! - Distribución de latencias (p50, p95, p99)
//! - Exportación a JSON/CSV

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Nivel de logging
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// Formato de output del logger
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Texto plano legible
    Plain,
    /// JSON estructurado
    Json,
    /// JSON pretty-printed
    JsonPretty,
}

/// Evento de log estructurado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Timestamp del evento
    pub timestamp: String,
    /// Nivel de log
    pub level: LogLevel,
    /// Mensaje principal
    pub message: String,
    /// Campos adicionales (key-value)
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, String>,
}

/// Logger estructurado
pub struct StructuredLogger {
    /// Nivel mínimo de logging
    level: LogLevel,
    /// Formato de output
    format: LogFormat,
    /// Eventos almacenados en memoria (para testing/export)
    events: Arc<Mutex<Vec<LogEvent>>>,
}

impl StructuredLogger {
    /// Crea un nuevo logger
    pub fn new(level: LogLevel, format: LogFormat) -> Self {
        Self {
            level,
            format,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Log a debug level
    pub fn debug(&self, message: &str, fields: HashMap<String, String>) {
        self.log(LogLevel::Debug, message, fields);
    }

    /// Log a info level
    pub fn info(&self, message: &str, fields: HashMap<String, String>) {
        self.log(LogLevel::Info, message, fields);
    }

    /// Log a warn level
    pub fn warn(&self, message: &str, fields: HashMap<String, String>) {
        self.log(LogLevel::Warn, message, fields);
    }

    /// Log a error level
    pub fn error(&self, message: &str, fields: HashMap<String, String>) {
        self.log(LogLevel::Error, message, fields);
    }

    /// Log general con nivel especificado
    fn log(&self, level: LogLevel, message: &str, fields: HashMap<String, String>) {
        // Filtrar por nivel
        if !self.should_log(level) {
            return;
        }

        let event = LogEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            message: message.to_string(),
            fields,
        };

        // Almacenar en memoria
        if let Ok(mut events) = self.events.lock() {
            events.push(event.clone());
        }

        // Output según formato
        match self.format {
            LogFormat::Plain => {
                let fields_str = if event.fields.is_empty() {
                    String::new()
                } else {
                    let pairs: Vec<String> = event.fields.iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect();
                    format!(" [{}]", pairs.join(", "))
                };
                eprintln!("[{}] {}: {}{}", event.timestamp, event.level, event.message, fields_str);
            }
            LogFormat::Json => {
                if let Ok(json) = serde_json::to_string(&event) {
                    eprintln!("{}", json);
                }
            }
            LogFormat::JsonPretty => {
                if let Ok(json) = serde_json::to_string_pretty(&event) {
                    eprintln!("{}", json);
                }
            }
        }
    }

    /// Verifica si debe loggear un nivel
    fn should_log(&self, level: LogLevel) -> bool {
        let level_value = match level {
            LogLevel::Debug => 0,
            LogLevel::Info => 1,
            LogLevel::Warn => 2,
            LogLevel::Error => 3,
        };
        let min_level_value = match self.level {
            LogLevel::Debug => 0,
            LogLevel::Info => 1,
            LogLevel::Warn => 2,
            LogLevel::Error => 3,
        };
        level_value >= min_level_value
    }

    /// Obtiene todos los eventos almacenados
    pub fn get_events(&self) -> Vec<LogEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Limpia los eventos almacenados
    pub fn clear_events(&self) {
        self.events.lock().unwrap().clear();
    }
}

/// Colector de métricas con contadores atómicos
pub struct MetricsCollector {
    /// Cache hits
    cache_hits: AtomicUsize,
    /// Cache misses
    cache_misses: AtomicUsize,
    /// Total de queries procesadas
    total_queries: AtomicUsize,
    /// Suma de latencias (para calcular average)
    total_latency_ms: AtomicU64,
    /// Errores por tipo
    errors_by_type: Arc<Mutex<HashMap<String, AtomicUsize>>>,
    /// Distribución de latencias
    latency_samples: Arc<Mutex<Vec<u64>>>,
}

impl MetricsCollector {
    /// Crea un nuevo colector de métricas
    pub fn new() -> Self {
        Self {
            cache_hits: AtomicUsize::new(0),
            cache_misses: AtomicUsize::new(0),
            total_queries: AtomicUsize::new(0),
            total_latency_ms: AtomicU64::new(0),
            errors_by_type: Arc::new(Mutex::new(HashMap::new())),
            latency_samples: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Registra un cache hit
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Registra un cache miss
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Registra una query procesada con su latencia
    pub fn record_query(&self, latency_ms: u64) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms.fetch_add(latency_ms, Ordering::Relaxed);
        
        // Almacenar sample para distribución
        if let Ok(mut samples) = self.latency_samples.lock() {
            samples.push(latency_ms);
        }
    }

    /// Registra un error por tipo
    pub fn record_error(&self, error_type: &str) {
        if let Ok(mut errors) = self.errors_by_type.lock() {
            errors.entry(error_type.to_string())
                .or_insert_with(|| AtomicUsize::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Obtiene el cache hit rate (0.0 - 1.0)
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Obtiene la latencia promedio en ms
    pub fn avg_latency_ms(&self) -> f64 {
        let total_queries = self.total_queries.load(Ordering::Relaxed);
        let total_latency = self.total_latency_ms.load(Ordering::Relaxed);
        
        if total_queries == 0 {
            0.0
        } else {
            total_latency as f64 / total_queries as f64
        }
    }

    /// Obtiene los percentiles de latencia (p50, p95, p99)
    pub fn latency_percentiles(&self) -> LatencyPercentiles {
        let mut samples = self.latency_samples.lock().unwrap().clone();
        
        if samples.is_empty() {
            return LatencyPercentiles {
                p50: 0,
                p95: 0,
                p99: 0,
                count: 0,
            };
        }
        
        samples.sort_unstable();
        let count = samples.len();
        
        let p50_idx = (count as f64 * 0.50) as usize;
        let p95_idx = (count as f64 * 0.95) as usize;
        let p99_idx = (count as f64 * 0.99) as usize;
        
        LatencyPercentiles {
            p50: samples[p50_idx.min(count - 1)],
            p95: samples[p95_idx.min(count - 1)],
            p99: samples[p99_idx.min(count - 1)],
            count,
        }
    }

    /// Obtiene un snapshot de las métricas actuales
    pub fn snapshot(&self) -> MetricsSnapshot {
        let errors_map = {
            let errors = self.errors_by_type.lock().unwrap();
            errors.iter()
                .map(|(k, v)| (k.clone(), v.load(Ordering::Relaxed)))
                .collect()
        };

        MetricsSnapshot {
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
            cache_hit_rate: self.cache_hit_rate(),
            total_queries: self.total_queries.load(Ordering::Relaxed),
            avg_latency_ms: self.avg_latency_ms(),
            latency_percentiles: self.latency_percentiles(),
            errors_by_type: errors_map,
        }
    }

    /// Resetea todas las métricas
    pub fn reset(&self) {
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.total_queries.store(0, Ordering::Relaxed);
        self.total_latency_ms.store(0, Ordering::Relaxed);
        
        if let Ok(mut errors) = self.errors_by_type.lock() {
            errors.clear();
        }
        
        if let Ok(mut samples) = self.latency_samples.lock() {
            samples.clear();
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Percentiles de latencia
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    /// Percentil 50 (mediana) en ms
    pub p50: u64,
    /// Percentil 95 en ms
    pub p95: u64,
    /// Percentil 99 en ms
    pub p99: u64,
    /// Número de samples
    pub count: usize,
}

/// Snapshot de métricas en un momento dado
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Cache hits
    pub cache_hits: usize,
    /// Cache misses
    pub cache_misses: usize,
    /// Cache hit rate (0.0 - 1.0)
    pub cache_hit_rate: f64,
    /// Total queries procesadas
    pub total_queries: usize,
    /// Latencia promedio en ms
    pub avg_latency_ms: f64,
    /// Percentiles de latencia
    pub latency_percentiles: LatencyPercentiles,
    /// Errores por tipo
    pub errors_by_type: HashMap<String, usize>,
}

impl MetricsSnapshot {
    /// Genera un reporte legible
    pub fn report(&self) -> String {
        let mut lines = vec![
            "📊 Metrics Snapshot".to_string(),
            format!("  Cache: {}/{} hits ({:.1}% rate)", 
                self.cache_hits, 
                self.cache_hits + self.cache_misses, 
                self.cache_hit_rate * 100.0),
            format!("  Queries: {} total, {:.1}ms avg latency", 
                self.total_queries, 
                self.avg_latency_ms),
            format!("  Latency: p50={:.0}ms, p95={:.0}ms, p99={:.0}ms ({} samples)",
                self.latency_percentiles.p50,
                self.latency_percentiles.p95,
                self.latency_percentiles.p99,
                self.latency_percentiles.count),
        ];

        if !self.errors_by_type.is_empty() {
            lines.push("  Errors:".to_string());
            let mut sorted_errors: Vec<_> = self.errors_by_type.iter().collect();
            sorted_errors.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count desc
            for (error_type, count) in sorted_errors.iter().take(5) {
                lines.push(format!("    - {}: {} occurrences", error_type, count));
            }
        }

        lines.join("\n")
    }

    /// Exporta a JSON
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Exporta a CSV (formato simplificado)
    pub fn to_csv(&self) -> String {
        let mut lines = vec![
            "metric,value".to_string(),
            format!("cache_hits,{}", self.cache_hits),
            format!("cache_misses,{}", self.cache_misses),
            format!("cache_hit_rate,{:.4}", self.cache_hit_rate),
            format!("total_queries,{}", self.total_queries),
            format!("avg_latency_ms,{:.2}", self.avg_latency_ms),
            format!("p50_latency_ms,{}", self.latency_percentiles.p50),
            format!("p95_latency_ms,{}", self.latency_percentiles.p95),
            format!("p99_latency_ms,{}", self.latency_percentiles.p99),
        ];

        // Agregar errores
        for (error_type, count) in &self.errors_by_type {
            lines.push(format!("error_{},{}", error_type.replace(',', "_"), count));
        }

        lines.join("\n")
    }
}

/// Sistema de monitoreo completo
pub struct MonitoringSystem {
    /// Logger estructurado
    logger: Arc<StructuredLogger>,
    /// Colector de métricas
    metrics: Arc<MetricsCollector>,
}

impl MonitoringSystem {
    /// Crea un nuevo sistema de monitoreo
    pub fn new(log_level: LogLevel, log_format: LogFormat) -> Self {
        Self {
            logger: Arc::new(StructuredLogger::new(log_level, log_format)),
            metrics: Arc::new(MetricsCollector::new()),
        }
    }

    /// Logger por defecto (Info level, Plain format)
    pub fn default() -> Self {
        Self::new(LogLevel::Info, LogFormat::Plain)
    }

    /// Obtiene el logger
    pub fn logger(&self) -> Arc<StructuredLogger> {
        self.logger.clone()
    }

    /// Obtiene el colector de métricas
    pub fn metrics(&self) -> Arc<MetricsCollector> {
        self.metrics.clone()
    }

    /// Mide la duración de una operación
    pub async fn measure<F, T>(&self, operation_name: &str, f: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        let start = Instant::now();
        
        let result = f.await;
        
        let duration_ms = start.elapsed().as_millis() as u64;
        
        match &result {
            Ok(_) => {
                self.logger.info(operation_name, {
                    let mut fields = HashMap::new();
                    fields.insert("duration_ms".to_string(), duration_ms.to_string());
                    fields.insert("status".to_string(), "success".to_string());
                    fields
                });
                self.metrics.record_query(duration_ms);
            }
            Err(e) => {
                self.logger.error(operation_name, {
                    let mut fields = HashMap::new();
                    fields.insert("duration_ms".to_string(), duration_ms.to_string());
                    fields.insert("error".to_string(), e.to_string());
                    fields
                });
                self.metrics.record_error(&format!("{:?}", e));
            }
        }
        
        result
    }

    /// Genera un reporte completo del estado actual
    pub fn report(&self) -> String {
        let snapshot = self.metrics.snapshot();
        snapshot.report()
    }

    /// Exporta métricas a JSON
    pub fn export_json(&self) -> Result<String> {
        let snapshot = self.metrics.snapshot();
        snapshot.to_json()
    }

    /// Exporta métricas a CSV
    pub fn export_csv(&self) -> String {
        let snapshot = self.metrics.snapshot();
        snapshot.to_csv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_structured_logging() {
        let logger = StructuredLogger::new(LogLevel::Info, LogFormat::Plain);
        
        // Log varios niveles
        logger.debug("Debug message", HashMap::new()); // No debe aparecer (level > debug)
        logger.info("Info message", HashMap::new());
        logger.warn("Warn message", HashMap::new());
        logger.error("Error message", {
            let mut fields = HashMap::new();
            fields.insert("code".to_string(), "500".to_string());
            fields
        });
        
        // Verificar que se almacenaron los eventos (excepto debug)
        let events = logger.get_events();
        assert_eq!(events.len(), 3); // Info, Warn, Error
        assert_eq!(events[0].level, LogLevel::Info);
        assert_eq!(events[1].level, LogLevel::Warn);
        assert_eq!(events[2].level, LogLevel::Error);
        assert_eq!(events[2].fields.get("code").unwrap(), "500");
    }

    #[test]
    fn test_metrics_collection() {
        let metrics = MetricsCollector::new();
        
        // Registrar métricas
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();
        metrics.record_query(100);
        metrics.record_query(200);
        metrics.record_query(150);
        
        // Verificar
        assert_eq!(metrics.cache_hits.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.cache_misses.load(Ordering::Relaxed), 1);
        assert!((metrics.cache_hit_rate() - 0.6667).abs() < 0.001); // 2/3 = 0.6667
        assert_eq!(metrics.total_queries.load(Ordering::Relaxed), 3);
        assert_eq!(metrics.avg_latency_ms(), 150.0); // (100+200+150)/3
    }

    #[test]
    fn test_cache_hit_rate_calculation() {
        let metrics = MetricsCollector::new();
        
        // Caso 1: Sin datos
        assert_eq!(metrics.cache_hit_rate(), 0.0);
        
        // Caso 2: Solo hits
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        assert_eq!(metrics.cache_hit_rate(), 1.0);
        
        // Caso 3: Mix
        metrics.record_cache_miss();
        metrics.record_cache_miss();
        assert_eq!(metrics.cache_hit_rate(), 0.5); // 2 hits, 2 misses
    }

    #[test]
    fn test_latency_tracking() {
        let metrics = MetricsCollector::new();
        
        // Registrar latencias variadas
        metrics.record_query(50);
        metrics.record_query(100);
        metrics.record_query(150);
        metrics.record_query(200);
        metrics.record_query(500);
        metrics.record_query(1000);
        
        let percentiles = metrics.latency_percentiles();
        
        assert_eq!(percentiles.count, 6);
        // p50 con 6 elementos es el índice 3 (cuarto elemento después de ordenar)
        // [50, 100, 150, 200, 500, 1000] -> p50 = 200
        assert!(percentiles.p50 >= 150 && percentiles.p50 <= 200); // Mediana
        assert!(percentiles.p95 >= 500);  // p95 debería estar en el rango alto
        assert!(percentiles.p99 >= 500);  // p99 también
    }

    #[test]
    fn test_error_tracking() {
        let metrics = MetricsCollector::new();
        
        // Registrar diferentes tipos de error
        metrics.record_error("NetworkError");
        metrics.record_error("NetworkError");
        metrics.record_error("TimeoutError");
        metrics.record_error("ParseError");
        
        let snapshot = metrics.snapshot();
        
        assert_eq!(snapshot.errors_by_type.get("NetworkError"), Some(&2));
        assert_eq!(snapshot.errors_by_type.get("TimeoutError"), Some(&1));
        assert_eq!(snapshot.errors_by_type.get("ParseError"), Some(&1));
    }

    #[test]
    fn test_json_export() {
        let metrics = MetricsCollector::new();
        
        metrics.record_cache_hit();
        metrics.record_query(100);
        metrics.record_error("TestError");
        
        let snapshot = metrics.snapshot();
        let json = snapshot.to_json().unwrap();
        
        // Verificar que es JSON válido
        assert!(json.contains("cache_hits"));
        assert!(json.contains("total_queries"));
        assert!(json.contains("TestError"));
    }

    #[test]
    fn test_csv_export() {
        let metrics = MetricsCollector::new();
        
        metrics.record_cache_hit();
        metrics.record_cache_miss();
        metrics.record_query(100);
        
        let snapshot = metrics.snapshot();
        let csv = snapshot.to_csv();
        
        // Verificar formato CSV
        assert!(csv.contains("metric,value"));
        assert!(csv.contains("cache_hits,1"));
        assert!(csv.contains("cache_misses,1"));
        assert!(csv.contains("avg_latency_ms"));
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = MetricsCollector::new();
        
        // Registrar datos
        metrics.record_cache_hit();
        metrics.record_query(100);
        metrics.record_error("TestError");
        
        // Verificar que hay datos
        assert!(metrics.total_queries.load(Ordering::Relaxed) > 0);
        
        // Resetear
        metrics.reset();
        
        // Verificar que todo está en 0
        assert_eq!(metrics.cache_hits.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.total_queries.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.snapshot().errors_by_type.len(), 0);
    }
}
