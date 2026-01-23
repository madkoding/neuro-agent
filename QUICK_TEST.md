# Quick Test Instructions

## TL;DR - Just Run This

```bash
# 1. Compile
cargo build --release

# 2. Run normally (logs go to file, screen stays clean)
./target/release/neuro

# 3. In another terminal, monitor logs:
tail -f ~/.local/share/neuro/neuro.log

# 4. In the app, type:
# Analiza este repositorio y explicame de que se trata

# 5. Watch the logs
# If you see logs continuously every 10 seconds → Working!
# If logs stop at ~43-44 seconds → We found the freeze point!
```

## Where Are the Logs?

All detailed logs are automatically saved to:
```bash
~/.local/share/neuro/neuro.log
```

Monitor them in real-time with:
```bash
tail -f ~/.local/share/neuro/neuro.log
```

Filter specific patterns:
```bash
# See only timing logs
tail -f ~/.local/share/neuro/neuro.log | grep TIMING

# See only background task logs
tail -f ~/.local/share/neuro/neuro.log | grep BG-TASK

# See only event loop logs
tail -f ~/.local/share/neuro/neuro.log | grep EVENT-LOOP
```

## What to Look For

### ✅ Good Signs
- Logs appear with 🔧, 🔄, ⏱️ emojis
- Response shows progress: 1/5, 2/5, 3/5...
- Chat output appears and grows
- No freeze or UI hang

### ❌ Problem Signs
- Logs stop appearing suddenly
- UI becomes unresponsive
- Spinner stops moving
- Message doesn't complete

## If Freeze Happens

Share:
1. What time the freeze occurred (e.g., "43 seconds")
2. Last log message you saw
3. Which emoji stopped appearing (🔧, 🔄, or ⏱️)

Example:
> "Froze at 44s. Last log was '⏱️ [TIMING] Processing at 40s'. No more logs after that."

## Files to Read

- **Quick start**: This file
- **Detailed guide**: `TESTING_GUIDE_FREEZE_FIX.md`
- **Technical details**: `SESSION_SUMMARY_2.md`
- **Diagnostics**: `DIAGNOSTICS_FREEZE_FIX.md`
