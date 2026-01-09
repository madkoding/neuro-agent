//! Context module exports

pub mod cache;
pub mod git_context;
pub mod manager;
pub mod related_files;

pub use git_context::{GitChangedFile, GitChangeType, GitContext};
pub use manager::{ContextManager, LLMContext, Priority};
pub use related_files::{RelatedFile, RelatedFilesDetector, RelationType};
