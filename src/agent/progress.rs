//! Streaming progress tracker for real-time status updates
//!
//! Provides detailed progress information during long-running operations.

use tokio::sync::mpsc;

/// Progress stage during operation execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressStage {
    /// Classifying user query
    Classifying,
    /// Searching RAPTOR index
    SearchingContext { chunks: usize },
    /// Executing a tool
    ExecutingTool { tool_name: String },
    /// Generating response
    Generating,
    /// Completed successfully
    Complete,
    /// Failed with error
    Failed { error: String },
}

/// Progress update message
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub stage: ProgressStage,
    pub message: String,
    pub elapsed_ms: u64,
}

impl ProgressUpdate {
    /// Create a new progress update
    pub fn new(stage: ProgressStage, message: impl Into<String>, elapsed_ms: u64) -> Self {
        Self {
            stage,
            message: message.into(),
            elapsed_ms,
        }
    }
}

/// Progress tracker for streaming updates
pub struct ProgressTracker {
    tx: mpsc::Sender<ProgressUpdate>,
    start_time: std::time::Instant,
}

impl ProgressTracker {
    /// Create a new progress tracker
    pub fn new(tx: mpsc::Sender<ProgressUpdate>) -> Self {
        Self {
            tx,
            start_time: std::time::Instant::now(),
        }
    }

    /// Send a progress update
    pub async fn update(&self, stage: ProgressStage, message: impl Into<String>) {
        let elapsed = self.start_time.elapsed().as_millis() as u64;
        let update = ProgressUpdate::new(stage, message, elapsed);
        let _ = self.tx.send(update).await;
    }

    /// Send classifying stage
    pub async fn classifying(&self) {
        self.update(ProgressStage::Classifying, "🔍 Clasificando consulta...").await;
    }

    /// Send searching context stage
    pub async fn searching_context(&self, chunks: usize) {
        self.update(
            ProgressStage::SearchingContext { chunks },
            format!("📊 Buscando contexto ({} chunks)...", chunks),
        ).await;
    }

    /// Send executing tool stage
    pub async fn executing_tool(&self, tool_name: impl Into<String>) {
        let tool = tool_name.into();
        self.update(
            ProgressStage::ExecutingTool { tool_name: tool.clone() },
            format!("🔧 Ejecutando {}...", tool),
        ).await;
    }

    /// Send generating response stage
    pub async fn generating(&self) {
        self.update(ProgressStage::Generating, "💭 Generando respuesta...").await;
    }

    /// Send complete stage
    pub async fn complete(&self) {
        self.update(ProgressStage::Complete, "✓ Completado").await;
    }

    /// Send failed stage
    pub async fn failed(&self, error: impl Into<String>) {
        let err = error.into();
        self.update(
            ProgressStage::Failed { error: err.clone() },
            format!("❌ Error: {}", err),
        ).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_progress_tracker() {
        let (tx, mut rx) = mpsc::channel(10);
        let tracker = ProgressTracker::new(tx);

        tracker.classifying().await;
        let update = rx.recv().await.unwrap();
        assert_eq!(update.stage, ProgressStage::Classifying);
        assert!(update.message.contains("Clasificando"));

        tracker.searching_context(100).await;
        let update = rx.recv().await.unwrap();
        assert_eq!(update.stage, ProgressStage::SearchingContext { chunks: 100 });
        assert!(update.message.contains("100 chunks"));

        tracker.complete().await;
        let update = rx.recv().await.unwrap();
        assert_eq!(update.stage, ProgressStage::Complete);
    }
}
