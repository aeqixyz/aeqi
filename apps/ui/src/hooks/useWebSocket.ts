import { useEffect, useRef, useState, useCallback } from "react";

export interface RuntimeStepRecord {
  id: string;
  phase: string;
  summary: string;
  status: string;
  timestamp: string;
}

export interface RuntimeArtifact {
  kind: string;
  label: string;
  reference: string;
}

export interface RuntimeVerificationReport {
  checks_run: string[];
  confidence?: number | null;
  approved?: boolean | null;
  warnings: string[];
  evidence_summary: string[];
}

export interface RuntimeOutcome {
  status: string;
  summary: string;
  reason?: string | null;
  next_action?: string | null;
  artifacts: RuntimeArtifact[];
  verification?: RuntimeVerificationReport | null;
}

export interface RuntimeSession {
  session_id: string;
  task_id: string;
  worker_id: string;
  project: string;
  model?: string | null;
  status: string;
  phase: string;
  started_at: string;
  updated_at: string;
  checkpoint_refs: string[];
  steps: RuntimeStepRecord[];
}

export interface RuntimeExecution {
  session: RuntimeSession;
  outcome: RuntimeOutcome;
}

export interface WorkerEvent {
  event_type: string;
  task_id?: string;
  agent?: string;
  project?: string;
  turns?: number;
  cost_usd?: number;
  outcome?: string;
  confidence?: number;
  reason?: string;
  runtime_session?: RuntimeSession;
  runtime?: RuntimeExecution;
  [key: string]: any;
}

export function useWebSocket() {
  const [events, setEvents] = useState<WorkerEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const connect = useCallback(() => {
    const token = localStorage.getItem("aeqi_token");
    if (!token) return;

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(`${protocol}//${window.location.host}/api/ws?token=${token}`);
    wsRef.current = ws;

    ws.onopen = () => setConnected(true);

    ws.onclose = () => {
      setConnected(false);
      // Reconnect after 3 seconds
      reconnectTimer.current = setTimeout(() => connect(), 3000);
    };

    ws.onerror = () => {
      ws.close();
    };

    ws.onmessage = (e) => {
      try {
        const msg = JSON.parse(e.data);
        if (msg.event === "worker" && msg.data) {
          setEvents((prev) => [...prev.slice(-50), msg.data]); // keep last 50
        }
      } catch {
        // ignore malformed messages
      }
    };
  }, []);

  useEffect(() => {
    connect();
    return () => {
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current);
      if (wsRef.current) wsRef.current.close();
    };
  }, [connect]);

  const clearEvents = useCallback(() => setEvents([]), []);

  return { events, connected, clearEvents };
}
