import { invoke } from "@tauri-apps/api/core";
import type {
  FieldChoice,
  SetupResult,
  Snapshot,
} from "./types";

export interface PartialConfig {
  base_url: string;
  user: string;
}

export async function isConfigured(): Promise<boolean> {
  return invoke<boolean>("is_configured");
}

export async function getConfig(): Promise<PartialConfig | null> {
  return invoke<PartialConfig | null>("get_config");
}

export async function getPoints(): Promise<Snapshot | null> {
  return invoke<Snapshot | null>("get_points");
}

export async function refreshNow(): Promise<Snapshot> {
  return invoke<Snapshot>("refresh_now");
}

export async function saveCredentials(args: {
  url: string;
  user: string;
  token: string;
  fieldId?: string;
}): Promise<SetupResult> {
  return invoke<SetupResult>("save_credentials", {
    url: args.url,
    user: args.user,
    token: args.token,
    fieldId: args.fieldId ?? null,
  });
}

export async function listPointCandidates(): Promise<FieldChoice[]> {
  return invoke<FieldChoice[]>("list_point_candidates");
}

export async function clearCredentials(): Promise<void> {
  await invoke("clear_credentials");
}

export async function getIdleSeconds(): Promise<number> {
  return invoke<number>("get_idle_seconds");
}

export async function getMode(): Promise<"days90" | "monthly"> {
  return invoke<"days90" | "monthly">("get_mode");
}

export async function setMode(mode: "days90" | "monthly"): Promise<void> {
  await invoke("set_mode", { mode });
}

export async function getProjectKey(): Promise<string | null> {
  return invoke<string | null>("get_project_key");
}

export async function setProjectKey(projectKey: string | null): Promise<void> {
  await invoke("set_project_key", { projectKey });
}

export async function quitApp(): Promise<void> {
  await invoke("quit_app");
}

export async function sendToBack(): Promise<void> {
  await invoke("send_to_back");
}

export async function sendToFront(): Promise<void> {
  await invoke("send_to_front");
}
