import { create } from "zustand";
import { processHub } from "../lib/processhub";
import type { Service } from "../types";

interface UiState {
  settingsOpen: boolean;
  checkingForUpdates: boolean;
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
}

export const useUiStore = create<UiState>((set, get) => ({
  settingsOpen: false,
  checkingForUpdates: false,
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

  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  toggleSettings: () =>
    set((current) => ({ settingsOpen: !current.settingsOpen })),
  setAboutOpen: (aboutOpen) => set({ aboutOpen }),
  setStartupSettingsOpen: (startupSettingsOpen) => set({ startupSettingsOpen }),
  setQuitConfirmationOpen: (quitConfirmationOpen) =>
    set({ quitConfirmationOpen }),
  setLifecycleError: (lifecycleError) => set({ lifecycleError }),
  setNotice: (notice) => set({ notice }),
  setSearchOpen: (searchOpen) => set({ searchOpen }),
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
      lifecycleError: null,
      notice: null,
    });
    try {
      const update = await processHub.checkForUpdate();
      set({
        notice: update
          ? `ProcessHub ${update.version} is available.`
          : "ProcessHub is up to date.",
      });
    } catch (cause) {
      set({ lifecycleError: String(cause) });
    } finally {
      set({ checkingForUpdates: false, settingsOpen: false });
    }
  },
}));
