import { invoke } from "@tauri-apps/api/core";
import { configureCommandOptions } from "./blocks";
interface Settings {
  app?: { name?: string; language?: string };
  appearance?: {
    theme?: "light" | "dark";
    primary_color?: string;
    background_color?: string;
    surface_color?: string;
  };
  preferences?: { show_all_command_options?: boolean };
}
export async function applySettings(): Promise<void> {
  const settings = await invoke<Settings>("get_settings");
  const brand = document.querySelector(".brand strong");
  if (brand) brand.textContent = settings.app?.name?.trim() || "SOL Knowledge";
  const a = settings.appearance ?? {};
  const root = document.documentElement;
  const light = a.theme === "light";
  root.dataset.theme = light ? "light" : "dark";
  root.style.colorScheme = light ? "light" : "dark";
  const color = (name: string, v: string | undefined, fallback: string) =>
    root.style.setProperty(name, v && /^#[0-9a-f]{6}$/i.test(v) ? v : fallback);
  color("--yellow", a.primary_color, "#facc15");
  color("--bg", a.background_color, light ? "#f5f5f2" : "#101010");
  color("--surface", a.surface_color, light ? "#fffef8" : "#202020");
  color("--ink", undefined, light ? "#202020" : "#f5f5f2");
  color("--muted", undefined, light ? "#595951" : "#a2a29b");
  configureCommandOptions(
    settings.preferences?.show_all_command_options === true,
  );
}
