import { errMsg, onSyncFinished, syncNow, syncStatus, type LastSync, type SyncStatus } from "./api";
import { app, toast } from "./app.svelte";

/** Shared sync state: the sidebar status line and the App Settings sheet read the same thing. */
export const sync = $state({
  status: null as SyncStatus | null,
  busy: false,
});

export async function refreshSync(): Promise<void> {
  try {
    sync.status = await syncStatus();
  } catch (e) {
    toast(errMsg(e));
  }
}

/** Re-query views only when remote data may have arrived. */
function finished(last: LastSync): void {
  if (sync.status) sync.status = { ...sync.status, last, running: false };
  if (last.report && last.report.pulled > 0) app.rev++;
}

/** Start listening for background sync results. Returns a cleanup function. */
export function startSync(): () => void {
  refreshSync();
  let off: (() => void) | undefined;
  let gone = false;
  onSyncFinished(finished).then((u) => (gone ? u() : (off = u)));
  return () => {
    gone = true;
    off?.();
  };
}

export async function runSync(): Promise<void> {
  sync.busy = true;
  try {
    await syncNow();
  } catch (e) {
    toast(errMsg(e));
  } finally {
    sync.busy = false;
  }
  await refreshSync();
  app.rev++;
}
