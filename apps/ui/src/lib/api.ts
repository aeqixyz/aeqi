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

  // Status
  getStatus: () => request<any>("/status"),

  // Worker events
  getWorkerEvents: (params?: { cursor?: number }) => {
    const query = new URLSearchParams();
    if (params?.cursor != null) query.set("cursor", String(params.cursor));
    const qs = query.toString();
    return request<any>(`/worker/events${qs ? `?${qs}` : ""}`);
  },

  // Projects
  getProjects: () => request<any>("/projects"),

  // Tasks
  getTasks: (params?: { status?: string; project?: string }) => {
    const query = new URLSearchParams();
    if (params?.status) query.set("status", params.status);
    if (params?.project) query.set("project", params.project);
    const qs = query.toString();
    return request<any>(`/tasks${qs ? `?${qs}` : ""}`);
  },

  // Missions
  getMissions: (params?: { project?: string }) => {
    const query = new URLSearchParams();
    if (params?.project) query.set("project", params.project);
    const qs = query.toString();
    return request<any>(`/missions${qs ? `?${qs}` : ""}`);
  },

  // Agents
  getAgents: () => request<any>("/agents"),

  // Audit
  getAudit: (params?: { last?: number; project?: string }) => {
    const query = new URLSearchParams();
    if (params?.last) query.set("last", String(params.last));
    if (params?.project) query.set("project", params.project);
    const qs = query.toString();
    return request<any>(`/audit${qs ? `?${qs}` : ""}`);
  },

  // Blackboard
  getBlackboard: (params?: { project?: string; limit?: number }) => {
    const query = new URLSearchParams();
    if (params?.project) query.set("project", params.project);
    if (params?.limit) query.set("limit", String(params.limit));
    const qs = query.toString();
    return request<any>(`/blackboard${qs ? `?${qs}` : ""}`);
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
  getMemories: (params?: { project?: string; query?: string; limit?: number }) => {
    const q = new URLSearchParams();
    if (params?.project) q.set("project", params.project);
    if (params?.query) q.set("query", params.query);
    if (params?.limit) q.set("limit", String(params.limit));
    const qs = q.toString();
    return request<any>(`/memories${qs ? `?${qs}` : ""}`);
  },

  // Skills
  getSkills: () => request<any>("/skills"),

  // Pipelines
  getPipelines: () => request<any>("/pipelines"),

  // Project Knowledge
  getProjectKnowledge: (name: string) => request<any>(`/projects/${name}/knowledge`),

  // Knowledge CRUD
  storeKnowledge: (data: { project: string; key: string; content: string; category?: string; scope?: string }) =>
    request<any>("/knowledge/store", { method: "POST", body: JSON.stringify(data) }),

  deleteKnowledge: (data: { project: string; id: string }) =>
    request<any>("/knowledge/delete", { method: "POST", body: JSON.stringify(data) }),

  // Channel Knowledge
  getChannelKnowledge: (params: { project: string; query?: string; limit?: number }) => {
    const q = new URLSearchParams();
    q.set("project", params.project);
    if (params.query) q.set("query", params.query);
    if (params.limit) q.set("limit", String(params.limit));
    return request<any>(`/knowledge/channel?${q.toString()}`);
  },

  // Agent Identity
  getAgentIdentity: (name: string) => request<any>(`/agents/${name}/identity`),
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
    project?: string | null;
    department?: string | null;
    channelName?: string | null;
    chatId?: number;
    sender?: string;
  }) =>
    request<any>("/chat/full", {
      method: "POST",
      body: JSON.stringify({
        message: params.message,
        ...(params.project ? { project: params.project } : {}),
        ...(params.department ? { department: params.department } : {}),
        ...(params.channelName ? { channel_name: params.channelName } : {}),
        ...(params.chatId ? { chat_id: params.chatId } : {}),
        ...(params.sender ? { sender: params.sender } : {}),
      }),
    }),

  // Chat — typed thread timeline
  chatTimeline: (params?: {
    chatId?: number;
    project?: string | null;
    department?: string | null;
    channelName?: string | null;
    limit?: number;
  }) => {
    const query = new URLSearchParams();
    if (params?.chatId) query.set("chat_id", String(params.chatId));
    if (params?.project) query.set("project", params.project);
    if (params?.department) query.set("department", params.department);
    if (params?.channelName) query.set("channel_name", params.channelName);
    if (params?.limit) query.set("limit", String(params.limit));
    const qs = query.toString();
    return request<any>(`/chat/timeline${qs ? `?${qs}` : ""}`);
  },

  // Write: Create Task
  createTask: (data: { project: string; subject: string; description?: string }) =>
    request<any>("/tasks", {
      method: "POST",
      body: JSON.stringify(data),
    }),

  // Write: Close Task
  closeTask: (id: string, data?: { reason?: string; project?: string }) =>
    request<any>(`/tasks/${id}/close`, {
      method: "POST",
      body: JSON.stringify(data || {}),
    }),

  // Write: Post to Blackboard
  postBlackboard: (data: { project: string; key: string; content: string; tags?: string[]; durability?: string }) =>
    request<any>("/blackboard", {
      method: "POST",
      body: JSON.stringify(data),
    }),

  // Notes
  getNotes: () => request<any>("/notes"),
  getNote: (channel: string) => request<any>(`/notes/${encodeURIComponent(channel)}`),
  saveNote: (data: { channel: string; content: string }) =>
    request<any>("/notes", { method: "POST", body: JSON.stringify(data) }),
  deleteNote: (id: string) =>
    request<any>(`/notes/${id}/delete`, { method: "DELETE" }),
  updateDirectiveStatus: (id: string, data: { status: string; task_id?: string }) =>
    request<any>(`/directives/${id}/status`, { method: "POST", body: JSON.stringify(data) }),
};

export { ApiError };
