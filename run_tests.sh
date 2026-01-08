#!/bin/bash
# Script para ejecutar los tests funcionales de Neuro Agent
# Uso: ./run_tests.sh [opción]

set -e

# Colores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Banner
echo -e "${BLUE}"
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║       🧪 NEURO AGENT - TEST SUITE RUNNER 🧪              ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# Función para verificar si Ollama está corriendo
check_ollama() {
    echo -e "${YELLOW}🔍 Verificando Ollama...${NC}"
    if curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Ollama está corriendo${NC}"
        return 0
    else
        echo -e "${RED}❌ Ollama no está corriendo${NC}"
        echo -e "${YELLOW}💡 Inicia Ollama con: ollama serve${NC}"
        return 1
    fi
}

# Función para verificar modelos
check_models() {
    echo -e "${YELLOW}🔍 Verificando modelos...${NC}"
    
    if ollama list | grep -q "qwen3:0.6b"; then
        echo -e "${GREEN}✅ Modelo qwen3:0.6b disponible${NC}"
    else
        echo -e "${RED}❌ Modelo qwen3:0.6b no encontrado${NC}"
        echo -e "${YELLOW}💡 Descárgalo con: ollama pull qwen3:0.6b${NC}"
    fi
    
    if ollama list | grep -q "qwen3:8b"; then
        echo -e "${GREEN}✅ Modelo qwen3:8b disponible${NC}"
    else
        echo -e "${RED}❌ Modelo qwen3:8b no encontrado${NC}"
        echo -e "${YELLOW}💡 Descárgalo con: ollama pull qwen3:8b${NC}"
    fi
}

# Función para mostrar ayuda
show_help() {
    echo "Uso: $0 [opción]"
    echo ""
    echo "Opciones:"
    echo "  all           - Ejecutar TODOS los tests (requiere Ollama)"
    echo "  fast          - Solo tests rápidos (sin Ollama)"
    echo "  functional    - Tests funcionales completos (requiere Ollama)"
    echo "  tools         - Tests de herramientas"
    echo "  classification - Tests de clasificación y routing"
    echo "  chat          - Test de chat conversacional"
    echo "  arithmetic    - Test de operaciones aritméticas"
    echo "  code          - Test de generación de código"
    echo "  context       - Test de comprensión de contexto"
    echo "  integration   - Test de integración completa"
    echo "  check         - Verificar requisitos (Ollama y modelos)"
    echo "  help          - Mostrar esta ayuda"
    echo ""
    echo "Ejemplos:"
    echo "  $0 fast           # Tests rápidos sin Ollama"
    echo "  $0 functional     # Todos los tests funcionales"
    echo "  $0 chat           # Solo test de chat"
    echo "  $0 check          # Verificar configuración"
}

# Procesar argumentos
case "${1:-help}" in
    all)
        echo -e "${BLUE}🚀 Ejecutando TODOS los tests...${NC}"
        check_ollama || exit 1
        echo ""
        cargo test --verbose
        cargo test --test functional_tests -- --ignored --nocapture
        ;;
    
    fast)
        echo -e "${BLUE}⚡ Ejecutando tests rápidos (sin Ollama)...${NC}"
        echo ""
        cargo test --test tool_tests
        cargo test --test classification_tests
        ;;
    
    functional)
        echo -e "${BLUE}🧪 Ejecutando tests funcionales completos...${NC}"
        check_ollama || exit 1
        echo ""
        cargo test --test functional_tests -- --ignored --nocapture --test-threads=1
        ;;
    
    tools)
        echo -e "${BLUE}🔧 Ejecutando tests de herramientas...${NC}"
        echo ""
        cargo test --test tool_tests -- --nocapture
        ;;
    
    classification)
        echo -e "${BLUE}📊 Ejecutando tests de clasificación...${NC}"
        echo ""
        cargo test --test classification_tests -- --nocapture
        ;;
    
    chat)
        echo -e "${BLUE}💬 Ejecutando test de chat...${NC}"
        check_ollama || exit 1
        echo ""
        cargo test --test functional_tests test_simple_chat -- --ignored --nocapture
        ;;
    
    arithmetic)
        echo -e "${BLUE}🧮 Ejecutando test de aritmética...${NC}"
        check_ollama || exit 1
        echo ""
        cargo test --test functional_tests test_arithmetic_operations -- --ignored --nocapture
        ;;
    
    code)
        echo -e "${BLUE}💻 Ejecutando test de generación de código...${NC}"
        check_ollama || exit 1
        echo ""
        cargo test --test functional_tests test_code_generation -- --ignored --nocapture
        ;;
    
    context)
        echo -e "${BLUE}🧠 Ejecutando test de comprensión de contexto...${NC}"
        check_ollama || exit 1
        echo ""
        cargo test --test functional_tests test_context_comprehension -- --ignored --nocapture
        ;;
    
    integration)
        echo -e "${BLUE}🔄 Ejecutando test de integración completa...${NC}"
        check_ollama || exit 1
        echo ""
        cargo test --test functional_tests test_full_integration_scenario -- --ignored --nocapture
        ;;
    
    check)
        echo -e "${BLUE}🔍 Verificando requisitos...${NC}"
        echo ""
        
        # Verificar Rust
        if command -v cargo &> /dev/null; then
            echo -e "${GREEN}✅ Cargo instalado:${NC} $(cargo --version)"
        else
            echo -e "${RED}❌ Cargo no encontrado${NC}"
        fi
        
        # Verificar Ollama
        if command -v ollama &> /dev/null; then
            echo -e "${GREEN}✅ Ollama instalado:${NC} $(ollama --version 2>/dev/null || echo 'version desconocida')"
            check_ollama
            check_models
        else
            echo -e "${RED}❌ Ollama no instalado${NC}"
            echo -e "${YELLOW}💡 Instala desde: https://ollama.ai${NC}"
        fi
        
        # Verificar estructura de tests
        echo ""
        echo -e "${YELLOW}📂 Estructura de tests:${NC}"
        if [ -f "tests/functional_tests.rs" ]; then
            echo -e "${GREEN}✅ tests/functional_tests.rs${NC}"
        fi
        if [ -f "tests/tool_tests.rs" ]; then
            echo -e "${GREEN}✅ tests/tool_tests.rs${NC}"
        fi
        if [ -f "tests/classification_tests.rs" ]; then
            echo -e "${GREEN}✅ tests/classification_tests.rs${NC}"
        fi
        ;;
    
    help|--help|-h)
        show_help
        ;;
    
    *)
        echo -e "${RED}❌ Opción desconocida: $1${NC}"
        echo ""
        show_help
        exit 1
        ;;
esac

echo ""
echo -e "${GREEN}✨ Completado!${NC}"
