import { create } from "zustand";

export type LayoutMode = "focus" | "split" | "stack";

interface UIState {
  sidebarCollapsed: boolean;
  layout: LayoutMode;
  layoutPickerOpen: boolean;
  splitRatio: number;
  drawerOpen: boolean;
  drawerMode: "context" | "activity";
  toggleSidebar: () => void;
  setLayout: (mode: LayoutMode) => void;
  setSplitRatio: (ratio: number) => void;
  toggleLayoutPicker: () => void;
  closeLayoutPicker: () => void;
  toggleDrawer: () => void;
  setDrawerMode: (mode: "context" | "activity") => void;
}

export const useUIStore = create<UIState>((set) => ({
  sidebarCollapsed: localStorage.getItem("aeqi_sidebar_collapsed") === "true",
  layout: (localStorage.getItem("aeqi_layout") as LayoutMode) || "split",
  layoutPickerOpen: false,
  splitRatio: parseFloat(localStorage.getItem("aeqi_split_ratio") || "0.65"),
  toggleSidebar: () =>
    set((state) => {
      const next = !state.sidebarCollapsed;
      localStorage.setItem("aeqi_sidebar_collapsed", String(next));
      return { sidebarCollapsed: next };
    }),
  setLayout: (mode) => {
    localStorage.setItem("aeqi_layout", mode);
    set({ layout: mode, layoutPickerOpen: false });
  },
  setSplitRatio: (ratio) => {
    const clamped = Math.max(0.3, Math.min(0.85, ratio));
    localStorage.setItem("aeqi_split_ratio", String(clamped));
    set({ splitRatio: clamped });
  },
  toggleLayoutPicker: () => set((s) => ({ layoutPickerOpen: !s.layoutPickerOpen })),
  closeLayoutPicker: () => set({ layoutPickerOpen: false }),
  drawerOpen: localStorage.getItem("aeqi_drawer_open") !== "false",
  drawerMode: "context",
  toggleDrawer: () =>
    set((s) => {
      const next = !s.drawerOpen;
      localStorage.setItem("aeqi_drawer_open", String(next));
      return { drawerOpen: next };
    }),
  setDrawerMode: (mode) => set({ drawerMode: mode }),
}));
