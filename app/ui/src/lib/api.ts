import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Command } from "./generated/command";
import type { CommandOutput } from "./generated/output";

export type { Command, CommandOutput };

/** Narrow a CommandOutput to one `result` variant. */
export type OutputOf<R extends CommandOutput["result"]> = Extract<CommandOutput, { result: R }>;

const HTTP_BASE: string = import.meta.env.VITE_TALLY_API ?? "http://127.0.0.1:7878";

/** Human-readable message from whatever a transport rejected with. */
export function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "error" in e) return String((e as { error: unknown }).error);
  return String(e);
}

async function executeHttp(command: Command): Promise<CommandOutput> {
  let res: Response;
  try {
    res = await fetch(`${HTTP_BASE}/api/execute`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(command),
    });
  } catch {
    throw new Error(`Cannot reach the Tally server at ${HTTP_BASE}`);
  }
  let body: unknown;
  try {
    body = await res.json();
  } catch {
    throw new Error(`Server returned an invalid response (HTTP ${res.status})`);
  }
  if (body && typeof body === "object" && "error" in body) {
    throw new Error(String((body as { error: unknown }).error));
  }
  if (!res.ok) throw new Error(`Server error (HTTP ${res.status})`);
  return body as CommandOutput;
}

/**
 * Run one core Command: through the Rust backend when inside Tauri (same path as MCP), otherwise
 * over HTTP. Rejects with an Error carrying the core/store message. The UI never computes
 * grades: it renders what comes back.
 */
export async function execute(command: Command): Promise<CommandOutput> {
  if (isTauri()) {
    try {
      return (await invoke("execute", { command })) as CommandOutput;
    } catch (e) {
      throw new Error(errMsg(e));
    }
  }
  return executeHttp(command);
}

/** A fresh UUID v4 for add/create commands (core never generates ids). */
export function newId(): string {
  return crypto.randomUUID();
}

/** Outcome of a sync attempt: a report, or an error message. */
export type SyncReport = {
  pushed: number;
  pulled: number;
  skipped: [string, string][];
  cursor: number;
};
export type LastSync = { at_ms: number; report: SyncReport | null; error: string | null };
export type SyncStatus = {
  configured: boolean;
  server: string | null;
  running: boolean;
  last: LastSync | null;
};

/** Sync is a desktop-app feature (the Rust backend talks to the server). */
export const syncAvailable: boolean = isTauri();

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return (await invoke(cmd, args)) as T;
  } catch (e) {
    throw new Error(errMsg(e));
  }
}

export const syncStatus = () => call<SyncStatus>("sync_status");
export const syncNow = () => call<SyncReport>("sync_now");
export const syncConfigSet = (server: string, token: string) =>
  call<void>("sync_config_set", { server, token });

/** Subscribe to background sync results. Returns an unsubscribe function. */
export async function onSyncFinished(handler: (last: LastSync) => void): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<LastSync>("sync-finished", (e) => handler(e.payload));
}
