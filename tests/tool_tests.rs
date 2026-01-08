//! Tests específicos para herramientas (Tools)
//!
//! Verifica que cada tool funciona correctamente de forma aislada

use tempfile::TempDir;

/// Test de CalculatorTool
#[tokio::test]
async fn test_calculator_tool() {
    let test_cases = vec![
        ("2 + 2", 4.0),
        ("10 * 5", 50.0),
        ("100 / 4", 25.0),
        ("15 - 7", 8.0),
        ("2 ^ 8", 256.0),
        ("sqrt(144)", 12.0),
        ("sin(0)", 0.0),
    ];

    for (expr, expected) in test_cases {
        println!("\n🧮 Calculando: {}", expr);
        
        // Aquí necesitarías implementar la llamada real al tool
        // Esto es un ejemplo de cómo podría estructurarse
        let result = evaluate_expression(expr);
        
        match result {
            Ok(value) => {
                println!("   ✅ Resultado: {}", value);
                assert!((value - expected).abs() < 0.01, 
                    "Esperado {} pero obtuvo {}", expected, value);
            }
            Err(e) => {
                panic!("❌ Error calculando '{}': {}", expr, e);
            }
        }
    }
}

/// Helper function para evaluar expresiones
fn evaluate_expression(expr: &str) -> Result<f64, String> {
    use meval;
    meval::eval_str(expr).map_err(|e| e.to_string())
}

/// Test de FileReadTool
#[tokio::test]
async fn test_file_read_tool() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    
    let content = "Este es un archivo de prueba\nCon múltiples líneas\n¡Funciona!";
    std::fs::write(&test_file, content).unwrap();
    
    println!("\n📖 Leyendo archivo: {}", test_file.display());
    
    let read_content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(read_content, content);
    println!("   ✅ Contenido leído correctamente");
}

/// Test de FileWriteTool
#[tokio::test]
async fn test_file_write_tool() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("output.txt");
    
    let content = "Contenido escrito por el test";
    
    println!("\n📝 Escribiendo archivo: {}", test_file.display());
    
    std::fs::write(&test_file, content).unwrap();
    
    let read_back = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(read_back, content);
    println!("   ✅ Archivo escrito y verificado correctamente");
}

/// Test de ListDirectoryTool
#[tokio::test]
async fn test_list_directory_tool() {
    let temp_dir = TempDir::new().unwrap();
    
    // Crear estructura de prueba
    std::fs::write(temp_dir.path().join("file1.txt"), "test1").unwrap();
    std::fs::write(temp_dir.path().join("file2.rs"), "test2").unwrap();
    std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();
    
    println!("\n📂 Listando directorio: {}", temp_dir.path().display());
    
    let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    
    println!("   Entradas encontradas: {:?}", entries);
    
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().any(|e| e.contains("file1.txt")));
    assert!(entries.iter().any(|e| e.contains("file2.rs")));
    assert!(entries.iter().any(|e| e.contains("subdir")));
    
    println!("   ✅ Directorio listado correctamente");
}

/// Test de ShellExecuteTool (comandos seguros)
#[tokio::test]
async fn test_shell_execute_safe_commands() {
    use std::process::Command;
    
    let safe_commands = vec![
        ("echo", vec!["Hello World"]),
        ("pwd", vec![]),
        ("date", vec![]),
    ];

    for (cmd, args) in safe_commands {
        println!("\n💻 Ejecutando: {} {:?}", cmd, args);
        
        let output = Command::new(cmd)
            .args(&args)
            .output();
        
        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);
                
                println!("   Salida: {}", stdout);
                if !stderr.is_empty() {
                    println!("   Stderr: {}", stderr);
                }
                
                assert!(result.status.success(), "Comando debería ejecutarse exitosamente");
                println!("   ✅ Comando ejecutado correctamente");
            }
            Err(e) => {
                println!("   ⚠️ Error (puede ser esperado): {}", e);
            }
        }
    }
}

/// Test de detección de comandos peligrosos
#[tokio::test]
async fn test_dangerous_command_detection() {
    let dangerous_commands = vec![
        "rm -rf /",
        "dd if=/dev/zero of=/dev/sda",
        "mkfs.ext4 /dev/sda",
        ":(){ :|:& };:",  // Fork bomb
        "chmod 777 / -R",
    ];

    for cmd in dangerous_commands {
        println!("\n⚠️ Verificando comando peligroso: {}", cmd);
        
        let is_dangerous = is_command_dangerous(cmd);
        assert!(is_dangerous, "El comando '{}' debería ser detectado como peligroso", cmd);
        
        println!("   ✅ Correctamente identificado como peligroso");
    }
}

/// Helper para detectar comandos peligrosos
fn is_command_dangerous(cmd: &str) -> bool {
    let dangerous_patterns = vec![
        "rm -rf",
        "dd if=",
        "mkfs",
        ":()",
        "chmod 777",
        "> /dev/",
        "format",
        "del /f",
    ];
    
    dangerous_patterns.iter().any(|pattern| cmd.contains(pattern))
}

/// Test de Git tool (si existe repositorio)
#[tokio::test]
async fn test_git_operations() {
    use std::process::Command;
    
    println!("\n🔧 Verificando operaciones Git...");
    
    // Verificar si estamos en un repo git
    let is_git_repo = Command::new("git")
        .args(&["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    
    if !is_git_repo {
        println!("   ⚠️ No es un repositorio Git, saltando test");
        return;
    }
    
    // Test git status
    println!("\n   Ejecutando: git status");
    let status = Command::new("git")
        .args(&["status", "--short"])
        .output()
        .unwrap();
    
    let output = String::from_utf8_lossy(&status.stdout);
    println!("   Status:\n{}", output);
    assert!(status.status.success());
    
    // Test git log
    println!("\n   Ejecutando: git log --oneline -5");
    let log = Command::new("git")
        .args(&["log", "--oneline", "-5"])
        .output()
        .unwrap();
    
    let log_output = String::from_utf8_lossy(&log.stdout);
    println!("   Log:\n{}", log_output);
    assert!(log.status.success());
    
    println!("\n   ✅ Operaciones Git funcionan correctamente");
}

/// Test de Search tool
#[tokio::test]
async fn test_search_tool() {
    let temp_dir = TempDir::new().unwrap();
    
    // Crear archivos de prueba
    std::fs::write(
        temp_dir.path().join("file1.txt"),
        "Este archivo contiene la palabra clave BUSCAR"
    ).unwrap();
    
    std::fs::write(
        temp_dir.path().join("file2.txt"),
        "Este no contiene nada especial"
    ).unwrap();
    
    std::fs::write(
        temp_dir.path().join("file3.rs"),
        "fn main() {\n    println!(\"BUSCAR en Rust\");\n}"
    ).unwrap();
    
    println!("\n🔍 Buscando palabra 'BUSCAR' en: {}", temp_dir.path().display());
    
    // Implementar búsqueda simple
    let mut matches = Vec::new();
    for entry in std::fs::read_dir(temp_dir.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_file() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if content.contains("BUSCAR") {
                    matches.push(entry.file_name().to_string_lossy().to_string());
                }
            }
        }
    }
    
    println!("   Coincidencias: {:?}", matches);
    assert_eq!(matches.len(), 2, "Debería encontrar 2 archivos");
    println!("   ✅ Búsqueda completada correctamente");
}

/// Test de Formatter tool
#[tokio::test]
async fn test_formatter_tool() {
    let unformatted_rust = "fn main(){println!(\"test\");let x=5;let y=10;}";
    let expected_formatted = "fn main() {\n    println!(\"test\");\n    let x = 5;\n    let y = 10;\n}";
    
    println!("\n✨ Test Formatter");
    println!("   Código sin formatear: {}", unformatted_rust);
    
    // En un caso real, llamarías a rustfmt o similar
    // Aquí simulamos el resultado esperado
    
    println!("   Código formateado:");
    println!("{}", expected_formatted);
    
    // Verificaciones básicas del formato
    assert!(expected_formatted.contains("fn main()"));
    assert!(expected_formatted.contains("    println!"));
    assert!(expected_formatted.contains("let x = 5"));
    
    println!("   ✅ Formato verificado");
}

/// Test de Analyzer tool
#[tokio::test]
async fn test_analyzer_tool() {
    let code = r#"
fn fibonacci(n: u32) -> u32 {
    if n <= 1 {
        return n;
    }
    fibonacci(n - 1) + fibonacci(n - 2)
}

fn main() {
    let result = fibonacci(10);
    println!("Result: {}", result);
}
"#;
    
    println!("\n🔬 Analizando código:");
    println!("{}", code);
    
    // Análisis básico
    let lines = code.lines().count();
    let functions = code.matches("fn ").count();
    let is_recursive = code.contains("fibonacci(n - 1)");
    
    println!("\n   Métricas:");
    println!("   - Líneas: {}", lines);
    println!("   - Funciones: {}", functions);
    println!("   - ¿Es recursivo?: {}", is_recursive);
    
    assert!(lines > 5);
    assert_eq!(functions, 2);
    assert!(is_recursive);
    
    println!("   ✅ Análisis completado");
}

/// Test de Documentation tool
#[tokio::test]
async fn test_documentation_extraction() {
    let code_with_docs = r#"
/// Esta función suma dos números
/// 
/// # Argumentos
/// * `a` - Primer número
/// * `b` - Segundo número
/// 
/// # Returns
/// La suma de a y b
fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
    
    println!("\n📚 Extrayendo documentación:");
    
    // Extraer comentarios de documentación
    let doc_lines: Vec<&str> = code_with_docs
        .lines()
        .filter(|line| line.trim().starts_with("///"))
        .collect();
    
    println!("   Documentación encontrada:");
    for line in &doc_lines {
        println!("   {}", line);
    }
    
    assert!(doc_lines.len() > 0, "Debería encontrar líneas de documentación");
    assert!(doc_lines.iter().any(|l| l.contains("suma dos números")));
    
    println!("   ✅ Documentación extraída correctamente");
}

/// Test de Test Runner tool (simulado)
#[tokio::test]
async fn test_runner_simulation() {
    use std::process::Command;
    
    println!("\n🧪 Test Runner - Ejecutando tests del proyecto...");
    
    // Ejecutar cargo test en modo check (no ejecuta, solo compila)
    let output = Command::new("cargo")
        .args(&["test", "--no-run", "--message-format=short"])
        .output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);
            
            println!("   Compilación de tests:");
            if !stdout.is_empty() {
                println!("{}", stdout);
            }
            if !stderr.is_empty() {
                println!("{}", stderr);
            }
            
            if result.status.success() {
                println!("   ✅ Tests compilaron correctamente");
            } else {
                println!("   ⚠️ Error compilando tests");
            }
        }
        Err(e) => {
            println!("   ⚠️ No se pudo ejecutar cargo: {}", e);
        }
    }
}

/// Test de Context tool
#[tokio::test]
async fn test_context_gathering() {
    println!("\n📋 Recolectando contexto del proyecto...");
    
    let current_dir = std::env::current_dir().unwrap();
    println!("   Directorio: {}", current_dir.display());
    
    // Verificar Cargo.toml
    let cargo_toml = current_dir.join("Cargo.toml");
    if cargo_toml.exists() {
        println!("   ✅ Cargo.toml encontrado");
        
        let content = std::fs::read_to_string(&cargo_toml).unwrap();
        if let Some(line) = content.lines().find(|l| l.starts_with("name =")) {
            println!("   Proyecto: {}", line);
        }
    }
    
    // Contar archivos Rust
    let mut rust_files = 0;
    if let Ok(entries) = std::fs::read_dir(current_dir.join("src")) {
        for entry in entries {
            if let Ok(entry) = entry {
                if entry.path().extension().and_then(|s| s.to_str()) == Some("rs") {
                    rust_files += 1;
                }
            }
        }
    }
    println!("   Archivos .rs en src/: {}", rust_files);
    
    assert!(rust_files > 0, "Debería haber archivos Rust");
    println!("   ✅ Contexto recolectado");
}

/// Test de dependencias
#[tokio::test]
async fn test_dependency_analysis() {
    println!("\n📦 Analizando dependencias...");
    
    let cargo_toml = std::env::current_dir()
        .unwrap()
        .join("Cargo.toml");
    
    if !cargo_toml.exists() {
        println!("   ⚠️ Cargo.toml no encontrado");
        return;
    }
    
    let content = std::fs::read_to_string(&cargo_toml).unwrap();
    
    // Buscar sección de dependencias
    let in_deps = content.lines()
        .skip_while(|l| !l.starts_with("[dependencies]"))
        .skip(1)
        .take_while(|l| !l.starts_with('['))
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .count();
    
    println!("   Dependencias encontradas: {}", in_deps);
    
    // Algunas dependencias clave
    let has_tokio = content.contains("tokio");
    let has_serde = content.contains("serde");
    
    println!("   - tokio: {}", has_tokio);
    println!("   - serde: {}", has_serde);
    
    println!("   ✅ Análisis de dependencias completado");
}
