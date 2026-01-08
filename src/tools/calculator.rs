//! Calculator tool for evaluating mathematical expressions

use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// Errors
// ============================================================================

#[derive(Error, Debug)]
pub enum CalculatorError {
    #[error("Invalid expression: {0}")]
    InvalidExpression(String),
    #[error("Evaluation error: {0}")]
    EvaluationError(String),
}

// ============================================================================
// CalculatorTool
// ============================================================================

/// Calculator tool for evaluating mathematical expressions
/// Supports basic arithmetic operations: +, -, *, /, ^, sqrt, sin, cos, etc.
#[derive(Debug, Clone, Default)]
pub struct CalculatorTool;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalculatorArgs {
    /// The mathematical expression to evaluate (e.g., "1+1", "2*3+5", "sqrt(16)")
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculatorOutput {
    /// The result of the expression
    pub result: String,
    /// The original expression
    pub expression: String,
}

impl Tool for CalculatorTool {
    const NAME: &'static str = "calculator";

    type Args = CalculatorArgs;
    type Output = CalculatorOutput;
    type Error = CalculatorError;

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        rig::completion::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Evaluate mathematical expressions. Supports: +, -, *, /, ^, sqrt, sin, cos, tan, ln, log, abs, ceil, floor. Examples: '1+1', '2*3+5', 'sqrt(16)', 'sin(3.14/2)'".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(CalculatorArgs))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let expression = args.expression.trim();

        // Evaluate the expression using meval
        match meval::eval_str(expression) {
            Ok(result) => {
                // Format the result nicely
                let formatted = if result.fract() == 0.0 && result.abs() < 1e10 {
                    // It's an integer, show without decimals
                    format!("{}", result as i64)
                } else {
                    // Show decimals with reasonable precision
                    format!("{:.6}", result).trim_end_matches('0').trim_end_matches('.').to_string()
                };

                Ok(CalculatorOutput {
                    result: formatted,
                    expression: expression.to_string(),
                })
            }
            Err(e) => Err(CalculatorError::EvaluationError(format!(
                "Could not evaluate '{}': {}",
                expression, e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_arithmetic() {
        let tool = CalculatorTool;

        let result = tool.call(CalculatorArgs {
            expression: "1+1".to_string(),
        }).await.unwrap();
        assert_eq!(result.result, "2");

        let result = tool.call(CalculatorArgs {
            expression: "2*3".to_string(),
        }).await.unwrap();
        assert_eq!(result.result, "6");

        let result = tool.call(CalculatorArgs {
            expression: "10/2".to_string(),
        }).await.unwrap();
        assert_eq!(result.result, "5");
    }

    #[tokio::test]
    async fn test_advanced_math() {
        let tool = CalculatorTool;

        let result = tool.call(CalculatorArgs {
            expression: "sqrt(16)".to_string(),
        }).await.unwrap();
        assert_eq!(result.result, "4");

        let result = tool.call(CalculatorArgs {
            expression: "2^3".to_string(),
        }).await.unwrap();
        assert_eq!(result.result, "8");
    }

    #[tokio::test]
    async fn test_invalid_expression() {
        let tool = CalculatorTool;

        let result = tool.call(CalculatorArgs {
            expression: "invalid".to_string(),
        }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_decimal_formatting() {
        let tool = CalculatorTool;

        let result = tool.call(CalculatorArgs {
            expression: "10/3".to_string(),
        }).await.unwrap();
        // Should have reasonable precision
        assert!(result.result.starts_with("3.333"));
    }
}
