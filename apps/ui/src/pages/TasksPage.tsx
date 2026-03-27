import { useEffect, useState } from "react";
import Header from "@/components/Header";
import StatusBadge from "@/components/StatusBadge";
import EmptyState from "@/components/EmptyState";
import { PRIORITY_COLORS } from "@/lib/constants";
import { api } from "@/lib/api";
import { runtimeLabel, summarizeTaskRuntime } from "@/lib/runtime";

export default function TasksPage() {
  const [tasks, setTasks] = useState<any[]>([]);
  const [projects, setProjects] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState("");
  const [projectFilter, setProjectFilter] = useState("");
  const [showForm, setShowForm] = useState(false);
  const [newTask, setNewTask] = useState({ project: "", subject: "", description: "" });
  const [creating, setCreating] = useState(false);

  const fetchTasks = () => {
    setLoading(true);
    const params: any = {};
    if (statusFilter) params.status = statusFilter;
    if (projectFilter) params.project = projectFilter;
    api.getTasks(params).then((data) => {
      setTasks(data.tasks || []);
      setLoading(false);
    }).catch(() => setLoading(false));
  };

  useEffect(() => { fetchTasks(); }, [statusFilter, projectFilter]);

  useEffect(() => {
    api.getProjects().then((data) => setProjects(data.projects || [])).catch(() => {});
  }, []);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newTask.project || !newTask.subject) return;
    setCreating(true);
    try {
      await api.createTask(newTask);
      setNewTask({ project: "", subject: "", description: "" });
      setShowForm(false);
      fetchTasks();
    } catch {
      // ignore
    }
    setCreating(false);
  };

  const handleClose = async (taskId: string) => {
    await api.closeTask(taskId);
    fetchTasks();
  };

  return (
    <>
      <Header
        title="Tasks"
        actions={
          <button className="btn btn-primary" onClick={() => setShowForm(!showForm)}>
            {showForm ? "Cancel" : "+ New Task"}
          </button>
        }
      />

      {showForm && (
        <form className="dash-panel" style={{ marginBottom: "var(--space-6)", padding: "var(--space-5)" }} onSubmit={handleCreate}>
          <div style={{ display: "flex", gap: "var(--space-3)", marginBottom: "var(--space-3)" }}>
            <select
              className="filter-select"
              value={newTask.project}
              onChange={(e) => setNewTask({ ...newTask, project: e.target.value })}
              required
            >
              <option value="">Select project...</option>
              {projects.map((p: any) => (
                <option key={p.name} value={p.name}>{p.name}</option>
              ))}
            </select>
            <input
              className="filter-input"
              style={{ flex: 1 }}
              placeholder="Task subject..."
              value={newTask.subject}
              onChange={(e) => setNewTask({ ...newTask, subject: e.target.value })}
              required
            />
          </div>
          <textarea
            className="filter-input"
            style={{ width: "100%", minHeight: "60px", marginBottom: "var(--space-3)", resize: "vertical" }}
            placeholder="Description (optional)..."
            value={newTask.description}
            onChange={(e) => setNewTask({ ...newTask, description: e.target.value })}
          />
          <button className="btn btn-primary" type="submit" disabled={creating}>
            {creating ? "Creating..." : "Create Task"}
          </button>
        </form>
      )}

      <div className="filters">
        <select
          className="filter-select"
          value={statusFilter}
          onChange={(e) => setStatusFilter(e.target.value)}
        >
          <option value="">All statuses</option>
          <option value="pending">Pending</option>
          <option value="in_progress">In Progress</option>
          <option value="done">Done</option>
          <option value="blocked">Blocked</option>
          <option value="cancelled">Cancelled</option>
        </select>
        <select
          className="filter-select"
          value={projectFilter}
          onChange={(e) => setProjectFilter(e.target.value)}
        >
          <option value="">All projects</option>
          {projects.map((p: any) => (
            <option key={p.name} value={p.name}>{p.name}</option>
          ))}
        </select>
        <span style={{ fontSize: "var(--font-size-xs)", color: "var(--text-muted)", alignSelf: "center" }}>
          {tasks.length} tasks
        </span>
      </div>

      {loading ? (
        <div className="loading">Loading tasks...</div>
      ) : tasks.length === 0 ? (
        <EmptyState title="No tasks" description="No tasks match the current filters." />
      ) : (
        <div className="task-table">
          {tasks.map((task: any) => {
            const label = runtimeLabel(task.runtime);
            const detail = summarizeTaskRuntime(task.runtime, task.closed_reason);

            return (
              <div key={task.id} className="task-row">
                <span
                  className="task-priority-bar"
                  style={{ backgroundColor: PRIORITY_COLORS[task.priority] || "var(--text-primary)" }}
                />
                <code className="task-id">{task.id}</code>
                <div style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: "2px" }}>
                  <span className="task-subject">{task.subject}</span>
                  {(label || detail) && (
                    <span
                      style={{
                        fontSize: "var(--font-size-xs)",
                        color: "var(--text-muted)",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {[label, detail].filter(Boolean).join(" • ")}
                    </span>
                  )}
                </div>
                <div className="task-meta">
                  <StatusBadge status={task.status} size="sm" />
                  <span>{task.assignee || "—"}</span>
                  <span>{task.project}</span>
                  {task.status !== "done" && task.status !== "cancelled" && (
                    <button
                      className="btn"
                      style={{ padding: "1px 8px", fontSize: "var(--font-size-xs)" }}
                      onClick={() => handleClose(task.id)}
                    >
                      close
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}
