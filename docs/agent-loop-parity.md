# Agent Loop Parity: AEQI vs Claude Code

## Goal

Make AEQI's agent loop (`aeqi-core/src/agent.rs`) at least as resilient, performant, and polished as Claude Code's (`refs/claude-code/src/query.ts`). The loop is the core product — everything else is orchestration on top.

## Status After This Session (2026-04-04)

### What Was Done Today (13 commits, 228 files)

**Agent loop:**
- True streaming tool execution (tools start during LLM stream via ToolUseComplete)
- All 13 ChatStreamEvent types emitted and forwarded to frontend
- Tool input_preview in ToolComplete (shows what was called with)
- Diminishing returns threshold relaxed (50 tok × 5 turns, was 500 × 3)
- Status events before compaction and memory recall
- DelegateStart/DelegateComplete event emission

**Data model:**
- Projects have UUIDs (auto-generated, persisted)
- Three-tier memory (agent → department → project with hierarchical_search)
- Sessions unified — one table, parent-child linked
- Everything creates a session (workers, delegates, triggers)
- SessionManager.spawn_session() as universal executor
- SpawnOptions builder (flat params, skills, auto_close)

**Architecture:**
- ChatEngine → MessageRouter
- UnifiedDelegateTool → DelegateTool (direct spawn, no dispatch bus)
- unified_delegate.rs → delegate.rs
- chat_ws.rs → session_ws.rs
- .company → .project on all DB structs
- Skill injection at spawn time
- architecture-audit skill
- Full web UI with interleaved segments, tool panels, session sidebar

### What AEQI's Agent Loop Does Well
- Streaming tool execution (ToolUseComplete → executor starts during stream)
- Context management: snip, microcompact, full compact, reactive compact
- Three-tier memory recall (hierarchical_search)
- Perpetual sessions (stays open, accepts follow-up messages)
- Token budget auto-continuation
- Output truncation recovery (MaxTokens → auto-continue)
- Fallback model switching on consecutive failures
- File change detection between turns
- Mid-loop memory recall
- Session memory extraction (fire-and-forget background task)
- Budget pressure injection into tool results (70% and 90% warnings)
- Diminishing returns detection

### Where Claude Code's Loop is Better

See Phase 3 comparison below — gaps identified in 12 areas.

### Where AEQI's Loop is Better Than CC

1. **Three-tier hierarchical memory** — agent→department→project scoped memory with `hierarchical_search`. CC has flat memory prefetch.
2. **Mid-loop memory recall** — proactively re-queries memory when tool output has novel terms (>200 chars, 3+ new terms). CC doesn't recall mid-loop.
3. **Session memory extraction** — fire-and-forget background task extracting structured insights (SCOPE CATEGORY: key | content) into domain-scoped memory at 50K+ prompt tokens.
4. **Perpetual sessions** — stays open via `input_rx` channel, accepts follow-up messages with full state reset per turn.
5. **Token budget auto-continuation** — built-in with percentage tracking, nudge messages showing "X% of budget used", stops at 90%.
6. **Budget pressure injection** — injects warnings into tool results at 70% and 90% of iteration budget. CC has no equivalent.
7. **Observer trait richness** — 16 hooks including `collect_attachments`, `pre_compact`/`post_compact`, `file_changed`, `user_prompt_submit`. CC's hooks are more user-facing but less programmatically extensible.
8. **Diminishing returns detection** — 50 tokens × 5 consecutive turns threshold. CC has this behind a feature flag (TOKEN_BUDGET).
9. **Structured 9-section compaction** — detailed prompt with `<analysis>` scratchpad + `<summary>` extraction, custom `compact_instructions` per project.
10. **File change detection between turns** — checks mtime of recently-read files, injects system reminders to re-read before editing.
11. **Smart model routing** — uses cheap `routing_model` for simple messages on iteration 1.
12. **Post-run reflection** — `reflect()` extracts up to 5 structured insights with scope/category/key/content into memory.

## Phase 3: Side-by-Side Comparison (Completed 2026-04-04)

### 1. Streaming Tool Execution During LLM Response

| | Claude Code | AEQI |
|---|---|---|
| **Trigger** | `content_block_stop` → `addTool()` → `processQueue()` | `ToolUseComplete` → `add_tool()` → `try_start_queued()` |
| **Concurrency** | `isConcurrencySafe(input)` per tool, mutual exclusion | `is_concurrent_safe(input)` per tool, same model |
| **Error cascade** | Bash-only: `sibling_error` aborts siblings via child AbortController | `sibling_errored` Arc<Mutex<bool>> — all error types signal |
| **Progress** | `pendingProgress[]` yielded immediately via Promise.race | `ToolProgress` event emitted during execution |

**Gap**: Minimal. Both start tools mid-stream with same timing. **One difference**: CC only cascades errors from Bash (implicit dependency chains), AEQI cascades from any tool error. CC's approach is more precise — read/fetch failures shouldn't kill sibling tools.

**Fix**: Change AEQI's sibling error signaling to only cascade from shell/bash tools. Low effort.

### 2. Error Recovery: Prompt Too Long

| | Claude Code | AEQI |
|---|---|---|
| **Detection** | API returns 413, withheld from UI | API returns context-length error string match |
| **Step 1** | Context collapse drain (commit staged collapses) | — |
| **Step 2** | Reactive compact (full LLM summarization) | Emergency compact (snip+microcompact+full compact) |
| **Step 3** | Surface withheld error + return `prompt_too_long` | Break with `ContextExhausted` or `ApiError` |
| **Error withholding** | Yes — recoverable errors hidden from UI until recovery fails | No — errors surface immediately |
| **Single-shot guard** | `hasAttemptedReactiveCompact` flag | Checks compaction count vs `MAX_COMPACTIONS_PER_RUN` (3) |

**Gap**: 
- No error withholding — AEQI exposes intermediate errors to frontend observers even when recovery succeeds
- No context collapse (cheap drain step before expensive full compact)
- AEQI's reactive compact is effective but lacks the cheaper first-pass

**Fix (P0)**:
1. Add error withholding: on context-length error, suppress observer notification, attempt recovery, only surface if recovery fails
2. Context collapse is a larger feature — defer to P1

### 3. Error Recovery: Max Output Tokens

| | Claude Code | AEQI |
|---|---|---|
| **Step 1** | Escalate from 8K default to 64K `ESCALATED_MAX_TOKENS` | — |
| **Step 2** | Multi-turn: inject "resume mid-thought, break into smaller pieces" | Auto-continue with continuation prompt, `OutputTruncated { attempt }` |
| **Circuit breaker** | `MAX_OUTPUT_TOKENS_RECOVERY_LIMIT = 3` | `output_recovery_count` (configurable) |
| **Reset** | Counter resets on normal next-turn and stop-hook-blocking continues | Resets per-run |

**Gap**: CC has an escalation step (try higher limit before going multi-turn). AEQI goes straight to continuation. Both are valid strategies.

**Fix (P2)**: Consider adding escalation step — try `max_tokens * 4` (capped at model max) before falling to continuation. Low priority since AEQI's continuation approach works well.

### 4. Error Recovery: Streaming Fallback

| | Claude Code | AEQI |
|---|---|---|
| **Mid-stream fallback** | Tombstone orphaned messages, clear all buffers atomically, discard executor, continue from fallback response | Not implemented |
| **Model fallback** | `FallbackTriggeredError` → switch model, strip signature blocks, clear buffers, discard executor, retry inner loop | After `FALLBACK_TRIGGER_COUNT` (3) consecutive failures → switch to `fallback_model`, decrement iteration, continue |
| **Buffer clearing** | `assistantMessages.length=0`, `toolResults.length=0`, `toolUseBlocks.length=0`, `needsFollowUp=false` | Executor `discard()` aborts handles + cancels queued |
| **Signature handling** | Strips thinking signature blocks (model-bound, would 400 on different model) | N/A (no extended thinking signatures) |

**Gap (P0)**:
- No mid-stream fallback with tombstoning. If the provider switches models mid-stream, orphaned partial messages could corrupt the conversation.
- No atomic buffer clearing when fallback triggers. AEQI's executor `discard()` handles tool cleanup but not message-level cleanup.
- Fallback trigger is based on consecutive failures (reactive) rather than specific error types (proactive).

**Fix (P0)**:
1. Add tombstone support: when `call_streaming_with_tools()` detects a fallback, mark any partial assistant messages as tombstoned (remove from history, emit tombstone event)
2. Clear accumulated text/tool state on fallback before retry
3. Consider provider-level fallback signals (specific error types that trigger immediate fallback rather than waiting for 3 failures)

### 5. Context Compaction: Levels and Triggers

| | Claude Code | AEQI |
|---|---|---|
| **Order** | Microcompact → Snip → Context Collapse → Auto-compact → Reactive | Snip (85%) → Microcompact (85%) → Full compact (80%) → Reactive (on error) |
| **Microcompact trigger** | Time-based (cache expired) OR count-based (cached path) | Token threshold (85% of compact threshold) |
| **Snip** | Removes old rounds | Removes oldest assistant+tool rounds from compactable window |
| **Full compact** | LLM summarization with file+skill re-injection | 9-section LLM summarization with file restoration + skill preservation |
| **Reactive** | Triggered by API 413, runs full compaction | Triggered by context-length error, runs snip+microcompact+full compact |
| **Context Collapse** | Persistent commit log, 90% commit / 95% block thresholds | Not implemented |
| **Circuit breaker** | 3 consecutive autocompact failures → stop | `MAX_COMPACTIONS_PER_RUN = 3` hard cap |

**Gap**: 
- No context collapse system (persistent structured log that can be cheaply drained before expensive full compact)
- CC's microcompact has prompt-cache-aware path (`cache_edits` API) — AEQI's is simpler replacement

**Fix (P1)**: Context collapse is architecturally significant. Defer. AEQI's 3-stage pipeline is solid for current needs.

### 6. Context Compaction: Post-Compact Restoration

| | Claude Code | AEQI |
|---|---|---|
| **Files** | Re-reads via cache clearing + forced CLAUDE.md re-read | Restores from `recent_files` tracking (5 files, 5K tokens each, 50K budget) |
| **Skills** | Clears invoked-skill-names so skills re-inject on next turn | Preserves skill messages verbatim in compacted output |
| **State clearing** | Resets microcompact, context collapse, getUserContext cache, system prompt sections, classifier approvals | Resets replacement_state for microcompacted entries |
| **Tool pairing** | Invariant maintained at error boundaries (4 sites) | `repair_tool_pairing()` post-compact: injects synthetic results for dangling tool_use, strips orphan tool_results |

**Gap**: Minimal. AEQI's approach of preserving skills verbatim is arguably better than CC's clear-and-re-inject. AEQI's explicit `repair_tool_pairing()` is more thorough post-compact.

**Fix**: None needed. AEQI's post-compact restoration is at parity or better.

### 7. Stop Hooks / Post-Turn Validation

| | Claude Code | AEQI |
|---|---|---|
| **Mechanism** | Shell commands in `settings.json` via `executeStopHooks()` | `Observer.after_turn()` — Rust trait returning `LoopAction` |
| **Blocking** | `blockingErrors[]` fed back to model for fixing | `Inject(Vec<String>)` — messages force continuation |
| **Prevention** | `preventContinuation: true` stops loop entirely | `Halt(String)` stops loop |
| **Fire-and-forget** | Memory extraction, prompt suggestion, auto-dream | Session memory extraction (fire-and-forget tokio::spawn) |
| **User-configurable** | Yes — shell commands in JSON settings | No — requires Rust code |

**Gap**: AEQI's `after_turn` is more powerful programmatically (can inject arbitrary messages, has full context) but not user-configurable. CC's shell-command hooks are accessible to end users.

**Fix (P2)**: Add configurable shell-command hooks to AEQI's observer system. Could implement as a `ShellHookObserver` that reads hook definitions from project config and executes them.

### 8. Prefetching During Streaming

| | Claude Code | AEQI |
|---|---|---|
| **Memory** | Started once at loop entry, polled after tools | Not prefetched — mid-loop recall happens after tools |
| **Skill discovery** | Started per-iteration during streaming, consumed post-tools | Skills injected at spawn time, not discovered mid-session |
| **Tool use summary** | Haiku generates summary after tools, consumed next iteration during streaming | Not implemented |

**Gap (P1)**:
- Memory recall could start during LLM streaming rather than after tools. The search query is known (user prompt + recent context). This would overlap 100-500ms of memory search with 5-30s of model generation.
- Tool use summaries would help with long sessions — a cheap model summarizes what tools did, reducing context pressure.

**Fix (P1)**:
1. Start `hierarchical_search` during streaming (spawn task when stream begins, await after tools). Requires refactoring mid-loop recall to also consume prefetch results.
2. Tool use summaries: spawn cheap model call after tool batch, consume result at start of next iteration's post-streaming phase. Lower priority.

### 9. Tool Result Budget Enforcement

| | Claude Code | AEQI |
|---|---|---|
| **Per-tool limit** | `maxResultSizeChars` on Tool definition, `Infinity` opts out | `DEFAULT_MAX_TOOL_RESULT_CHARS` (50K), per-tool override via `max_result_size_chars()` |
| **Aggregate limit** | `contentReplacementState` tracks replacements | `DEFAULT_MAX_TOOL_RESULTS_PER_TURN` (200K), `enforce_result_budget()` truncates largest first |
| **Persistence** | Persist to disk with preview | Persist to disk with `PERSIST_PREVIEW_SIZE` (2K) preview, fallback to head+tail truncation |
| **Tracking** | `ContentReplacementState` per-conversation | `ContentReplacementState` HashMap prevents double-processing |

**Gap**: None. AEQI has MORE granularity (aggregate per-turn budget + per-tool override + budget pressure injection). At parity or better.

### 10. Conversation Repair

| | Claude Code | AEQI |
|---|---|---|
| **Post-compact** | Not explicit (relies on invariant maintenance) | `repair_tool_pairing()`: injects synthetic results for dangling tool_use, strips orphan tool_results |
| **On API error** | `yieldMissingToolResultBlocks()` — 4 call sites | Executor `discard()` cancels queued, but no explicit orphan repair on API errors |
| **On abort** | Synthetic tool_results via executor `getRemainingResults()` | Executor abort + cancel, but unclear if synthetic results generated |
| **On fallback** | Orphaned tool_use blocks get error tool_results before retry | Not handled (see gap #4) |

**Gap**: AEQI's post-compact repair is good. But it may miss orphaned tool_use blocks at error/abort/fallback boundaries.

**Fix (P0)**: Add `yield_missing_tool_results()` equivalent. After any error or abort in `try_streaming_with_tools()`, scan assistant messages for tool_use blocks without matching tool_results and inject synthetic error results. Critical for conversation integrity.

### 11. Worktree Isolation for Subagents

| | Claude Code | AEQI |
|---|---|---|
| **Mechanism** | `EnterWorktree`/`ExitWorktree` tools, creates temp git worktree, transparent CWD switch | Not implemented |
| **Use case** | Parallel subagents can't conflict on file writes | Subagents share working directory |

**Gap**: Missing entirely. When multiple delegates run in parallel on the same project, they can conflict on file operations.

**Fix (P1)**: Implement worktree isolation in `DelegateTool`. When spawning a delegate with `isolation: "worktree"`, create a git worktree, set the delegate's CWD, merge changes back on completion. Requires git operations and CWD management in `spawn_session()`.

### 12. Permission Model in the Loop

| | Claude Code | AEQI |
|---|---|---|
| **Architecture** | 10-step pipeline: deny→ask→tool-specific→safety→bypass→always-allow→classifier | `observer.before_tool()` returns `LoopAction` |
| **User-facing** | Settings-based rules, interactive prompts, ML classifier for auto-mode | Programmatic only (Rust observer trait) |
| **Safety checks** | `.git/`, `.claude/`, shell configs — bypass-immune | No built-in safety checks |

**Gap**: AEQI has no user-facing permission system. The `before_tool` hook enables programmatic control but there's no interactive permission prompt, no deny/allow rules, no safety-sensitive path detection.

**Fix (P2)**: Not critical for current use (solo developer, all projects are mine). Add safety-sensitive path detection as a quick win. Full permission system is P3.

## Phase 4: Implementation Priority (Completed 2026-04-04)

### P0 — Resilience (Do First)

| # | Fix | Effort | Impact |
|---|---|---|---|
| P0-1 | **Error withholding**: suppress observer notification on recoverable errors (context-length, max-tokens), only surface if recovery fails | Small | Prevents frontend confusion during recovery |
| P0-2 | **Conversation repair at error boundaries**: add `yield_missing_tool_results()` after API errors, aborts, and fallback switches | Medium | Prevents corrupted tool_use/tool_result pairing |
| P0-3 | **Streaming fallback atomicity**: tombstone partial messages, clear accumulated state, discard executor on mid-stream or model fallback | Medium | Prevents conversation corruption on fallback |
| P0-4 | **Bash-only error cascading**: change sibling error signal to only fire from shell/bash tool errors | Small | Prevents unnecessary tool cancellation |

### P1 — Performance

| # | Fix | Effort | Impact |
|---|---|---|---|
| P1-1 | **Memory prefetch during streaming**: spawn `hierarchical_search` when stream starts, consume after tools | Small | Overlaps 100-500ms memory latency with model generation |
| P1-2 | **Worktree isolation for delegates**: git worktree create/cleanup in spawn_session | Large | Enables safe parallel delegate execution |
| P1-3 | **Context collapse system**: persistent structured log with cheap drain before full compact | Large | Cheaper recovery from prompt-too-long errors |

### P2 — UX

| # | Fix | Effort | Impact |
|---|---|---|---|
| P2-1 | **Max output token escalation**: try `max_tokens * 4` before falling to continuation | Small | May avoid multi-turn continuation overhead |
| P2-2 | **Shell-command stop hooks**: `ShellHookObserver` reads hook defs from project config | Medium | User-configurable post-turn validation |
| P2-3 | **Safety-sensitive path detection**: warn before modifying .git/, config files | Small | Basic safety for non-solo use |

### P3 — Polish

| # | Fix | Effort | Impact |
|---|---|---|---|
| P3-1 | **Tool use summaries**: cheap model summarizes tool batch, consumed next iteration | Medium | Reduces context pressure in long sessions |
| P3-2 | **Full permission system**: rules-based deny/ask/allow with interactive prompts | Large | Required for multi-user deployment |
| P3-3 | **Prompt-cache-aware microcompact**: `cache_edits` API integration | Medium | Reduces cache invalidation on microcompact |

## Research Plan

### Phase 1: Deep Read Claude Code (query.ts ecosystem)

Read these files END TO END, not grep:

1. **`refs/claude-code/src/query.ts`** (~1729 lines)
   - The main agent loop. Every state transition, every recovery path.
   - Focus on: how `State` object carries context between iterations, the 7 continue sites, what `transition.reason` tracks
   - Map every error recovery strategy: context collapse drain, reactive compact, max output tokens retry, streaming fallback, model fallback

2. **`refs/claude-code/src/services/tools/StreamingToolExecutor.ts`**
   - How tools start during streaming (not after)
   - Concurrency model: read-safe vs exclusive
   - Bash error cascading vs non-bash independence
   - Progress message yielding
   - Discard pattern on streaming fallback

3. **`refs/claude-code/src/services/tools/toolOrchestration.ts`**
   - Batch execution (legacy path)
   - Partition logic: consecutive read-only → batch, non-read-only → sequential
   - Context modifier queuing during concurrent batches

4. **`refs/claude-code/src/services/compact/autoCompact.ts`**
   - Threshold calculation (effective window - 13K buffer)
   - Proactive vs reactive trigger
   - Circuit breaker (3 consecutive failures → stop)
   - Post-compact: file restoration, skill re-injection

5. **`refs/claude-code/src/services/compact/microCompact.ts`**
   - Time-based and token-based clearing
   - Which tool results are eligible
   - Prompt caching integration (cache_edits)

6. **`refs/claude-code/src/services/compact/contextCollapse.ts`** (if exists)
   - Persistent commit log across turns
   - Committed vs uncommitted blocks
   - How it survives across messages in long sessions

7. **`refs/claude-code/src/query/stopHooks.ts`**
   - Post-tool-batch validation
   - Can block continuation
   - Fire-and-forget vs blocking hooks
   - Hook types and when they run

8. **Error withholding pattern** (in query.ts)
   - Recoverable errors (prompt_too_long, max_output_tokens, media size)
   - Withheld from UI until recovery attempted
   - How withheld messages are managed if recovery fails

9. **Streaming fallback atomicity** (in query.ts)
   - When model fallback occurs mid-stream
   - Tombstoning orphaned partial messages
   - Buffer clearing (assistantMessages, toolResults, toolUseBlocks)
   - Executor discard + recreation

10. **Token budget system** (in query.ts)
    - Server-side task_budget parameter
    - Carryover across compactions
    - Nudge messages at threshold

### Phase 2: Deep Read AEQI (agent.rs ecosystem)

Read these files END TO END:

1. **`crates/aeqi-core/src/agent.rs`** (~3100 lines)
   - The full `run()` method and main loop
   - Every `LoopTransition` variant and what triggers each
   - Error handling: context-length, fallback model, observer on_error
   - `try_streaming_with_tools()` — the streaming call + tool execution
   - `call_streaming_with_tools()` — retry wrapper
   - Context management: snip_compact, microcompact, compact_messages
   - Result processing: observer hooks, persist/truncate, budget injection
   - Mid-loop memory recall
   - File change detection
   - Session memory extraction
   - Output truncation recovery
   - Token budget auto-continuation
   - Perpetual mode input waiting
   - format_tool_input() for input_preview

2. **`crates/aeqi-core/src/streaming_executor.rs`** (~500 lines)
   - StreamingToolExecutor queue management
   - Concurrent-safe checks
   - Sibling error signaling
   - Duration tracking
   - Discard + abort

3. **`crates/aeqi-core/src/chat_stream.rs`**
   - All 13 ChatStreamEvent variants
   - ChatStreamSender broadcast mechanics

4. **`crates/aeqi-core/src/traits/`**
   - Provider trait (chat + chat_stream)
   - Observer trait (before_model, after_model, before_tool, after_tool, on_error, etc.)
   - Memory trait (store, search, hierarchical_search)
   - Tool trait (execute, is_concurrent_safe)

### Phase 3: Side-by-Side Comparison

For each of these concerns, document CC's exact mechanism, AEQI's exact mechanism, the gap, and the concrete fix:

1. **Streaming tool execution during LLM response**
   - CC: Tools start as `content_block_stop` fires for each tool_use block
   - AEQI: Tools start on ToolUseComplete (same timing after today's work?)
   - Gap: ?

2. **Error recovery: prompt too long**
   - CC: Withhold error → context collapse drain → reactive compact → fail
   - AEQI: Reactive compact → retry. No withholding, no collapse drain.
   - Gap: No error withholding, no context collapse persistent log

3. **Error recovery: max output tokens**
   - CC: Withhold → reduce max_tokens by 50% → retry (circuit breaker at 3)
   - AEQI: Auto-continue with "Continue executing" prompt (max 3)
   - Gap: Different strategy. CC truncates, AEQI continues. Which is better?

4. **Error recovery: streaming fallback**
   - CC: Tombstone orphans, clear buffers atomically, discard executor, retry with fallback model
   - AEQI: Fallback model switch on 3 consecutive failures
   - Gap: No atomic retry with buffer clearing. No tombstoning.

5. **Context compaction: levels and triggers**
   - CC: Microcompact → snip → context collapse → auto-compact → reactive
   - AEQI: Snip → microcompact → full compact → reactive
   - Gap: No persistent context collapse log. Different ordering.

6. **Context compaction: post-compact restoration**
   - CC: Re-inject files + skills after compaction
   - AEQI: Re-inject recent files via recent_files tracking
   - Gap: Skills not re-injected after compaction?

7. **Stop hooks / post-turn validation**
   - CC: Shell-command hooks in settings.json, can block continuation
   - AEQI: Observer.after_turn() — Rust trait, not user-configurable
   - Gap: No user-configurable post-turn hooks

8. **Prefetching during streaming**
   - CC: Memory prefetch + skill discovery start during LLM streaming, consumed post-tools
   - AEQI: Memory recall happens after tools, not during
   - Gap: No prefetching during stream

9. **Tool result budget enforcement**
   - CC: Per-tool output size limits? (need to verify)
   - AEQI: max_tool_result_chars (50K), persist/truncate for oversized, aggregate budget per turn
   - Gap: ?

10. **Conversation repair**
    - CC: Tool_use/tool_result pairing invariant enforced
    - AEQI: repair_tool_pairing() after compaction
    - Gap: Same pattern? Need to compare.

11. **Worktree isolation for subagents**
    - CC: EnterWorktree/ExitWorktree tools, transparent CWD switch
    - AEQI: Not implemented
    - Gap: Missing entirely

12. **Permission model in the loop**
    - CC: before_tool hook checks permissions, can block/allow/ask
    - AEQI: observer.before_tool() can Halt, but no permission system
    - Gap: No permission system

### Phase 4: Implementation Priority

After the comparison, rank fixes by impact:

**P0 (Resilience):** Error recovery, streaming fallback, context collapse persistence
**P1 (Performance):** Prefetching during stream, worktree isolation
**P2 (UX):** Permission model, user-configurable hooks, stop hooks
**P3 (Polish):** Post-compact skill re-injection, token budget carryover

## Key Files Reference

### Claude Code
```
refs/claude-code/src/query.ts                          — Main agent loop (1729 lines)
refs/claude-code/src/services/tools/StreamingToolExecutor.ts  — Streaming tool scheduler
refs/claude-code/src/services/tools/toolOrchestration.ts      — Batch tool execution
refs/claude-code/src/services/tools/toolExecution.ts          — Tool execution engine
refs/claude-code/src/services/compact/autoCompact.ts          — Full compaction
refs/claude-code/src/services/compact/microCompact.ts         — Incremental compaction
refs/claude-code/src/query/stopHooks.ts                       — Post-turn hooks
refs/claude-code/src/utils/permissions/permissions.ts         — Permission system
refs/claude-code/src/tools/EnterWorktreeTool/                 — Worktree isolation
refs/claude-code/src/Tool.ts                                  — Tool interface
```

### AEQI
```
crates/aeqi-core/src/agent.rs              — Main agent loop (~3100 lines)
crates/aeqi-core/src/streaming_executor.rs — Tool executor (~500 lines)
crates/aeqi-core/src/chat_stream.rs        — Event types
crates/aeqi-core/src/traits/               — Provider, Observer, Memory, Tool traits
crates/aeqi-core/src/config.rs             — Agent + project config
crates/aeqi-core/src/identity.rs           — Agent identity (persona, knowledge, memory)
crates/aeqi-orchestrator/src/session_manager.rs — spawn_session (universal executor)
crates/aeqi-orchestrator/src/delegate.rs        — Delegation tool
crates/aeqi-orchestrator/src/middleware/        — Middleware chain (9 layers)
```

## How to Use This Document

In a fresh Claude Code session:

1. "Read `/home/claudedev/aeqi/docs/agent-loop-parity.md` for context"
2. "Execute Phase 1: deep read Claude Code's query.ts and related files"
3. "Execute Phase 2: deep read AEQI's agent.rs"
4. "Execute Phase 3: produce the side-by-side comparison"
5. "Execute Phase 4: implement P0 fixes"

Each phase should be a focused research block — read entire files, don't grep. The goal is UNDERSTANDING, not pattern matching.
