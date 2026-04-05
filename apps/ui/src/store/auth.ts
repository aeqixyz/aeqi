import { create } from "zustand";
import { api } from "@/lib/api";

export type AuthMode = "none" | "secret" | "accounts" | null;

interface User {
  id: string;
  email: string;
  name: string;
  avatar_url?: string;
  companies?: string[];
}

interface AuthState {
  token: string | null;
  authMode: AuthMode;
  googleOAuth: boolean;
  user: User | null;
  loading: boolean;
  error: string | null;
  pendingEmail: string | null; // email awaiting verification
  authModeLoaded: boolean;

  fetchAuthMode: () => Promise<void>;
  login: (secret: string) => Promise<boolean>;
  loginWithEmail: (email: string, password: string) => Promise<boolean>;
  signup: (email: string, password: string, name: string) => Promise<"verified" | "pending" | "error">;
  verifyEmail: (email: string, code: string) => Promise<boolean>;
  resendCode: (email: string) => Promise<boolean>;
  handleOAuthCallback: (token: string) => void;
  fetchMe: () => Promise<void>;
  logout: () => void;
  isAuthenticated: () => boolean;
  needsOnboarding: () => boolean;
}

export const useAuthStore = create<AuthState>((set, get) => ({
  token: localStorage.getItem("aeqi_token"),
  authMode: (localStorage.getItem("aeqi_auth_mode") as AuthMode) || null,
  googleOAuth: false,
  user: null,
  loading: false,
  error: null,
  pendingEmail: null,
  authModeLoaded: false,

  fetchAuthMode: async () => {
    if (get().authModeLoaded) return;
    try {
      const resp = await api.getAuthMode();
      const mode = (resp.mode || "secret") as AuthMode;
      localStorage.setItem("aeqi_auth_mode", mode || "secret");
      set({ authMode: mode, googleOAuth: resp.google_oauth, authModeLoaded: true });

      if (mode === "none" && !get().token) {
        try {
          const loginResp = await api.login("");
          if (loginResp.ok && loginResp.token) {
            localStorage.setItem("aeqi_token", loginResp.token);
            set({ token: loginResp.token });
          }
        } catch {
          set({ token: "none" });
        }
      }
    } catch {
      set({ authMode: "secret", authModeLoaded: true });
    }
  },

  login: async (secret: string) => {
    set({ loading: true, error: null });
    try {
      const resp = await api.login(secret);
      if (resp.ok && resp.token) {
        localStorage.setItem("aeqi_token", resp.token);
        set({ token: resp.token, loading: false });
        return true;
      }
      set({ loading: false, error: "Invalid secret" });
      return false;
    } catch {
      set({ loading: false, error: "Login failed" });
      return false;
    }
  },

  loginWithEmail: async (email: string, password: string) => {
    set({ loading: true, error: null });
    try {
      const resp = await api.loginWithEmail(email, password);
      if (resp.ok && resp.token) {
        localStorage.setItem("aeqi_token", resp.token);
        set({ token: resp.token, user: resp.user || null, loading: false });
        return true;
      }
      set({ loading: false, error: "Invalid email or password" });
      return false;
    } catch (e: any) {
      set({ loading: false, error: e?.message || "Login failed" });
      return false;
    }
  },

  signup: async (email: string, password: string, name: string) => {
    set({ loading: true, error: null });
    try {
      const resp = await api.signup(email, password, name);
      if (resp.ok && (resp as any).pending_verification) {
        // Save token so user can onboard while unverified.
        if (resp.token) {
          localStorage.setItem("aeqi_token", resp.token);
          localStorage.setItem("aeqi_pending_email", email);
          set({ token: resp.token, user: resp.user || null, loading: false, pendingEmail: email });
        } else {
          set({ loading: false, pendingEmail: email });
        }
        return "pending";
      }
      if (resp.ok && resp.token) {
        localStorage.setItem("aeqi_token", resp.token);
        set({ token: resp.token, user: resp.user || null, loading: false });
        return "verified";
      }
      set({ loading: false, error: "Signup failed" });
      return "error";
    } catch (e: any) {
      set({ loading: false, error: e?.message || "Signup failed" });
      return "error";
    }
  },

  verifyEmail: async (email: string, code: string) => {
    set({ loading: true, error: null });
    try {
      const resp = await api.verifyEmail(email, code);
      if (resp.ok && resp.token) {
        localStorage.setItem("aeqi_token", resp.token);
        set({ token: resp.token, user: resp.user || null, loading: false, pendingEmail: null });
        return true;
      }
      set({ loading: false, error: "Invalid or expired code" });
      return false;
    } catch (e: any) {
      set({ loading: false, error: e?.message || "Verification failed" });
      return false;
    }
  },

  resendCode: async (email: string) => {
    try {
      await api.resendCode(email);
      return true;
    } catch {
      return false;
    }
  },

  handleOAuthCallback: (token: string) => {
    localStorage.setItem("aeqi_token", token);
    set({ token });
  },

  fetchMe: async () => {
    try {
      const user = await api.getMe();
      set({ user });
    } catch {
      // Not critical.
    }
  },

  logout: () => {
    localStorage.removeItem("aeqi_token");
    localStorage.removeItem("aeqi_auth_mode");
    localStorage.removeItem("aeqi_pending_email");
    localStorage.removeItem("aeqi_company");
    localStorage.removeItem("aeqi_company_tagline");
    localStorage.removeItem("aeqi_company_avatar");
    set({ token: null, user: null, pendingEmail: null, authMode: null, authModeLoaded: false });
  },

  isAuthenticated: () => {
    const { authMode, token } = get();
    if (authMode === "none") return true;
    return !!token;
  },

  needsOnboarding: () => {
    const { user } = get();
    if (!user) return false;
    return !user.companies || user.companies.length === 0;
  },
}));
