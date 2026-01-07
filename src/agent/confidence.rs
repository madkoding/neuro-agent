//! Confidence scoring for tool calls

use serde_json::Value;

pub struct ToolCallCandidate {
    pub tool_name: String,
    pub args: Value,
    pub confidence: f32, // 0.0 - 1.0
    pub method: ParseMethod,
}

#[derive(Debug, Clone, Copy)]
pub enum ParseMethod {
    JsonSchema,      // confidence: 0.95
    XmlParsing,      // confidence: 0.75
    PatternMatching, // confidence: 0.60
    NaturalLanguage, // confidence: 0.40
}

impl ToolCallCandidate {
    pub fn should_execute(&self) -> bool {
        self.confidence >= 0.7
    }

    pub fn with_method(tool_name: String, args: Value, method: ParseMethod) -> Self {
        let confidence = match method {
            ParseMethod::JsonSchema => 0.95,
            ParseMethod::XmlParsing => 0.75,
            ParseMethod::PatternMatching => 0.60,
            ParseMethod::NaturalLanguage => 0.40,
        };

        Self {
            tool_name,
            args,
            confidence,
            method,
        }
    }

    pub fn new(tool_name: String, args: Value, confidence: f32, method: ParseMethod) -> Self {
        Self {
            tool_name,
            args,
            confidence,
            method,
        }
    }
}

pub fn select_best_candidate(candidates: Vec<ToolCallCandidate>) -> Option<ToolCallCandidate> {
    candidates
        .into_iter()
        .filter(|c| c.should_execute())
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

#[derive(Deserialize)]
pub struct StructuredResponse {
    pub action: String,
    pub tool_name: Option<String>,
    pub tool_args: Option<Value>,
    pub response_text: Option<String>,
}

use serde::Deserialize;
