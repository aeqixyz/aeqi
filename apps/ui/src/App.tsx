import { useEffect } from "react";
import { Routes, Route, Navigate } from "react-router-dom";
import { useAuthStore } from "@/store/auth";
import AppLayout from "@/components/AppLayout";
import LoginPage from "@/pages/LoginPage";
import SignupPage from "@/pages/SignupPage";
import VerifyEmailPage from "@/pages/VerifyEmailPage";
import OnboardingPage from "@/pages/OnboardingPage";
import AuthCallbackPage from "@/pages/AuthCallbackPage";
import WelcomePage from "@/pages/WelcomePage";
import NewWorkspacePage from "@/pages/NewWorkspacePage";
import DashboardHome from "@/components/DashboardHome";
import EventsPage from "@/pages/EventsPage";
import QuestsPage from "@/pages/QuestsPage";
import InsightsPage from "@/pages/InsightsPage";

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const authMode = useAuthStore((s) => s.authMode);
  const token = useAuthStore((s) => s.token);
  const fetchAuthMode = useAuthStore((s) => s.fetchAuthMode);
  const pendingEmail = useAuthStore((s) => s.pendingEmail);

  useEffect(() => {
    fetchAuthMode();
  }, [fetchAuthMode]);

  // Loading mode discovery
  if (!authMode) {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", minHeight: "100vh" }}>
        <div style={{ color: "rgba(0,0,0,0.3)", fontSize: 13 }}>Loading...</div>
      </div>
    );
  }

  if (authMode === "none") return <>{children}</>;
  if (!token) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

export default function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route path="/signup" element={<SignupPage />} />
      <Route path="/verify" element={<VerifyEmailPage />} />
      <Route path="/onboarding" element={<OnboardingPage />} />
      <Route path="/auth/callback" element={<AuthCallbackPage />} />
      <Route
        path="/*"
        element={
          <ProtectedRoute>
            <AppLayout />
          </ProtectedRoute>
        }
      >
        <Route index element={<WelcomePage />} />
        <Route path="new" element={<NewWorkspacePage />} />
        <Route path="agents" element={<DashboardHome />} />
        <Route path="events" element={<EventsPage />} />
        <Route path="quests" element={<QuestsPage />} />
        <Route path="insights" element={<InsightsPage />} />
        <Route path="company" element={<div className="page-content"><h2 style={{color:'var(--text-primary)',margin:'24px'}}>Company</h2><p style={{color:'var(--text-muted)',margin:'0 24px',fontSize:13}}>Projects, teams, and organization settings.</p></div>} />
      </Route>
    </Routes>
  );
}
