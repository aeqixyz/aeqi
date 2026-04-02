import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAuthStore } from "@/store/auth";

export default function LoginPage() {
  const [secret, setSecret] = useState("");
  const { login, loading, error } = useAuthStore();
  const navigate = useNavigate();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const ok = await login(secret);
    if (ok) navigate("/");
  };

  return (
    <div className="login-page">
      <div className="login-card">
        <h1 className="login-title">AEQI</h1>
        <p className="login-subtitle">Agent Orchestration</p>
        <form className="login-form" onSubmit={handleSubmit}>
          <input
            className="login-input"
            type="password"
            placeholder="Access key"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            autoFocus
          />
          {error && <div className="login-error">{error}</div>}
          <button className="btn btn-primary" type="submit" disabled={loading}>
            {loading ? "Connecting..." : "Enter"}
          </button>
        </form>
        <p className="login-footer">
          Self-hosted instance. Enter your configured access key.
        </p>
      </div>
    </div>
  );
}
