export type TaskRuntime = {
  session?: {
    phase?: string | null;
    model?: string | null;
  } | null;
  outcome?: {
    status?: string | null;
    summary?: string | null;
    reason?: string | null;
    next_action?: string | null;
    verification?: {
      approved?: boolean | null;
      confidence?: number | null;
      warnings?: string[];
    } | null;
  } | null;
} | null;

const RUNTIME_PHASE_LABELS: Record<string, string> = {
  prime: "Prime",
  frame: "Frame",
  act: "Act",
  verify: "Verify",
  commit: "Commit",
};

const RUNTIME_STATUS_LABELS: Record<string, string> = {
  done: "Done",
  blocked: "Blocked",
  handoff: "Handoff",
  failed: "Failed",
};

export function formatRuntimePhase(phase?: string | null): string | null {
  if (!phase) return null;
  return RUNTIME_PHASE_LABELS[phase] || phase;
}

export function formatRuntimeStatus(status?: string | null): string | null {
  if (!status) return null;
  return RUNTIME_STATUS_LABELS[status] || status;
}

export function summarizeTaskRuntime(
  runtime?: TaskRuntime,
  closedReason?: string | null,
): string | null {
  const reason = runtime?.outcome?.reason?.trim();
  if (reason) return reason;

  const summary = runtime?.outcome?.summary?.trim();
  if (summary) return summary;

  const warning = runtime?.outcome?.verification?.warnings?.find(Boolean)?.trim();
  if (warning) return warning;

  const fallback = closedReason?.trim();
  return fallback || null;
}

export function runtimeLabel(runtime?: TaskRuntime): string | null {
  const phase = formatRuntimePhase(runtime?.session?.phase);
  const status = formatRuntimeStatus(runtime?.outcome?.status);
  const parts = [phase, status].filter(Boolean);
  return parts.length > 0 ? parts.join(" • ") : null;
}
