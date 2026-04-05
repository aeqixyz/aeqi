const BASE_URL = import.meta.env.VITE_API_URL || "/api";

class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

function getToken(): string | null {
  return localStorage.getItem("aeqi_token");
}

async function parseResponseBody(res: Response): Promise<any> {
  const contentType = res.headers.get("content-type") || "";
  if (!contentType.includes("application/json")) {
    return null;
  }

  try {
    return await res.json();
  } catch {
    return null;
  }
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const url = `${BASE_URL}${path}`;
  const token = getToken();
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(options?.headers as Record<string, string>),
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const res = await fetch(url, { ...options, headers });
  const body = await parseResponseBody(res);

  if (res.status === 401) {
    localStorage.removeItem("aeqi_token");
    window.location.href = "/login";
    throw new ApiError(401, "Unauthorized");
  }

  if (!res.ok) {
    const message =
      body?.error ||
      body?.message ||
      `API error: ${res.statusText}`;
    throw new ApiError(res.status, message);
  }

  return body as T;
}

export const api = {
  // Auth
  login: (secret: string) =>
    request<{ ok: boolean; token: string }>("/auth/login", {
      method: "POST",
      body: JSON.stringify({ secret }),
    }),

  // Dashboard
  getDashboard: () => request<any>("/dashboard"),

  // Departments
  getDepartments: () => request<any>("/departments"),

  // Status
  getStatus: () => request<any>("/status"),

  // Worker events
  getWorkerEvents: (params?: { cursor?: number }) => {
    const query = new URLSearchParams();
    if (params?.cursor != null) query.set("cursor", String(params.cursor));
    const qs = query.toString();
    return request<any>(`/worker/events${qs ? `?${qs}` : ""}`);
  },

  // Companies
  getCompanies: () => request<any>("/companies"),
  createCompany: (data: { name: string }) =>
    request<any>("/companies", { method: "POST", body: JSON.stringify(data) }),

  // Tasks
  getTasks: (params?: { status?: string; company?: string }) => {
    const query = new URLSearchParams();
    if (params?.status) query.set("status", params.status);
    if (params?.company) query.set("company", params.company);
    const qs = query.toString();
    return request<any>(`/tasks${qs ? `?${qs}` : ""}`);
  },

  // Missions
  getMissions: (params?: { company?: string }) => {
    const query = new URLSearchParams();
    if (params?.company) query.set("company", params.company);
    const qs = query.toString();
    return request<any>(`/missions${qs ? `?${qs}` : ""}`);
  },

  // Agents
  getAgents: () => request<any>("/agents/registry"),

  // Audit
  getAudit: (params?: { last?: number; company?: string }) => {
    const query = new URLSearchParams();
    if (params?.last) query.set("last", String(params.last));
    if (params?.company) query.set("company", params.company);
    const qs = query.toString();
    return request<any>(`/audit${qs ? `?${qs}` : ""}`);
  },

  // Notes
  getNotes: (params?: { company?: string; limit?: number }) => {
    const query = new URLSearchParams();
    if (params?.company) query.set("company", params.company);
    if (params?.limit) query.set("limit", String(params.limit));
    const qs = query.toString();
    return request<any>(`/notes${qs ? `?${qs}` : ""}`);
  },

  // Expertise
  getExpertise: (domain?: string) => {
    const query = new URLSearchParams();
    if (domain) query.set("domain", domain);
    const qs = query.toString();
    return request<any>(`/expertise${qs ? `?${qs}` : ""}`);
  },

  // Cost
  getCost: () => request<any>("/cost"),

  // Brief
  getBrief: () => request<any>("/brief"),

  // Memories
  getMemories: (params?: { company?: string; query?: string; limit?: number }) => {
    const q = new URLSearchParams();
    if (params?.company) q.set("company", params.company);
    if (params?.query) q.set("query", params.query);
    if (params?.limit) q.set("limit", String(params.limit));
    const qs = q.toString();
    return request<any>(`/memories${qs ? `?${qs}` : ""}`);
  },

  // Skills
  getSkills: () => request<any>("/skills"),

  // Pipelines
  getPipelines: () => request<any>("/pipelines"),

  // Company Knowledge
  getCompanyKnowledge: (name: string) => request<any>(`/companies/${name}/knowledge`),

  // Knowledge CRUD
  storeKnowledge: (data: { company: string; key: string; content: string; category?: string; scope?: string }) =>
    request<any>("/knowledge/store", { method: "POST", body: JSON.stringify(data) }),

  deleteKnowledge: (data: { company: string; id: string }) =>
    request<any>("/knowledge/delete", { method: "POST", body: JSON.stringify(data) }),

  // Channel Knowledge
  getChannelKnowledge: (params: { company: string; query?: string; limit?: number }) => {
    const q = new URLSearchParams();
    q.set("company", params.company);
    if (params.query) q.set("query", params.query);
    if (params.limit) q.set("limit", String(params.limit));
    return request<any>(`/knowledge/channel?${q.toString()}`);
  },

  // Agent Identity
  getAgentIdentity: (name: string) => request<any>(`/agents/${name}/identity`),
  getAgentPrompts: (name: string) => request<any>(`/agents/${name}/prompts`),
  saveAgentFile: (name: string, filename: string, content: string) =>
    request<any>(`/agents/${name}/files`, {
      method: "POST",
      body: JSON.stringify({ filename, content }),
    }),

  // Rate Limit
  getRateLimit: () => request<any>("/rate-limit"),

  // Crons & Watchdogs
  getCrons: () => request<any>("/crons"),
  getWatchdogs: () => request<any>("/watchdogs"),

  // Health
  getHealth: () => request<any>("/health"),

  // Chat — canonical path
  chatFull: (params: {
    message: string;
    company?: string | null;
    department?: string | null;
    channelName?: string | null;
    chatId?: number;
    sender?: string;
  }) =>
    request<any>("/chat/full", {
      method: "POST",
      body: JSON.stringify({
        message: params.message,
        ...(params.company ? { company: params.company } : {}),
        ...(params.department ? { department: params.department } : {}),
        ...(params.channelName ? { channel_name: params.channelName } : {}),
        ...(params.chatId ? { chat_id: params.chatId } : {}),
        ...(params.sender ? { sender: params.sender } : {}),
      }),
    }),

  // Chat — typed thread timeline
  chatTimeline: (params?: {
    chatId?: number;
    company?: string | null;
    department?: string | null;
    channelName?: string | null;
    limit?: number;
  }) => {
    const query = new URLSearchParams();
    if (params?.chatId) query.set("chat_id", String(params.chatId));
    if (params?.company) query.set("company", params.company);
    if (params?.department) query.set("department", params.department);
    if (params?.channelName) query.set("channel_name", params.channelName);
    if (params?.limit) query.set("limit", String(params.limit));
    const qs = query.toString();
    return request<any>(`/chat/timeline${qs ? `?${qs}` : ""}`);
  },

  // Write: Create Task
  createTask: (data: { company: string; subject: string; description?: string }) =>
    request<any>("/tasks", {
      method: "POST",
      body: JSON.stringify(data),
    }),

  // Write: Close Task
  closeTask: (id: string, data?: { reason?: string; company?: string }) =>
    request<any>(`/tasks/${id}/close`, {
      method: "POST",
      body: JSON.stringify(data || {}),
    }),

  // Write: Post Note
  postNote: (data: { company: string; key: string; content: string; tags?: string[]; durability?: string }) =>
    request<any>("/notes", {
      method: "POST",
      body: JSON.stringify(data),
    }),

  // Single task
  getTask: (id: string) => request<any>(`/tasks/${id}`),

  // Audit filtered by task (client-side filter)
  getAuditForTask: async (taskId: string, last = 50) => {
    const data = await request<any>(`/audit?last=${last}`);
    const entries = (data.entries || data.audit || []).filter(
      (e: any) => e.task_id === taskId
    );
    return { entries };
  },

  // Sessions
  getSessions: (agentId?: string) => {
    const q = new URLSearchParams();
    if (agentId) q.set("agent_id", agentId);
    const qs = q.toString();
    return request<any>(`/sessions${qs ? `?${qs}` : ""}`);
  },
  createSession: (agentId: string) =>
    request<any>("/sessions", { method: "POST", body: JSON.stringify({ agent_id: agentId }) }),

  // Spawn Agent
  spawnAgent: (data: { template: string; project?: string; parent_id?: string }) =>
    request<{ agent_id: string }>("/agents/spawn", { method: "POST", body: JSON.stringify(data) }),

  // Create Prompt
  createPrompt: (data: { project: string; name: string; content: string }) =>
    request<{ ok: boolean }>("/prompts", { method: "POST", body: JSON.stringify(data) }),
  closeSession: (sessionId: string) =>
    request<any>(`/sessions/${sessionId}/close`, { method: "POST" }),

  // Session children (spawned work)
  getSessionChildren: (sessionId: string) =>
    request<any>(`/sessions/${sessionId}/children`),

  // Session messages
  getSessionMessages: (params: { session_id?: string; channel_name?: string; agent_id?: string; limit?: number }) => {
    // Prefer new session-based endpoint when a UUID session_id is available.
    if (params.session_id) {
      const limit = params.limit || 50;
      return request<any>(`/sessions/${params.session_id}/messages?limit=${limit}`);
    }
    // Fallback to deprecated endpoint for backwards compat.
    const query = new URLSearchParams();
    if (params.channel_name) query.set("channel_name", params.channel_name);
    if (params.agent_id) query.set("agent_id", params.agent_id);
    if (params.limit) query.set("limit", String(params.limit));
    const qs = query.toString();
    return request<any>(`/chat/history${qs ? `?${qs}` : ""}`);
  },

  // Context panel (per-channel)
  getNote: (channel: string) => request<any>(`/notes/${encodeURIComponent(channel)}`),
  saveNote: (data: { channel: string; content: string }) =>
    request<any>("/notes", { method: "POST", body: JSON.stringify(data) }),
  deleteNote: (id: string) =>
    request<any>(`/notes/${id}/delete`, { method: "DELETE" }),
  updateDirectiveStatus: (id: string, data: { status: string; task_id?: string }) =>
    request<any>(`/directives/${id}/status`, { method: "POST", body: JSON.stringify(data) }),

  // Triggers
  getTriggers: () => request<any>("/triggers"),
};

export { ApiError };
