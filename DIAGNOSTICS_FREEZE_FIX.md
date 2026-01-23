# Diagnostic Guide: 43-44 Second Freeze Investigation

## Overview
This guide explains the diagnostic logging that has been added to help identify where the 43-44 second freeze is occurring.

## What Changed
Enhanced the code with detailed timing and event logging at key points:

1. **Background Task Logging** (`src/ui/modern_app.rs` lines 1380-1441):
   - Logs when background task starts and completes
   - Tracks orchestrator lock acquisition time
   - Monitors `router_orch.process()` execution time
   - Shows the 120-second timeout wrapper status

2. **Event Loop Logging** (`src/ui/modern_app.rs` lines 742-753):
   - Logs every 100 event loop iterations (~8 seconds)
   - Tracks elapsed processing time
   - Confirms event loop is responsive

3. **Event Processing Logging** (`src/ui/modern_app.rs` lines 820-826):
   - Logs event type every 10 seconds during processing
   - Shows what kind of events are being received
   - Confirms chunks are arriving

## How to Test

### Step 1: Build with Debug Logging
```bash
cd /home/madkoding/proyectos/neuro-agent
cargo build --release
```

### Step 2: Run with Debug Logs Enabled
```bash
RUST_LOG=debug ./target/release/neuro
```

### Step 3: Send a Test Query
When the app is running, type a query that will take a while to respond:
```
Analiza este repositorio y explicame de que se trata
```

### Step 4: Watch for the Freeze
The program will start showing progress messages (1/5, 2/5, etc). Watch the timing:
- At 10s: You should see a log message "⏱️ [TIMING] Processing at 10s, event..."
- At 20s: Another timing log
- At 30s: Another timing log
- At 40s: Another timing log
- **At ~43-44s**: This is where the freeze occurs

### Step 5: Analyze the Logs
Look for these patterns in the logs:

#### Expected Pattern (Good):
```
🔧 [BG-TASK] Starting background task...
🔧 [BG-TASK] Acquired orchestrator lock at Xms
🔧 [BG-TASK] Using Router orchestrator
🔧 [BG-TASK] Event channel set at Xms
🔧 [BG-TASK] Calling router_orch.process() at Xms
... (many chunks arriving) ...
⏱️ [TIMING] Processing at 10s, event: ...
⏱️ [TIMING] Processing at 20s, event: ...
⏱️ [TIMING] Processing at 30s, event: ...
⏱️ [TIMING] Processing at 40s, event: ...
🔧 [BG-TASK] router_orch.process() returned after XXXXms (total: XXXXms)
🔧 [BG-TASK] Response received successfully
🔧 [BG-TASK] Background task complete at XXXXms
```

#### Problem Pattern A - Event Loop Not Responsive:
If you see the logs stop at ~40s and never reach 🔧 logs, it means the event loop itself is frozen.

#### Problem Pattern B - router_orch.process() Hanging:
If you see "Calling router_orch.process()" but never see "router_orch.process() returned", it means the process() call itself is hanging (and the 120s timeout should catch it).

#### Problem Pattern C - Chunks Stop Arriving:
If you see timing logs up to 40s but then no more chunks, it means:
- The background task is waiting for chunks
- But the orchestrator's internal tasks are blocked
- The 120s timeout will eventually timeout

## Key Log Messages to Look For

```
🔧 [BG-TASK] Starting background task            → Background work begins
🔧 [BG-TASK] Calling router_orch.process()       → Process method called
⏱️ [TIMING] Processing at 10s, event:            → Chunks arriving (shown every 10s)
🔄 [EVENT-LOOP] Iteration X, processing_elapsed  → Event loop running (shown every ~8s)
🔧 [BG-TASK] router_orch.process() returned      → Process method completed
```

## What Should Happen

### Good Case (Fast Response):
- At 5s: chunks start arriving
- At 10s-50s: steady stream of chunks (timing logs shown)
- At 50-60s: process() completes, StreamEnd received
- UI returns to "Ready" immediately

### Timeout Case (Process Taking >120s):
- At 1s-120s: chunks arriving normally
- At 120s: timeout fires, error message sent
- Error message shows in chat: "Timeout: El procesamiento tardó más de 120 segundos"
- UI returns to "Ready" after timeout message

### Freeze Case (What User Reported):
- At 0-40s: chunks arriving, everything looks good
- At 43-44s: **Event loop stops responding**
- No more UI updates
- No more log messages
- Program appears frozen

## If Freeze Still Occurs

1. **Check the event loop is still running:**
   - Do you see `🔄 [EVENT-LOOP]` messages continuing past 40s?
   - If no: The event loop itself is frozen (likely in `draw()` or `event::poll()`)
   - If yes: Events are being processed, look for chunks still arriving

2. **Check if background task is running:**
   - Do you see `🔧 [BG-TASK]` messages continuing?
   - If no: Background task is hanging
   - If yes: Background task is alive (timeout will eventually trigger)

3. **Check if chunks are arriving:**
   - Are new chunks appearing in the chat?
   - Is the timing log showing events every 10s?
   - If no chunks for 10+ seconds: Ollama has stopped responding

## Example Debug Session

```bash
$ RUST_LOG=debug ./target/release/neuro
...
2026-01-16T10:30:00Z DEBUG neuro: 🔧 [BG-TASK] Starting background task for query: 'Analiza este repositorio'
2026-01-16T10:30:00Z DEBUG neuro: 🔧 [BG-TASK] Acquired orchestrator lock at 2ms
2026-01-16T10:30:00Z DEBUG neuro: 🔧 [BG-TASK] Using Router orchestrator
2026-01-16T10:30:00Z DEBUG neuro: 🔧 [BG-TASK] Event channel set at 5ms
2026-01-16T10:30:00Z DEBUG neuro: 🔧 [BG-TASK] Calling router_orch.process() at 6ms
2026-01-16T10:30:02Z DEBUG neuro: ⏱️ [TIMING] Processing at 10s, event: Chunk(...)
2026-01-16T10:30:10Z DEBUG neuro: 🔄 [EVENT-LOOP] Iteration 126, processing_elapsed: 10s
2026-01-16T10:30:12Z DEBUG neuro: ⏱️ [TIMING] Processing at 20s, event: Chunk(...)
2026-01-16T10:30:20Z DEBUG neuro: 🔄 [EVENT-LOOP] Iteration 252, processing_elapsed: 20s
2026-01-16T10:30:22Z DEBUG neuro: ⏱️ [TIMING] Processing at 30s, event: Chunk(...)
2026-01-16T10:30:30Z DEBUG neuro: 🔄 [EVENT-LOOP] Iteration 378, processing_elapsed: 30s
2026-01-16T10:30:32Z DEBUG neuro: ⏱️ [TIMING] Processing at 40s, event: Chunk(...)
2026-01-16T10:30:40Z DEBUG neuro: 🔄 [EVENT-LOOP] Iteration 504, processing_elapsed: 40s
[FREEZE AT THIS POINT - No more logs, UI stops responding]
```

In this example:
- Event loop was running (saw 126, 252, 378, 504 iterations)
- Chunks were arriving every 10 seconds (saw timing logs)
- But then everything stopped
- This suggests the event loop or background task got stuck

## Next Steps

1. **Run the program with debug logging** (instructions above)
2. **Send the test query** that reproduces the freeze
3. **Wait for the freeze** at 43-44s (or take note of what time it happens)
4. **Save the logs** to a file and share them
5. **Describe what you see:**
   - Did the event loop keep running?
   - Did chunks keep arriving?
   - Did any error messages appear?
   - Exactly how long did it take before freezing?

## Compile Command
```bash
cargo build --release  # ~20 seconds
```

## Run Command
```bash
RUST_LOG=debug ./target/release/neuro
```

---

**Status**: Diagnostic logging added. Ready for testing.
**Expected Result**: Detailed logs showing exactly where the freeze occurs.
**Action**: Run with debug logging and report what you see in the logs.
