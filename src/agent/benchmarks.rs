//! Performance benchmarking system with regression detection
//!
//! This module provides a comprehensive benchmarking framework for tracking
//! performance metrics and detecting regressions across different operations.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use super::monitoring::{LatencyPercentiles, MetricsCollector};

/// Estado del benchmark en relación a la baseline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenchmarkStatus {
    /// Más rápido que la baseline (mejora)
    Faster,
    /// Dentro del threshold de la baseline (aceptable)
    Baseline,
    /// Más lento pero dentro de tolerancia (aceptable con warning)
    SlowerAcceptable,
    /// Regresión inaceptable (falla)
    Regression,
}

impl std::fmt::Display for BenchmarkStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Faster => write!(f, "✅ Faster"),
            Self::Baseline => write!(f, "✓ Baseline"),
            Self::SlowerAcceptable => write!(f, "⚠ SlowerAcceptable"),
            Self::Regression => write!(f, "❌ Regression"),
        }
    }
}

/// Baseline de rendimiento para una operación
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkBaseline {
    /// Nombre de la operación
    pub operation: String,
    /// Latencia objetivo (ms)
    pub target_ms: u64,
    /// Percentil 50 baseline (ms)
    pub p50_ms: u64,
    /// Percentil 95 baseline (ms)
    pub p95_ms: u64,
    /// Percentil 99 baseline (ms)
    pub p99_ms: u64,
    /// Threshold de regresión (porcentaje, ej: 20.0 = 20%)
    pub regression_threshold_percent: f64,
}

impl BenchmarkBaseline {
    /// Crear nueva baseline
    pub fn new(
        operation: String,
        target_ms: u64,
        p50_ms: u64,
        p95_ms: u64,
        p99_ms: u64,
    ) -> Self {
        Self {
            operation,
            target_ms,
            p50_ms,
            p95_ms,
            p99_ms,
            regression_threshold_percent: 20.0, // Default: 20% slower is regression
        }
    }

    /// Establecer threshold de regresión personalizado
    pub fn with_threshold(mut self, threshold_percent: f64) -> Self {
        self.regression_threshold_percent = threshold_percent;
        self
    }
}

/// Resultado de un benchmark
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkResult {
    /// Nombre de la operación
    pub operation: String,
    /// Percentiles actuales
    pub current: LatencyPercentiles,
    /// Baseline de referencia
    pub baseline: BenchmarkBaseline,
    /// Porcentaje de cambio respecto a baseline (negativo = mejora)
    pub regression_percent: f64,
    /// Estado del benchmark
    pub status: BenchmarkStatus,
    /// Timestamp del benchmark
    #[serde(skip)]
    pub timestamp: Instant,
}

impl BenchmarkResult {
    /// Crear resultado comparando con baseline
    pub fn new(operation: String, current: LatencyPercentiles, baseline: BenchmarkBaseline) -> Self {
        // Calcular regresión basada en p50 (mediana)
        let regression_percent = if baseline.p50_ms > 0 {
            ((current.p50 as f64 - baseline.p50_ms as f64) / baseline.p50_ms as f64) * 100.0
        } else {
            0.0
        };

        // Determinar estado
        let status = if regression_percent < 0.0 {
            // Mejora (más rápido)
            BenchmarkStatus::Faster
        } else if regression_percent <= 5.0 {
            // Dentro de 5% es baseline
            BenchmarkStatus::Baseline
        } else if regression_percent <= baseline.regression_threshold_percent {
            // Entre 5% y threshold es aceptable con warning
            BenchmarkStatus::SlowerAcceptable
        } else {
            // Mayor a threshold es regresión
            BenchmarkStatus::Regression
        };

        Self {
            operation,
            current,
            baseline,
            regression_percent,
            status,
            timestamp: Instant::now(),
        }
    }

    /// Verificar si el benchmark pasó (no es regresión)
    pub fn passed(&self) -> bool {
        self.status != BenchmarkStatus::Regression
    }

    /// Formatear resultado como string legible
    pub fn format(&self) -> String {
        format!(
            "{} - {} | p50: {}ms (baseline: {}ms, {}{:.1}%) | p95: {}ms | p99: {}ms",
            self.status,
            self.operation,
            self.current.p50,
            self.baseline.p50_ms,
            if self.regression_percent >= 0.0 { "+" } else { "" },
            self.regression_percent,
            self.current.p95,
            self.current.p99,
        )
    }
}

/// Runner de benchmarks con baselines y detección de regresiones
pub struct BenchmarkRunner {
    /// Baselines registradas por operación
    baselines: HashMap<String, BenchmarkBaseline>,
    /// Collector de métricas
    metrics: Arc<MetricsCollector>,
    /// Resultados de benchmarks ejecutados
    results: Vec<BenchmarkResult>,
}

impl BenchmarkRunner {
    /// Crear nuevo runner
    pub fn new(metrics: Arc<MetricsCollector>) -> Self {
        Self {
            baselines: HashMap::new(),
            metrics,
            results: Vec::new(),
        }
    }

    /// Registrar baseline para una operación
    pub fn register_baseline(&mut self, baseline: BenchmarkBaseline) {
        self.baselines.insert(baseline.operation.clone(), baseline);
    }

    /// Ejecutar benchmark de una operación múltiples veces
    pub async fn benchmark<F, T>(&mut self, operation: &str, iterations: usize, mut func: F) -> Result<BenchmarkResult>
    where
        F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
        T: Send,
    {
        let mut latencies = Vec::with_capacity(iterations);

        // Ejecutar múltiples iteraciones
        for _ in 0..iterations {
            let start = Instant::now();
            
            // Ejecutar operación
            let result = func().await;
            
            let duration = start.elapsed();
            latencies.push(duration.as_millis() as u64);

            // Registrar en metrics si la operación fue exitosa
            if result.is_ok() {
                self.metrics.record_query(duration.as_millis() as u64);
            }
        }

        // Calcular percentiles
        latencies.sort_unstable();
        let current = LatencyPercentiles::from_sorted(&latencies);

        // Obtener baseline
        let baseline = self.baselines
            .get(operation)
            .cloned()
            .unwrap_or_else(|| {
                // Baseline por defecto si no existe
                BenchmarkBaseline::new(
                    operation.to_string(),
                    current.p95, // Target = p95 actual
                    current.p50,
                    current.p95,
                    current.p99,
                )
            });

        // Crear resultado
        let result = BenchmarkResult::new(operation.to_string(), current, baseline);
        self.results.push(result.clone());

        Ok(result)
    }

    /// Obtener todos los resultados
    pub fn results(&self) -> &[BenchmarkResult] {
        &self.results
    }

    /// Verificar si todos los benchmarks pasaron
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed())
    }

    /// Obtener resumen de resultados
    pub fn summary(&self) -> BenchmarkSummary {
        let total = self.results.len();
        let passed = self.results.iter().filter(|r| r.passed()).count();
        let regressions = self.results.iter().filter(|r| r.status == BenchmarkStatus::Regression).count();
        let improvements = self.results.iter().filter(|r| r.status == BenchmarkStatus::Faster).count();

        BenchmarkSummary {
            total,
            passed,
            regressions,
            improvements,
        }
    }

    /// Exportar resultados a CSV para tracking en CI
    pub fn export_csv(&self) -> String {
        let mut csv = String::from("operation,p50_current,p50_baseline,p95_current,p95_baseline,p99_current,p99_baseline,regression_percent,status\n");
        
        for result in &self.results {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{:.2},{:?}\n",
                result.operation,
                result.current.p50,
                result.baseline.p50_ms,
                result.current.p95,
                result.baseline.p95_ms,
                result.current.p99,
                result.baseline.p99_ms,
                result.regression_percent,
                result.status,
            ));
        }

        csv
    }

    /// Limpiar resultados
    pub fn clear_results(&mut self) {
        self.results.clear();
    }
}

/// Resumen de resultados de benchmarks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    /// Total de benchmarks ejecutados
    pub total: usize,
    /// Benchmarks que pasaron (no regresiones)
    pub passed: usize,
    /// Regresiones detectadas
    pub regressions: usize,
    /// Mejoras detectadas
    pub improvements: usize,
}

impl std::fmt::Display for BenchmarkSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Benchmarks: {}/{} passed | {} regressions | {} improvements",
            self.passed, self.total, self.regressions, self.improvements
        )
    }
}

/// Baselines predefinidas para operaciones comunes
pub mod presets {
    use super::BenchmarkBaseline;

    /// Baseline para clasificación de queries
    pub fn classification() -> BenchmarkBaseline {
        BenchmarkBaseline::new(
            "classification".to_string(),
            50,   // target: <50ms
            30,   // p50: 30ms
            45,   // p95: 45ms
            50,   // p99: 50ms
        )
    }

    /// Baseline para queries RAPTOR con preloader
    pub fn raptor_query() -> BenchmarkBaseline {
        BenchmarkBaseline::new(
            "raptor_query".to_string(),
            500,  // target: <500ms
            300,  // p50: 300ms
            450,  // p95: 450ms
            500,  // p99: 500ms
        )
    }

    /// Baseline para ejecución de tools
    pub fn tool_execution() -> BenchmarkBaseline {
        BenchmarkBaseline::new(
            "tool_execution".to_string(),
            200,  // target: <200ms
            100,  // p50: 100ms
            180,  // p95: 180ms
            200,  // p99: 200ms
        )
    }

    /// Baseline para operaciones de archivo
    pub fn file_operations() -> BenchmarkBaseline {
        BenchmarkBaseline::new(
            "file_operations".to_string(),
            10,   // target: <10ms
            5,    // p50: 5ms
            8,    // p95: 8ms
            10,   // p99: 10ms
        )
    }

    /// Baseline para cache lookup
    pub fn cache_lookup() -> BenchmarkBaseline {
        BenchmarkBaseline::new(
            "cache_lookup".to_string(),
            1,    // target: <1ms
            0,    // p50: <1ms
            1,    // p95: 1ms
            1,    // p99: 1ms
        ).with_threshold(50.0) // Cache permite mayor threshold (50%)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baseline_creation() {
        let baseline = BenchmarkBaseline::new(
            "test_op".to_string(),
            100,
            50,
            90,
            100,
        );

        assert_eq!(baseline.operation, "test_op");
        assert_eq!(baseline.target_ms, 100);
        assert_eq!(baseline.p50_ms, 50);
        assert_eq!(baseline.regression_threshold_percent, 20.0);
    }

    #[test]
    fn test_baseline_custom_threshold() {
        let baseline = BenchmarkBaseline::new(
            "test_op".to_string(),
            100, 50, 90, 100,
        ).with_threshold(15.0);

        assert_eq!(baseline.regression_threshold_percent, 15.0);
    }

    #[test]
    fn test_benchmark_status_faster() {
        let baseline = BenchmarkBaseline::new("test".to_string(), 100, 50, 90, 100);
        let current = LatencyPercentiles {
            p50: 40,  // Más rápido
            p95: 80,
            p99: 95,
            count: 100,
        };

        let result = BenchmarkResult::new("test".to_string(), current, baseline);
        
        assert_eq!(result.status, BenchmarkStatus::Faster);
        assert!(result.regression_percent < 0.0);
        assert!(result.passed());
    }

    #[test]
    fn test_benchmark_status_baseline() {
        let baseline = BenchmarkBaseline::new("test".to_string(), 100, 50, 90, 100);
        let current = LatencyPercentiles {
            p50: 52,  // +4% (dentro de 5%)
            p95: 93,
            p99: 102,
            count: 100,
        };

        let result = BenchmarkResult::new("test".to_string(), current, baseline);
        
        assert_eq!(result.status, BenchmarkStatus::Baseline);
        assert!(result.passed());
    }

    #[test]
    fn test_benchmark_status_slower_acceptable() {
        let baseline = BenchmarkBaseline::new("test".to_string(), 100, 50, 90, 100);
        let current = LatencyPercentiles {
            p50: 58,  // +16% (entre 5% y 20%)
            p95: 105,
            p99: 115,
            count: 100,
        };

        let result = BenchmarkResult::new("test".to_string(), current, baseline);
        
        assert_eq!(result.status, BenchmarkStatus::SlowerAcceptable);
        assert!(result.passed());
    }

    #[test]
    fn test_benchmark_status_regression() {
        let baseline = BenchmarkBaseline::new("test".to_string(), 100, 50, 90, 100);
        let current = LatencyPercentiles {
            p50: 65,  // +30% (mayor a 20% threshold)
            p95: 120,
            p99: 140,
            count: 100,
        };

        let result = BenchmarkResult::new("test".to_string(), current, baseline);
        
        assert_eq!(result.status, BenchmarkStatus::Regression);
        assert!(!result.passed());
    }

    #[tokio::test]
    async fn test_benchmark_runner() {
        let metrics = Arc::new(MetricsCollector::new());
        let mut runner = BenchmarkRunner::new(metrics);

        // Registrar baseline
        let baseline = BenchmarkBaseline::new("fast_op".to_string(), 100, 50, 90, 100);
        runner.register_baseline(baseline);

        // Ejecutar benchmark (operación que toma ~10ms)
        let result = runner.benchmark("fast_op", 10, || {
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(())
            })
        }).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.current.p50 >= 10);
        assert!(result.current.p50 <= 50); // Dentro de baseline
    }

    #[tokio::test]
    async fn test_benchmark_runner_summary() {
        let metrics = Arc::new(MetricsCollector::new());
        let mut runner = BenchmarkRunner::new(metrics);

        // Baseline alta (100ms)
        let baseline = BenchmarkBaseline::new("test_op".to_string(), 100, 50, 90, 100);
        runner.register_baseline(baseline);

        // Benchmark 1: rápido (dentro de baseline)
        let _ = runner.benchmark("test_op", 5, || {
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(30)).await;
                Ok(())
            })
        }).await;

        let summary = runner.summary();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.passed, 1);
    }

    #[test]
    fn test_csv_export() {
        let metrics = Arc::new(MetricsCollector::new());
        let mut runner = BenchmarkRunner::new(metrics);

        let baseline = BenchmarkBaseline::new("test".to_string(), 100, 50, 90, 100);
        let current = LatencyPercentiles { p50: 45, p95: 85, p99: 95, count: 100 };
        let result = BenchmarkResult::new("test".to_string(), current, baseline);
        
        runner.results.push(result);

        let csv = runner.export_csv();
        assert!(csv.contains("operation,p50_current"));
        assert!(csv.contains("test,45,50"));
    }

    #[test]
    fn test_presets() {
        let classification = presets::classification();
        assert_eq!(classification.target_ms, 50);
        assert_eq!(classification.p50_ms, 30);

        let raptor = presets::raptor_query();
        assert_eq!(raptor.target_ms, 500);

        let tools = presets::tool_execution();
        assert_eq!(tools.target_ms, 200);

        let files = presets::file_operations();
        assert_eq!(files.target_ms, 10);

        let cache = presets::cache_lookup();
        assert_eq!(cache.target_ms, 1);
        assert_eq!(cache.regression_threshold_percent, 50.0);
    }
}
