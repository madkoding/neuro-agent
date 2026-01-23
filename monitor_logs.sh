#!/bin/bash

# Monitor Neuro logs with color coding and filtering

LOG_FILE="$HOME/.local/share/neuro/neuro.log"

if [ ! -f "$LOG_FILE" ]; then
    echo "❌ Log file not found: $LOG_FILE"
    echo "Run neuro first to create the log file"
    exit 1
fi

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Get filter mode from argument
FILTER_MODE="${1:-all}"

echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${CYAN}  Neuro Log Monitor${NC}"
echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
echo "Log file: $LOG_FILE"
echo ""
echo "Usage: $0 [filter]"
echo "  all       - Show all logs (default)"
echo "  timing    - Show only timing logs"
echo "  task      - Show only background task logs"
echo "  loop      - Show only event loop logs"
echo "  errors    - Show only errors and warnings"
echo "  follow    - Follow logs in real-time"
echo ""

# Function to print formatted log
print_log() {
    local line=$1

    # Color by level
    if echo "$line" | grep -q "\[ERROR\]"; then
        echo -e "${RED}$line${NC}"
    elif echo "$line" | grep -q "\[WARN\]"; then
        echo -e "${YELLOW}$line${NC}"
    elif echo "$line" | grep -q "BG-TASK"; then
        echo -e "${PURPLE}$line${NC}"
    elif echo "$line" | grep -q "EVENT-LOOP"; then
        echo -e "${BLUE}$line${NC}"
    elif echo "$line" | grep -q "TIMING"; then
        echo -e "${GREEN}$line${NC}"
    else
        echo "$line"
    fi
}

case "$FILTER_MODE" in
    timing)
        echo -e "${GREEN}📊 Showing TIMING logs...${NC}\n"
        tail -f "$LOG_FILE" | grep --line-buffered "TIMING" | while read line; do
            print_log "$line"
        done
        ;;
    task)
        echo -e "${PURPLE}🔧 Showing BACKGROUND TASK logs...${NC}\n"
        tail -f "$LOG_FILE" | grep --line-buffered "BG-TASK" | while read line; do
            print_log "$line"
        done
        ;;
    loop)
        echo -e "${BLUE}🔄 Showing EVENT LOOP logs...${NC}\n"
        tail -f "$LOG_FILE" | grep --line-buffered "EVENT-LOOP" | while read line; do
            print_log "$line"
        done
        ;;
    errors)
        echo -e "${RED}⚠️  Showing ERRORS and WARNINGS...${NC}\n"
        tail -f "$LOG_FILE" | grep --line-buffered -E "ERROR|WARN" | while read line; do
            print_log "$line"
        done
        ;;
    follow)
        echo -e "${CYAN}📋 Following all logs in real-time...${NC}\n"
        tail -f "$LOG_FILE" | while read line; do
            print_log "$line"
        done
        ;;
    all)
        echo -e "${CYAN}📋 Showing last 50 lines of all logs...${NC}\n"
        tail -50 "$LOG_FILE" | while read line; do
            print_log "$line"
        done
        echo ""
        echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
        echo -e "${CYAN}To follow logs in real-time, run: $0 follow${NC}"
        echo -e "${CYAN}═══════════════════════════════════════════════════════════════${NC}"
        ;;
    *)
        echo -e "${RED}❌ Unknown filter mode: $FILTER_MODE${NC}"
        exit 1
        ;;
esac
