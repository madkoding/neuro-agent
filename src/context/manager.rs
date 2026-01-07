//! Context Manager - Smart context window management

use anyhow::Result;

pub struct ContextManager {
    // Placeholder for database and semantic search
    // In real implementation, add: Arc<Database>, Arc<SemanticSearch>
}

pub struct LLMContext {
    sections: Vec<ContextSection>,
    total_tokens: usize,
    max_tokens: usize,
}

struct ContextSection {
    content: String,
    tokens: usize,
    priority: Priority,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Priority {
    Low = 1,
    Medium = 2,
    High = 3,
}

impl ContextManager {
    pub fn new() -> Self {
        Self {}
    }

    /// Build optimal context for a query
    pub async fn build_context(
        &self,
        _project_id: &str,
        query: &str,
        max_tokens: usize,
    ) -> Result<LLMContext> {
        let mut ctx = LLMContext::new(max_tokens);

        // Add basic context with the query
        ctx.add(
            "user_query",
            format!("User query: {}", query),
            Priority::High,
        );

        // In a full implementation, this would:
        // 1. Get cached project summary
        // 2. Perform semantic search for relevant code
        // 3. Add recent files if space available
        // 4. Add dependency info if relevant

        ctx.optimize();
        Ok(ctx)
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LLMContext {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            sections: Vec::new(),
            total_tokens: 0,
            max_tokens,
        }
    }

    pub fn add(&mut self, _key: impl Into<String>, content: impl Into<String>, priority: Priority) {
        let content = content.into();
        let tokens = estimate_tokens(&content);

        self.sections.push(ContextSection {
            content,
            tokens,
            priority,
        });

        self.total_tokens += tokens;
    }

    pub fn has_space(&self, tokens: usize) -> bool {
        self.total_tokens + tokens <= self.max_tokens
    }

    pub fn optimize(&mut self) {
        if self.total_tokens <= self.max_tokens {
            return;
        }

        // Sort by priority (low first)
        self.sections.sort_by_key(|s| s.priority);

        // Remove low-priority sections until we fit
        while self.total_tokens > self.max_tokens && !self.sections.is_empty() {
            if let Some(removed) = self.sections.first() {
                if removed.priority == Priority::Low {
                    let tokens = removed.tokens;
                    self.sections.remove(0);
                    self.total_tokens -= tokens;
                } else {
                    break;
                }
            }
        }

        // If still too large, truncate medium priority
        if self.total_tokens > self.max_tokens {
            let mut i = 0;
            while i < self.sections.len() && self.total_tokens > self.max_tokens {
                if self.sections[i].priority == Priority::Medium {
                    let tokens = self.sections[i].tokens;
                    self.sections.remove(i);
                    self.total_tokens -= tokens;
                } else {
                    i += 1;
                }
            }
        }
    }

    pub fn to_string(&self) -> String {
        self.sections
            .iter()
            .map(|s| &s.content)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")
    }

    pub fn token_count(&self) -> usize {
        self.total_tokens
    }
}

/// Estimate tokens (rough approximation: 1 token ≈ 4 characters)
fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}
