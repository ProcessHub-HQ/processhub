import { create } from "zustand";
import { friendlyError } from "../lib/errors";
import { processHub } from "../lib/processhub";
import type {
  Service,
  UpdateInfo,
  UpdatePhase,
  UpdateProgress,
} from "../types";

export type AppView = "workspace" | "monitor";

interface UiState {
  view: AppView;
  settingsOpen: boolean;
  checkingForUpdates: boolean;
  availableUpdate: UpdateInfo | null;
  updateOpen: boolean;
  updatePhase: UpdatePhase;
  updateProgress: UpdateProgress | null;
  updateError: string | null;
  aboutOpen: boolean;
  startupSettingsOpen: boolean;
  quitConfirmationOpen: boolean;
  lifecycleError: string | null;
  notice: string | null;
  searchOpen: boolean;
  searchQuery: string;
  logServiceId: string | null;
  form: { service?: Service } | null;
  pendingDelete: Service | null;
  selectedId: string | null;
  setView: (view: AppView) => void;
  toggleMonitor: () => void;
  setSettingsOpen: (open: boolean) => void;
  toggleSettings: () => void;
  setAboutOpen: (open: boolean) => void;
  setStartupSettingsOpen: (open: boolean) => void;
  setQuitConfirmationOpen: (open: boolean) => void;
  setLifecycleError: (error: string | null) => void;
  setNotice: (notice: string | null) => void;
  setSearchOpen: (open: boolean) => void;
  setSearchQuery: (query: string) => void;
  setLogServiceId: (id: string | null) => void;
  openEditor: (service?: Service) => void;
  closeEditor: () => void;
  setPendingDelete: (service: Service | null) => void;
  setSelectedId: (id: string | null) => void;
  checkForUpdates: () => Promise<void>;
  openUpdateDialog: () => void;
  closeUpdateDialog: () => void;
  setUpdateProgress: (progress: UpdateProgress) => void;
  setUpdatePhase: (phase: UpdatePhase) => void;
  installUpdate: () => Promise<void>;
}

export const useUiStore = create<UiState>((set, get) => ({
  view: "workspace",
  settingsOpen: false,
  checkingForUpdates: false,
  availableUpdate: null,
  updateOpen: false,
  updatePhase: "idle",
  updateProgress: null,
  updateError: null,
  aboutOpen: false,
  startupSettingsOpen: false,
  quitConfirmationOpen: false,
  lifecycleError: null,
  notice: null,
  searchOpen: false,
  searchQuery: "",
  logServiceId: null,
  form: null,
  pendingDelete: null,
  selectedId: null,

  setView: (view) =>
    set({ view, settingsOpen: false, searchOpen: false, searchQuery: "" }),
  toggleMonitor: () =>
    set((current) => ({
      view: current.view === "monitor" ? "workspace" : "monitor",
      settingsOpen: false,
      searchOpen: false,
      searchQuery: "",
    })),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  toggleSettings: () =>
    set((current) => ({ settingsOpen: !current.settingsOpen })),
  setAboutOpen: (aboutOpen) => set({ aboutOpen }),
  setStartupSettingsOpen: (startupSettingsOpen) => set({ startupSettingsOpen }),
  setQuitConfirmationOpen: (quitConfirmationOpen) =>
    set({ quitConfirmationOpen }),
  setLifecycleError: (lifecycleError) => set({ lifecycleError }),
  setNotice: (notice) => set({ notice }),
  setSearchOpen: (searchOpen) =>
    set(searchOpen ? { searchOpen, view: "workspace" } : { searchOpen }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),
  setLogServiceId: (logServiceId) => set({ logServiceId }),
  openEditor: (service) =>
    set({
      form: service ? { service } : {},
      settingsOpen: false,
      logServiceId: null,
    }),
  closeEditor: () => set({ form: null }),
  setPendingDelete: (pendingDelete) => set({ pendingDelete }),
  setSelectedId: (selectedId) => set({ selectedId }),

  checkForUpdates: async () => {
    if (get().checkingForUpdates) return;
    set({
      checkingForUpdates: true,
      settingsOpen: false,
      updateOpen: true,
      updatePhase: "checking",
      availableUpdate: null,
      updateProgress: null,
      updateError: null,
      lifecycleError: null,
      notice: null,
    });
    try {
      const update = await processHub.checkForUpdate();
      if (update) {
        set({
          availableUpdate: update,
          updatePhase: "available",
        });
      } else {
        set({
          availableUpdate: null,
          updatePhase: "current",
        });
      }
    } catch (cause) {
      set({
        updatePhase: "error",
        updateError: friendlyError(
          cause,
          "Couldn't check for updates. Try again.",
        ),
      });
    } finally {
      set({ checkingForUpdates: false });
    }
  },

  openUpdateDialog: () => {
    if (get().availableUpdate) {
      set({
        updateOpen: true,
        updatePhase: "available",
        updateProgress: null,
        settingsOpen: false,
      });
      return;
    }
    void get().checkForUpdates();
  },

  closeUpdateDialog: () => {
    const phase = get().updatePhase;
    if (
      phase === "checking" ||
      phase === "downloading" ||
      phase === "installing" ||
      phase === "restarting"
    ) {
      return;
    }
    set({ updateOpen: false });
  },

  setUpdateProgress: (updateProgress) => set({ updateProgress }),

  setUpdatePhase: (updatePhase) => set({ updatePhase }),

  installUpdate: async () => {
    const { availableUpdate, updatePhase } = get();
    if (
      !availableUpdate ||
      (updatePhase !== "available" && updatePhase !== "error")
    ) {
      return;
    }
    set({
      updatePhase: "downloading",
      updateProgress: { downloaded: 0, total: null },
      updateError: null,
    });
    try {
      await processHub.installUpdate();
      set({ updatePhase: "restarting" });
    } catch (cause) {
      set({
        updatePhase: "error",
        updateError: friendlyError(
          cause,
          "The update couldn't be installed. Try again or download it from GitHub.",
        ),
        updateProgress: null,
      });
    }
  },
}));
