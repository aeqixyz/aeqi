import type { Checkpoint, AuditEntry } from "./types";

export type TimelineEventType =
  | "message"
  | "task_created"
  | "quest_started"
  | "task_checkpoint"
  | "task_blocked"
  | "quest_completed"
  | "task_cancelled"
  | "audit";

export interface TimelineItem {
  id: string;
  type: TimelineEventType;
  timestamp: string;
  summary?: string;
  agent?: string;
  // Message fields
  role?: string;
  content?: string;
  // Task fields
  taskId?: string;
  taskSubject?: string;
  taskStatus?: string;
  checkpoint?: Checkpoint;
  // Audit fields
  auditEntry?: AuditEntry;
}

export function checkpointsToTimeline(checkpoints: Checkpoint[], taskId: string): TimelineItem[] {
  return checkpoints.map((cp, i) => ({
    id: `cp-${taskId}-${i}`,
    type: "task_checkpoint" as const,
    timestamp: cp.timestamp,
    summary: cp.progress,
    agent: cp.worker,
    taskId,
    checkpoint: cp,
  }));
}

export function auditToTimeline(entries: AuditEntry[]): TimelineItem[] {
  return entries.map((e) => {
    let type: TimelineEventType = "audit";
    const dt = (e.decision_type || "").toLowerCase();
    if (dt.includes("task_created") || dt.includes("create_task")) type = "task_created";
    else if (dt.includes("task_started") || dt.includes("start_task")) type = "quest_started";
    else if (dt.includes("task_completed") || dt.includes("complete_task") || dt.includes("close_task")) type = "quest_completed";
    else if (dt.includes("task_blocked") || dt.includes("block_task")) type = "task_blocked";
    else if (dt.includes("task_cancelled") || dt.includes("cancel_task")) type = "task_cancelled";

    return {
      id: `audit-${e.id}`,
      type,
      timestamp: e.timestamp,
      summary: e.summary,
      agent: e.agent,
      taskId: e.task_id,
      auditEntry: e,
    };
  });
}

export function mergeTimelines(...timelines: TimelineItem[][]): TimelineItem[] {
  return timelines.flat().sort((a, b) => {
    const ta = new Date(a.timestamp).getTime();
    const tb = new Date(b.timestamp).getTime();
    return ta - tb;
  });
}
