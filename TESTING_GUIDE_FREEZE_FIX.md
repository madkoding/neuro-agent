# Testing Guide: 43-44 Second Freeze Fix

## Quick Start

### 1. Build the Latest Version
```bash
cd /home/madkoding/proyectos/neuro-agent
cargo build --release
```

### 2. Run Neuro (Screen Stays Clean)
```bash
./target/release/neuro
```

### 3. Monitor Logs in Another Terminal
```bash
# Watch logs in real-time
tail -f ~/.local/share/neuro/neuro.log

# Or filter for specific patterns:
tail -f ~/.local/share/neuro/neuro.log | grep "TIMING\|BG-TASK\|EVENT-LOOP"
```

### 4. Send a Test Query
In the Neuro app, type a query that will cause streaming:
```
Analiza este repositorio y explicame de que se trata
```

### 5. Watch for Freeze
- The query will start showing progress (1/5, 2/5, etc.) in the app
- Watch the logs for messages appearing every 10 seconds
- If freeze still occurs around 43-44 seconds, check which logs stop

## What Changed

### Code Changes Made
1. **Timeout Wrapper** - Added 120-second timeout around `router_orch.process()` call
   - If orchestrator hangs, will timeout cleanly instead of freezing UI
   - File: `src/ui/modern_app.rs`, lines 1413-1416

2. **Diagnostic Logging** - Added detailed timing logs at key points
   - Background task startup/completion with timestamps
   - Event loop responsiveness monitoring
   - Event reception tracking
   - Process execution time measurement

## How to Interpret Results

### Best Case - Everything Works
```
✅ You see logs every 10 seconds
✅ Chunks appear in chat continuously
✅ After response completes: "Background task complete"
✅ UI returns to "Listo" (Ready) state immediately
```

### Problem Case - Freeze at 43-44s
```
⚠️ Logs appear up to ~40s
⚠️ Then no more logs after that
⚠️ UI becomes unresponsive
⚠️ No "Background task complete" message
```

When this happens, share:
- What was the last log message you saw?
- Did the event loop logs continue (🔄)?
- Did chunk logs continue (⏱️)?
- Or did everything stop?

## Log Format Explanation

```
🔧 [BG-TASK] ...         → Background task execution details
🔄 [EVENT-LOOP] ...      → Event loop responsiveness check (every ~8s)
⏱️ [TIMING] ...          → Event reception timing (every 10s)
```

## Complete Testing Sequence

1. **Open terminal and navigate to project:**
   ```bash
   cd /home/madkoding/proyectos/neuro-agent
   ```

2. **Build release version:**
   ```bash
   cargo build --release
   ```

3. **Run with debug logging enabled:**
   ```bash
   RUST_LOG=debug ./target/release/neuro
   ```

4. **In the app, type the test query:**
   ```
   Analiza este repositorio y explicame de que se trata
   ```

5. **Press Enter and watch the logs**
   - Keep an eye on the terminal where the app is running
   - Note timestamps in the logs
   - Watch for:
     - Progress messages (1/5, 2/5, etc.)
     - Log messages appearing every 10 seconds
     - Final completion message

6. **When freeze occurs (if it does):**
   - Note the exact time in the logs
   - Take note of the last message you saw
   - Copy the relevant log lines

## Expected Timeline

### Normal Execution (No Freeze)
```
t=0s:   Query sent, processing starts
        🔧 [BG-TASK] Starting background task...

t=1-5s: Initial progress/classification
        Status: "🔍 Analizando consulta..."

t=5-10s: Start receiving chunks
        1/5: Listando directorio raíz...
        ⏱️ [TIMING] Processing at 10s...

t=10-20s: More chunks
        2/5: Leyendo README.md...
        ⏱️ [TIMING] Processing at 20s...

t=20-30s: Continue processing
        3/5: Leyendo Cargo.toml...
        ⏱️ [TIMING] Processing at 30s...

t=30-40s: Near completion
        4/5: Listando directorio 'src'...
        ⏱️ [TIMING] Processing at 40s...

t=40-50s: Final phase
        5/5: Generando respuesta...
        🔧 [BG-TASK] router_orch.process() returned...
        🔧 [BG-TASK] Background task complete...

UI returns to "Listo"
```

### Freeze Scenario (If Problem Still Exists)
```
[Same as above until...]

t=40-44s: Logs appear normally
        ⏱️ [TIMING] Processing at 40s...

t=44s:   ❌ FREEZE - No more logs
        ❌ UI becomes unresponsive
        ❌ Spinner stops moving
        ❌ Can't type anything
        ❌ Must press Ctrl+C to exit
```

## What Happens if Timeout Triggers

If the `router_orch.process()` call hangs beyond 120 seconds:
```
At t=120s: Timeout triggers
           🔧 [BG-TASK] router_orch.process() returned after 120000ms...
           Error message in chat: "Timeout: El procesamiento tardó más de 120 segundos"
           UI returns to "Listo"
```

This is a controlled failure, not a freeze.

## Troubleshooting

### "I don't see any logs"
- Make sure you're running with `RUST_LOG=debug`
- The binary must be from `cargo build --release`
- Logs should appear in the terminal where you ran the command

### "Logs appear but freeze still happens"
- Note exactly what time the freeze occurs
- Share the last 20-30 lines of logs before the freeze
- Describe which log categories stopped (🔧, 🔄, or ⏱️)

### "No freeze at all!"
- Great! The fix worked!
- Try a few more complex queries to confirm
- The timeout wrapper prevents indefinite hangs

## Files to Review

- `DIAGNOSTICS_FREEZE_FIX.md` - Detailed diagnostic guide
- `SESSION_SUMMARY_2.md` - Technical summary of changes
- `IMPROVEMENTS_SUMMARY.md` - Overall improvement history
- `src/ui/modern_app.rs` - Modified code (lines 1380-1441 for logging)

## Expected Outcomes

**Scenario A: Freeze is Fixed**
- Run multiple queries without freezing
- All logs appear continuously
- UI remains responsive throughout
- Response appears normally

**Scenario B: Freeze Still Occurs**
- Logs will show exactly where it stops
- Timeout wrapper prevents indefinite hang
- Can identify if it's event loop, background task, or Ollama issue

**Scenario C: Timeout Triggers**
- Processing takes >120s
- Timeout message appears in chat
- UI returns to ready state
- This is safe - prevents indefinite waiting

## Next Steps After Testing

1. **If works**: Celebrate! 🎉 The fix resolved the issue
2. **If freeze occurs**: Share logs showing:
   - Query sent
   - Last few log messages before freeze
   - Time when freeze occurred
   - Whether event loop logs continued or stopped
3. **If timeout**: Either improve Ollama performance or adjust timeout

---

**Ready to test!** Run the commands above and let me know what happens with the logs.
