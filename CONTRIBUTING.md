# Guía de Contribución

¡Gracias por tu interés en contribuir a Neuro! Esta guía te ayudará a empezar.

## Configuración del Entorno

1. **Instalar Rust**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Instalar Ollama**:
   - Descarga desde [ollama.ai](https://ollama.ai)
   - Instala el modelo: `ollama pull qwen3:8b`

3. **Clonar el repositorio**:
   ```bash
   git clone <repository-url>
   cd neuro-agent
   ```

4. **Compilar el proyecto**:
   ```bash
   cargo build
   ```

## Flujo de Trabajo

### Antes de Hacer Cambios

1. Crea una rama para tu feature:
   ```bash
   git checkout -b feature/mi-nueva-caracteristica
   ```

2. Asegúrate de que todo compila:
   ```bash
   cargo build
   cargo test
   ```

### Durante el Desarrollo

1. **Sigue las convenciones de código**:
   - Usa `cargo fmt` para formatear el código
   - Ejecuta `cargo clippy` para verificar sugerencias
   - Añade documentación a funciones públicas

2. **Escribe tests**:
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_mi_funcionalidad() {
           // ...
       }
   }
   ```

3. **Mantén commits atómicos**:
   ```bash
   git add -p  # Añade cambios parciales
   git commit -m "feat: descripción concisa del cambio"
   ```

### Antes de Hacer Pull Request

1. **Verifica que todo funciona**:
   ```bash
   cargo fmt
   cargo clippy --all-targets
   cargo build --release
   cargo test
   ```

2. **Actualiza la documentación** si es necesario

3. **Squash commits** si tienes muchos commits pequeños:
   ```bash
   git rebase -i HEAD~n  # n = número de commits
   ```

## Estructura del Proyecto

```
neuro-agent/
├── src/
│   ├── agent/          # Orquestación de modelos
│   ├── tools/          # Herramientas del agente
│   ├── raptor/         # Sistema RAPTOR para RAG
│   ├── ui/             # Interfaz TUI
│   ├── db/             # Persistencia
│   └── ...
├── Cargo.toml
└── README.md
```

## Convenciones de Código

### Nombres

- Structs: `PascalCase`
- Funciones: `snake_case`
- Constantes: `SCREAMING_SNAKE_CASE`
- Módulos: `snake_case`

### Documentación

Usa doc comments para elementos públicos:

```rust
/// Calcula el hash SHA256 de un contenido.
///
/// # Argumentos
///
/// * `content` - El contenido a hashear
///
/// # Ejemplo
///
/// ```
/// let hash = compute_hash(b"hello");
/// assert_eq!(hash.len(), 64);
/// ```
pub fn compute_hash(content: &[u8]) -> String {
    // ...
}
```

### Error Handling

Usa `anyhow::Result` para errores y proporciona contexto:

```rust
use anyhow::{Context, Result};

pub fn read_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .context(format!("Failed to read config from {:?}", path))?;
    
    let config: Config = toml::from_str(&content)
        .context("Failed to parse TOML config")?;
    
    Ok(config)
}
```

## Tipos de Commits

Seguimos [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` - Nueva funcionalidad
- `fix:` - Corrección de bug
- `docs:` - Cambios en documentación
- `style:` - Cambios de formato (no afectan el código)
- `refactor:` - Refactorización de código
- `perf:` - Mejoras de rendimiento
- `test:` - Añadir o modificar tests
- `chore:` - Cambios en build, dependencies, etc.

## Preguntas y Ayuda

- Abre un issue para preguntas o sugerencias
- Revisa issues existentes antes de crear uno nuevo
- Sé respetuoso y constructivo en las discusiones

¡Gracias por contribuir! 🚀
