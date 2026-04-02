import { Routes, Route, Navigate } from "react-router-dom";
import { useAuthStore } from "@/store/auth";
import AppLayout from "@/components/AppLayout";
import LoginPage from "@/pages/LoginPage";
import ChatPage from "@/pages/ChatPage";
import InboxPage from "@/pages/InboxPage";
import AgentsPage from "@/pages/AgentsPage";
import AgentDetailPage from "@/pages/AgentDetailPage";
import DepartmentsPage from "@/pages/DepartmentsPage";
import TasksPage from "@/pages/TasksPage";
import TriggersPage from "@/pages/TriggersPage";
import MemoryPage from "@/pages/MemoryPage";
import BlackboardPage from "@/pages/BlackboardPage";
import SkillsPage from "@/pages/SkillsPage";
import CostPage from "@/pages/CostPage";
import AuditPage from "@/pages/AuditPage";
import SettingsPage from "@/pages/SettingsPage";

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const token = useAuthStore((s) => s.token);
  if (!token) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

export default function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route
        path="/"
        element={
          <ProtectedRoute>
            <AppLayout />
          </ProtectedRoute>
        }
      >
        <Route index element={<ChatPage />} />
        <Route path="inbox" element={<InboxPage />} />
        <Route path="agents" element={<AgentsPage />} />
        <Route path="agents/:name" element={<AgentDetailPage />} />
        <Route path="departments" element={<DepartmentsPage />} />
        <Route path="tasks" element={<TasksPage />} />
        <Route path="triggers" element={<TriggersPage />} />
        <Route path="memory" element={<MemoryPage />} />
        <Route path="blackboard" element={<BlackboardPage />} />
        <Route path="skills" element={<SkillsPage />} />
        <Route path="cost" element={<CostPage />} />
        <Route path="audit" element={<AuditPage />} />
        <Route path="settings" element={<SettingsPage />} />
        {/* Redirects for removed routes */}
        <Route path="dashboard" element={<Navigate to="/" replace />} />
        <Route path="projects" element={<Navigate to="/" replace />} />
        <Route path="projects/:name" element={<Navigate to="/" replace />} />
        <Route path="missions" element={<Navigate to="/" replace />} />
        <Route path="operations" element={<Navigate to="/" replace />} />
        <Route path="knowledge" element={<Navigate to="/memory" replace />} />
      </Route>
    </Routes>
  );
}
