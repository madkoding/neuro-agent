//! Classification cache for faster query routing
//!
//! Caches RouterDecision results to avoid re-classifying similar queries.
//! Uses LRU eviction strategy and fuzzy matching for similarity detection.

use super::router_orchestrator::RouterDecision;
use lru::LruCache;
use std::num::NonZeroUsize;

/// Similarity threshold for fuzzy matching (0.0-1.0)
const SIMILARITY_THRESHOLD: f64 = 0.85;

/// Maximum cache entries
const CACHE_SIZE: usize = 100;

/// Cache for classification decisions
pub struct ClassificationCache {
    cache: LruCache<String, CachedDecision>,
}

/// Cached decision with metadata
#[derive(Clone, Debug)]
struct CachedDecision {
    decision: RouterDecision,
}

impl ClassificationCache {
    /// Create a new classification cache
    pub fn new() -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(CACHE_SIZE).unwrap()),
        }
    }

    /// Get cached decision for a query
    pub fn get(&mut self, query: &str) -> Option<RouterDecision> {
        let normalized = Self::normalize_query(query);
        
        // Exact match
        if let Some(cached) = self.cache.get(&normalized) {
            return Some(cached.decision.clone());
        }
        
        // Fuzzy match
        self.find_similar(&normalized)
    }

    /// Store a classification decision
    pub fn insert(&mut self, query: &str, decision: RouterDecision) {
        let normalized = Self::normalize_query(query);
        self.cache.put(
            normalized,
            CachedDecision { decision },
        );
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            size: self.cache.len(),
            capacity: self.cache.cap().get(),
        }
    }

    /// Normalize query for comparison
    fn normalize_query(query: &str) -> String {
        query
            .to_lowercase()
            .trim()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Find similar cached query using fuzzy matching
    fn find_similar(&mut self, query: &str) -> Option<RouterDecision> {
        let mut best_match: Option<(f64, RouterDecision)> = None;

        // Iterate through cache to find similar queries
        for (cached_query, cached_decision) in self.cache.iter() {
            let similarity = Self::calculate_similarity(query, cached_query);
            
            if similarity >= SIMILARITY_THRESHOLD {
                if let Some((best_score, _)) = best_match {
                    if similarity > best_score {
                        best_match = Some((similarity, cached_decision.decision.clone()));
                    }
                } else {
                    best_match = Some((similarity, cached_decision.decision.clone()));
                }
            }
        }

        best_match.map(|(_, decision)| decision)
    }

    /// Calculate similarity between two queries (Jaccard similarity)
    fn calculate_similarity(query1: &str, query2: &str) -> f64 {
        let words1: std::collections::HashSet<&str> = query1.split_whitespace().collect();
        let words2: std::collections::HashSet<&str> = query2.split_whitespace().collect();

        if words1.is_empty() && words2.is_empty() {
            return 1.0;
        }

        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }
}

impl Default for ClassificationCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub capacity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let mut cache = ClassificationCache::new();
        let decision = RouterDecision::DirectResponse {
            query: "test".to_string(),
            confidence: 0.9,
        };

        cache.insert("analyze main.rs", decision.clone());
        
        let result = cache.get("analyze main.rs");
        assert!(result.is_some());
    }

    #[test]
    fn test_fuzzy_match() {
        let mut cache = ClassificationCache::new();
        let decision = RouterDecision::DirectResponse {
            query: "test".to_string(),
            confidence: 0.9,
        };

        // Insert a query with 6 words
        cache.insert("please analyze the main rust file carefully", decision.clone());
        
        // Query with 5 of the same 6 words (removing "please")
        // Intersection: {analyze, the, main, rust, file, carefully} ∩ {analyze, the, main, rust, file, carefully} = 6
        // Wait, both are same words, so Jaccard = 1.0
        // Let me fix: cache has 7 words, query has 6 words (removing "please")
        // Cache words: {please, analyze, the, main, rust, file, carefully}
        // Query words: {analyze, the, main, rust, file, carefully}
        // Intersection: 6, Union: 7, Jaccard = 6/7 = 0.857 > 0.85 ✓
        let result = cache.get("analyze the main rust file carefully");
        assert!(result.is_some(), "Fuzzy match should work: J=6/7=0.857 > 0.85");
    }

    #[test]
    fn test_normalization() {
        let mut cache = ClassificationCache::new();
        let decision = RouterDecision::DirectResponse {
            query: "test".to_string(),
            confidence: 0.9,
        };

        cache.insert("  ANALYZE   Main.rs  ", decision.clone());
        
        // Normalized query should match
        let result = cache.get("analyze main.rs");
        assert!(result.is_some());
    }

    #[test]
    fn test_similarity_calculation() {
        // Test high similarity (all words match except one)
        let similarity = ClassificationCache::calculate_similarity(
            "analyze the main rust file",
            "analyze main rust file"
        );
        
        // Jaccard = 4/5 = 0.8 (below 0.85 threshold)
        assert!(similarity > 0.75 && similarity < 0.85);
        
        // Test very high similarity (only "the" differs)
        let similarity2 = ClassificationCache::calculate_similarity(
            "analyze main rust",
            "analyze main rust"
        );
        
        // Jaccard = 3/3 = 1.0 (exact match)
        assert_eq!(similarity2, 1.0);
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = ClassificationCache::new();
        let decision = RouterDecision::DirectResponse {
            query: "test".to_string(),
            confidence: 0.9,
        };

        cache.insert("query1", decision.clone());
        cache.insert("query2", decision.clone());

        let stats = cache.stats();
        assert_eq!(stats.size, 2);
        assert_eq!(stats.capacity, CACHE_SIZE);
    }
}
