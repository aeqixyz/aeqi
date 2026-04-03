interface StatusBadgeProps {
  status: string;
  size?: "sm" | "md";
}

const DISPLAY_LABELS: Record<string, string> = {
  idle: "Idle",
  working: "Working",
  offline: "Offline",
  pending: "Pending",
  in_progress: "In Progress",
  done: "Done",
  blocked: "Blocked",
  cancelled: "Cancelled",
  failed: "Failed",
};

export default function StatusBadge({ status, size = "md" }: StatusBadgeProps) {
  return (
    <span className={`status-badge status-badge-${size} status-badge-${status}`}>
      <span className="status-dot" />
      {DISPLAY_LABELS[status] || status}
    </span>
  );
}
