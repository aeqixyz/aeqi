import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { api } from "@/lib/api";

export default function QuickActions({
  agentId,
  agentName,
}: {
  agentId: string;
  agentName: string;
}) {
  const navigate = useNavigate();
  const [creating, setCreating] = useState(false);

  const handleNewQuest = () => {
    navigate("/quests");
  };

  const handleNewSession = async () => {
    setCreating(true);
    try {
      const result = await api.createSession(agentId);
      const sessionId = result?.session_id || result?.id;
      if (sessionId) {
        navigate(`/sessions?agent=${encodeURIComponent(agentName)}&session=${sessionId}`);
      } else {
        navigate(`/sessions?agent=${encodeURIComponent(agentName)}`);
      }
    } catch {
      // Navigate to sessions page even on failure
      navigate(`/sessions?agent=${encodeURIComponent(agentName)}`);
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="drawer-actions">
      <button className="drawer-action-btn" onClick={handleNewQuest}>
        New Quest
      </button>
      <button
        className="drawer-action-btn primary"
        onClick={handleNewSession}
        disabled={creating}
      >
        {creating ? "Starting\u2026" : "New Session"}
      </button>
    </div>
  );
}
