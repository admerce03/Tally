import { invoke } from "@tauri-apps/api/core";
import type { Command } from "./generated/command";
import type { CommandOutput } from "./generated/output";

export type { Command, CommandOutput };

/** Narrow a CommandOutput to one `result` variant. */
export type OutputOf<R extends CommandOutput["result"]> = Extract<CommandOutput, { result: R }>;

/**
 * Run one core Command through the Rust backend (same path as MCP). Rejects with the error
 * message string from core/store. The UI never computes grades: it renders what comes back.
 */
export async function execute(command: Command): Promise<CommandOutput> {
  return (await invoke("execute", { command })) as CommandOutput;
}

/** A fresh UUID v4 for add/create commands (core never generates ids). */
export function newId(): string {
  return crypto.randomUUID();
}
