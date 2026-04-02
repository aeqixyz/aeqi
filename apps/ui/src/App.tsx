import { Routes, Route, Navigate } from "react-router-dom";
import { useAuthStore } from "@/store/auth";
import AppLayout from "@/components/AppLayout";
import LoginPage from "@/pages/LoginPage";
import DashboardPage from "@/pages/DashboardPage";
import InboxPage from "@/pages/InboxPage";
import TasksPage from "@/pages/TasksPage";
import DepartmentsPage from "@/pages/DepartmentsPage";
import MemoryPage from "@/pages/MemoryPage";
import TriggersPage from "@/pages/TriggersPage";
import SkillsPage from "@/pages/SkillsPage";
import FinancePage from "@/pages/FinancePage";

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
        {/* 5 main views */}
        <Route index element={<DashboardPage />} />
        <Route path="inbox" element={<InboxPage />} />
        <Route path="issues" element={<TasksPage />} />
        <Route path="automations" element={<TriggersPage />} />
        <Route path="knowledge" element={<MemoryPage />} />
        <Route path="finance" element={<FinancePage />} />

        {/* Contextual routes */}
        <Route path="departments/:id" element={<DepartmentsPage />} />

        {/* Redirects */}
        <Route path="tasks" element={<Navigate to="/issues" replace />} />
        <Route path="triggers" element={<Navigate to="/automations" replace />} />
        <Route path="memory" element={<Navigate to="/knowledge" replace />} />
        <Route path="skills" element={<Navigate to="/knowledge" replace />} />
        <Route path="blackboard" element={<Navigate to="/knowledge" replace />} />
        <Route path="cost" element={<Navigate to="/" replace />} />
        <Route path="audit" element={<Navigate to="/" replace />} />
        <Route path="dashboard" element={<Navigate to="/" replace />} />
        <Route path="agents" element={<Navigate to="/" replace />} />
        <Route path="agents/:name" element={<Navigate to="/" replace />} />
        <Route path="settings" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
