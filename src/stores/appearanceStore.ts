import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";

export type Appearance = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "processhub:appearance";
const DARK_MEDIA_QUERY = "(prefers-color-scheme: dark)";
const BACKGROUND: Record<ResolvedTheme, string> = {
  light: "#f3f5f8",
  dark: "#17181a",
};

function isAppearance(value: string | null): value is Appearance {
  return value === "system" || value === "light" || value === "dark";
}

export function readStoredAppearance(): Appearance {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (isAppearance(stored)) return stored;
  } catch (cause) {
    console.warn("Could not read the appearance preference.", cause);
  }
  return "system";
}

export function systemTheme(): ResolvedTheme {
  return window.matchMedia(DARK_MEDIA_QUERY).matches ? "dark" : "light";
}

function applyResolvedTheme(resolved: ResolvedTheme) {
  const root = document.documentElement;
  root.dataset.theme = resolved;
  root.style.colorScheme = resolved;
  root.style.backgroundColor = BACKGROUND[resolved];
}

async function applyNativeTheme(
  appearance: Appearance,
  resolved: ResolvedTheme,
) {
  if (!isTauri()) return;
  try {
    await getCurrentWindow().setTheme(
      appearance === "system" ? null : resolved,
    );
  } catch (cause) {
    console.warn("Could not update the window appearance.", cause);
  }
}

export function applyAppearance(appearance: Appearance): ResolvedTheme {
  const resolved = appearance === "system" ? systemTheme() : appearance;
  document.documentElement.dataset.appearance = appearance;
  applyResolvedTheme(resolved);
  try {
    window.localStorage.setItem(STORAGE_KEY, appearance);
  } catch (cause) {
    console.warn("Could not save the appearance preference.", cause);
  }
  void applyNativeTheme(appearance, resolved);
  return resolved;
}

interface AppearanceState {
  appearance: Appearance;
  resolved: ResolvedTheme;
  setAppearance: (appearance: Appearance) => void;
  syncSystem: () => void;
}

const initialAppearance = readStoredAppearance();
const initialResolved =
  initialAppearance === "system" ? systemTheme() : initialAppearance;

export const useAppearanceStore = create<AppearanceState>((set, get) => ({
  appearance: initialAppearance,
  resolved: initialResolved,

  setAppearance: (appearance) => {
    set({ appearance, resolved: applyAppearance(appearance) });
  },

  syncSystem: () => {
    if (get().appearance !== "system") return;
    const resolved = systemTheme();
    if (resolved === get().resolved) return;
    applyResolvedTheme(resolved);
    set({ resolved });
  },
}));

function onSystemThemeChange() {
  useAppearanceStore.getState().syncSystem();
}

let mediaQuery: MediaQueryList | null = null;

export function initAppearance() {
  const { appearance } = useAppearanceStore.getState();
  useAppearanceStore.setState({ resolved: applyAppearance(appearance) });

  mediaQuery?.removeEventListener("change", onSystemThemeChange);
  mediaQuery = window.matchMedia(DARK_MEDIA_QUERY);
  mediaQuery.addEventListener("change", onSystemThemeChange);
}
