import { Routes, Route, Navigate } from "react-router-dom";
import { useAuthStore } from "@/store/auth";
import AppLayout from "@/components/AppLayout";
import LoginPage from "@/pages/LoginPage";
import WelcomePage from "@/pages/WelcomePage";
import DashboardHome from "@/components/DashboardHome";
import EventsPage from "@/pages/EventsPage";
import QuestsPage from "@/pages/QuestsPage";
import InsightsPage from "@/pages/InsightsPage";

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
        path="/*"
        element={
          <ProtectedRoute>
            <AppLayout />
          </ProtectedRoute>
        }
      >
        <Route index element={<WelcomePage />} />
        <Route path="agents" element={<DashboardHome />} />
        <Route path="events" element={<EventsPage />} />
        <Route path="quests" element={<QuestsPage />} />
        <Route path="insights" element={<InsightsPage />} />
        <Route path="company" element={<div className="page-content"><h2 style={{color:'var(--text-primary)',margin:'24px'}}>Company</h2><p style={{color:'var(--text-muted)',margin:'0 24px',fontSize:13}}>Projects, teams, and organization settings.</p></div>} />
      </Route>
    </Routes>
  );
}
