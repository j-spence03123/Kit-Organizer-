// Native dialogs, isolated so tests can mock them.
import { ask, open, save } from "@tauri-apps/plugin-dialog";

export async function pickFolders(multiple: boolean, title: string): Promise<string[]> {
  const res = await open({ directory: true, multiple, title });
  if (res == null) return [];
  return Array.isArray(res) ? res : [res];
}

export async function pickJsonFile(): Promise<string | null> {
  const res = await open({ multiple: false, directory: false, filters: [{ name: "Kit Organizer configuration", extensions: ["json"] }] });
  return typeof res === "string" ? res : null;
}

export async function pickSavePath(defaultPath: string): Promise<string | null> {
  return save({ defaultPath, filters: [{ name: "JSON", extensions: ["json"] }] });
}

export async function confirmDialog(message: string, title: string, okLabel = "Continue"): Promise<boolean> {
  return ask(message, { title, kind: "warning", okLabel, cancelLabel: "Cancel" });
}
