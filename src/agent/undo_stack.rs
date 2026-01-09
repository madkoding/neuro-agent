//! Undo/Redo Stack - Sistema de deshacer/rehacer operaciones
//!
//! Mantiene un historial de operaciones de archivos para permitir
//! deshacer cambios y rehacerlos posteriormente.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Tipo de operación en el stack
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationType {
    /// Escritura de archivo (modificación)
    FileWrite,
    /// Eliminación de archivo
    FileDelete,
    /// Creación de archivo nuevo
    FileCreate,
}

/// Operación individual que puede ser deshecha/rehecha
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// ID único de la operación (UUID)
    pub id: String,
    /// Momento en que se realizó la operación
    pub timestamp: SystemTime,
    /// Tipo de operación
    pub op_type: OperationType,
    /// Ruta del archivo afectado
    pub file_path: PathBuf,
    /// Contenido anterior del archivo (para undo)
    pub old_content: String,
    /// Contenido nuevo del archivo (para redo)
    pub new_content: String,
}

impl Operation {
    /// Crea una nueva operación
    pub fn new(
        op_type: OperationType,
        file_path: PathBuf,
        old_content: String,
        new_content: String,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            op_type,
            file_path,
            old_content,
            new_content,
        }
    }

    /// Aplica el undo (restaura old_content)
    pub fn apply_undo(&self) -> Result<()> {
        match self.op_type {
            OperationType::FileWrite => {
                // Restaurar contenido anterior
                if let Some(parent) = self.file_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&self.file_path, &self.old_content)?;
                Ok(())
            }
            OperationType::FileCreate => {
                // Eliminar archivo creado
                if self.file_path.exists() {
                    fs::remove_file(&self.file_path)?;
                }
                Ok(())
            }
            OperationType::FileDelete => {
                // Recrear archivo eliminado
                if let Some(parent) = self.file_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&self.file_path, &self.old_content)?;
                Ok(())
            }
        }
    }

    /// Aplica el redo (aplica new_content)
    pub fn apply_redo(&self) -> Result<()> {
        match self.op_type {
            OperationType::FileWrite => {
                // Aplicar nuevo contenido
                if let Some(parent) = self.file_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&self.file_path, &self.new_content)?;
                Ok(())
            }
            OperationType::FileCreate => {
                // Recrear archivo
                if let Some(parent) = self.file_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&self.file_path, &self.new_content)?;
                Ok(())
            }
            OperationType::FileDelete => {
                // Re-eliminar archivo
                if self.file_path.exists() {
                    fs::remove_file(&self.file_path)?;
                }
                Ok(())
            }
        }
    }

    /// Genera una descripción legible de la operación
    pub fn description(&self) -> String {
        let op_name = match self.op_type {
            OperationType::FileWrite => "write",
            OperationType::FileCreate => "create",
            OperationType::FileDelete => "delete",
        };
        
        let file_name = self.file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        
        let old_lines = self.old_content.lines().count();
        let new_lines = self.new_content.lines().count();
        
        format!("{} {} ({} → {} lines)", op_name, file_name, old_lines, new_lines)
    }
}

/// Stack de operaciones con soporte para undo/redo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoStack {
    /// Tamaño máximo del stack
    max_size: usize,
    /// Lista de operaciones (más reciente al final)
    operations: Vec<Operation>,
    /// Índice de la operación actual (apunta después de la última ejecutada)
    /// 0 = ninguna operación ejecutada
    /// operations.len() = todas ejecutadas
    current_index: usize,
}

impl UndoStack {
    /// Crea un nuevo stack con tamaño máximo
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size,
            operations: Vec::new(),
            current_index: 0,
        }
    }

    /// Crea un stack con tamaño por defecto (10)
    pub fn default() -> Self {
        Self::new(10)
    }

    /// Agrega una nueva operación al stack
    /// Limpia el historial de redo si hay operaciones por delante
    pub fn push(&mut self, operation: Operation) {
        // Eliminar operaciones más allá del current_index (invalida redo)
        self.operations.truncate(self.current_index);

        // Agregar nueva operación
        self.operations.push(operation);
        self.current_index = self.operations.len();

        // Mantener tamaño máximo (eliminar las más antiguas)
        if self.operations.len() > self.max_size {
            let to_remove = self.operations.len() - self.max_size;
            self.operations.drain(0..to_remove);
            self.current_index = self.operations.len();
        }
    }

    /// Deshace la última operación
    /// Retorna la operación que fue deshecha
    pub fn undo(&mut self) -> Result<&Operation> {
        if !self.can_undo() {
            return Err(anyhow!("No hay operaciones para deshacer"));
        }

        self.current_index -= 1;
        let operation = &self.operations[self.current_index];
        
        // Aplicar el undo
        operation.apply_undo()?;
        
        Ok(operation)
    }

    /// Rehace una operación previamente deshecha
    /// Retorna la operación que fue rehecha
    pub fn redo(&mut self) -> Result<&Operation> {
        if !self.can_redo() {
            return Err(anyhow!("No hay operaciones para rehacer"));
        }

        let operation = &self.operations[self.current_index];
        
        // Aplicar el redo
        operation.apply_redo()?;
        
        self.current_index += 1;
        
        Ok(operation)
    }

    /// Verifica si se puede deshacer
    pub fn can_undo(&self) -> bool {
        self.current_index > 0
    }

    /// Verifica si se puede rehacer
    pub fn can_redo(&self) -> bool {
        self.current_index < self.operations.len()
    }

    /// Limpia todo el stack
    pub fn clear(&mut self) {
        self.operations.clear();
        self.current_index = 0;
    }

    /// Obtiene todas las operaciones
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Obtiene la operación actual (última ejecutada)
    pub fn current_operation(&self) -> Option<&Operation> {
        if self.current_index > 0 {
            self.operations.get(self.current_index - 1)
        } else {
            None
        }
    }

    /// Obtiene el número de operaciones que se pueden deshacer
    pub fn undo_count(&self) -> usize {
        self.current_index
    }

    /// Obtiene el número de operaciones que se pueden rehacer
    pub fn redo_count(&self) -> usize {
        self.operations.len() - self.current_index
    }

    /// Genera un resumen del estado del stack
    pub fn summary(&self) -> String {
        format!(
            "Undo Stack: {} operations (can undo: {}, can redo: {})",
            self.operations.len(),
            self.undo_count(),
            self.redo_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_undo_stack_creation() {
        let stack = UndoStack::new(5);
        assert_eq!(stack.max_size, 5);
        assert_eq!(stack.operations.len(), 0);
        assert_eq!(stack.current_index, 0);
        assert!(!stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn test_push_operation() {
        let mut stack = UndoStack::new(10);
        
        let op = Operation::new(
            OperationType::FileWrite,
            PathBuf::from("test.txt"),
            "old content".to_string(),
            "new content".to_string(),
        );
        
        stack.push(op);
        
        assert_eq!(stack.operations.len(), 1);
        assert_eq!(stack.current_index, 1);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn test_undo_single_operation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        
        // Crear archivo inicial
        fs::write(&file_path, "old content").unwrap();
        
        // Simular escritura
        fs::write(&file_path, "new content").unwrap();
        
        let mut stack = UndoStack::new(10);
        let op = Operation::new(
            OperationType::FileWrite,
            file_path.clone(),
            "old content".to_string(),
            "new content".to_string(),
        );
        stack.push(op);
        
        // Deshacer
        let result = stack.undo();
        assert!(result.is_ok());
        
        // Verificar que se restauró el contenido antiguo
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "old content");
        assert!(!stack.can_undo());
        assert!(stack.can_redo());
    }

    #[test]
    fn test_redo_single_operation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        
        fs::write(&file_path, "old content").unwrap();
        
        let mut stack = UndoStack::new(10);
        let op = Operation::new(
            OperationType::FileWrite,
            file_path.clone(),
            "old content".to_string(),
            "new content".to_string(),
        );
        stack.push(op);
        
        // Deshacer y rehacer
        stack.undo().unwrap();
        let result = stack.redo();
        assert!(result.is_ok());
        
        // Verificar que se aplicó el nuevo contenido
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content");
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn test_undo_redo_chain() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        
        fs::write(&file_path, "v1").unwrap();
        
        let mut stack = UndoStack::new(10);
        
        // Push 3 operaciones
        stack.push(Operation::new(
            OperationType::FileWrite,
            file_path.clone(),
            "v1".to_string(),
            "v2".to_string(),
        ));
        
        stack.push(Operation::new(
            OperationType::FileWrite,
            file_path.clone(),
            "v2".to_string(),
            "v3".to_string(),
        ));
        
        stack.push(Operation::new(
            OperationType::FileWrite,
            file_path.clone(),
            "v3".to_string(),
            "v4".to_string(),
        ));
        
        assert_eq!(stack.operations.len(), 3);
        assert_eq!(stack.current_index, 3);
        
        // Undo 2 veces
        stack.undo().unwrap();
        assert_eq!(stack.current_index, 2);
        
        stack.undo().unwrap();
        assert_eq!(stack.current_index, 1);
        
        // Redo 1 vez
        stack.redo().unwrap();
        assert_eq!(stack.current_index, 2);
        
        assert_eq!(stack.undo_count(), 2);
        assert_eq!(stack.redo_count(), 1);
    }

    #[test]
    fn test_max_size_limit() {
        let mut stack = UndoStack::new(3);
        
        // Push 5 operaciones (excede max_size)
        for i in 1..=5 {
            stack.push(Operation::new(
                OperationType::FileWrite,
                PathBuf::from(format!("file{}.txt", i)),
                format!("old{}", i),
                format!("new{}", i),
            ));
        }
        
        // Solo debe mantener las últimas 3
        assert_eq!(stack.operations.len(), 3);
        assert_eq!(stack.current_index, 3);
        
        // Las operaciones más antiguas deben haber sido eliminadas
        assert!(stack.operations[0].file_path.to_str().unwrap().contains("file3"));
    }

    #[test]
    fn test_cannot_redo_after_new_operation() {
        let mut stack = UndoStack::new(10);
        
        // Push 2 operaciones
        stack.push(Operation::new(
            OperationType::FileWrite,
            PathBuf::from("test.txt"),
            "v1".to_string(),
            "v2".to_string(),
        ));
        
        stack.push(Operation::new(
            OperationType::FileWrite,
            PathBuf::from("test.txt"),
            "v2".to_string(),
            "v3".to_string(),
        ));
        
        // Deshacer una
        stack.undo().unwrap();
        assert!(stack.can_redo());
        
        // Push nueva operación
        stack.push(Operation::new(
            OperationType::FileWrite,
            PathBuf::from("test.txt"),
            "v2".to_string(),
            "v4".to_string(),
        ));
        
        // Ya no se puede rehacer (se limpió el historial)
        assert!(!stack.can_redo());
        assert_eq!(stack.operations.len(), 2);
    }

    #[test]
    fn test_operation_description() {
        let op = Operation::new(
            OperationType::FileWrite,
            PathBuf::from("test.txt"),
            "line1\nline2".to_string(),
            "line1\nline2\nline3".to_string(),
        );
        
        let desc = op.description();
        assert!(desc.contains("write"));
        assert!(desc.contains("test.txt"));
        assert!(desc.contains("2 → 3"));
    }

    #[test]
    fn test_file_create_undo() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("new_file.txt");
        
        // Crear archivo
        fs::write(&file_path, "content").unwrap();
        
        let mut stack = UndoStack::new(10);
        stack.push(Operation::new(
            OperationType::FileCreate,
            file_path.clone(),
            String::new(),
            "content".to_string(),
        ));
        
        // Deshacer (eliminar archivo)
        stack.undo().unwrap();
        assert!(!file_path.exists());
        
        // Rehacer (recrear archivo)
        stack.redo().unwrap();
        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "content");
    }
}
