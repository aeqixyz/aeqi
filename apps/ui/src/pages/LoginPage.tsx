import { useState, useEffect } from "react";
import { useNavigate, Link } from "react-router-dom";
import { useAuthStore } from "@/store/auth";

export default function LoginPage() {
  const navigate = useNavigate();
  const {
    authMode,
    googleOAuth,
    loading,
    error,
    fetchAuthMode,
    login,
    loginWithEmail,
    isAuthenticated,
  } = useAuthStore();

  const [secret, setSecret] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");

  useEffect(() => {
    fetchAuthMode();
  }, [fetchAuthMode]);

  useEffect(() => {
    if (isAuthenticated()) navigate("/", { replace: true });
  }, [isAuthenticated, navigate]);

  const handleSecretSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const ok = await login(secret);
    if (ok) navigate("/");
  };

  const handleEmailSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const ok = await loginWithEmail(email, password);
    if (ok) navigate("/");
  };

  const handleGoogle = () => {
    window.location.href = "/api/auth/google";
  };

  if (authMode === "secret") {
    return (
      <div className="auth-page">
        <div className="auth-container">
          <div className="auth-logo">æ</div>
          <h1 className="auth-heading">Welcome back</h1>
          <p className="auth-subheading">Enter your access key to continue</p>
          <form className="auth-form" onSubmit={handleSecretSubmit}>
            <input
              className="auth-input"
              type="password"
              placeholder="Access key"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              autoFocus
            />
            {error && <div className="auth-error">{error}</div>}
            <button className="auth-btn-primary" type="submit" disabled={loading}>
              {loading ? "Connecting..." : "Continue"}
            </button>
          </form>
        </div>
        <div className="auth-footer">
          <p>
            By continuing, you agree to the{" "}
            <a href="/terms">Terms of Service</a>
            {" "}and{" "}
            <a href="/privacy">Privacy Policy</a>.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="auth-page">
      <div className="auth-container">
        <div className="auth-logo">æ</div>
        <h1 className="auth-heading">Sign in to aeqi</h1>
        <p className="auth-subheading">Run companies with autonomous agents</p>

        {googleOAuth && (
          <button className="auth-btn-google" onClick={handleGoogle} type="button">
            <svg width="18" height="18" viewBox="0 0 24 24">
              <path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 01-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z" fill="#4285F4" />
              <path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" fill="#34A853" />
              <path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" fill="#FBBC05" />
              <path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" fill="#EA4335" />
            </svg>
            Continue with Google
          </button>
        )}

        {googleOAuth && (
          <div className="auth-divider"><span>or</span></div>
        )}

        <form className="auth-form" onSubmit={handleEmailSubmit}>
          <input
            className="auth-input"
            type="email"
            placeholder="Email address"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            autoFocus={!googleOAuth}
          />
          <input
            className="auth-input"
            type="password"
            placeholder="Password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          {error && <div className="auth-error">{error}</div>}
          <button className="auth-btn-primary" type="submit" disabled={loading}>
            {loading ? "Signing in..." : "Continue"}
          </button>
        </form>

        <p className="auth-switch">
          Don't have an account?{" "}
          <Link to="/signup">Sign up</Link>
        </p>
      </div>
      <div className="auth-footer">
        <p>
          By continuing, you agree to the{" "}
          <a href="/terms">Terms of Service</a>
          {" "}and{" "}
          <a href="/privacy">Privacy Policy</a>.
        </p>
      </div>
    </div>
  );
}
