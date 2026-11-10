<script lang="ts">
  import { onMount } from "svelte";
  import {
    errMsg,
    onSyncFinished,
    syncConfigSet,
    syncNow,
    syncStatus,
    type LastSync,
    type SyncStatus,
  } from "../api";
  import { app, toast } from "../app.svelte";
  import Drawer from "./Drawer.svelte";

  let status = $state<SyncStatus | null>(null);
  let busy = $state(false);
  let settings = $state(false);
  let server = $state("");
  let token = $state("");
  let saving = $state(false);

  async function refreshStatus() {
    try {
      status = await syncStatus();
    } catch (e) {
      toast(errMsg(e));
    }
  }

  /** Re-query views only when remote data may have arrived. */
  function finished(last: LastSync) {
    if (status) status = { ...status, last, running: false };
    if (last.report && last.report.pulled > 0) app.rev++;
  }

  onMount(() => {
    refreshStatus();
    let off: (() => void) | undefined;
    let gone = false;
    onSyncFinished(finished).then((u) => (gone ? u() : (off = u)));
    return () => {
      gone = true;
      off?.();
    };
  });

  async function sync() {
    busy = true;
    try {
      await syncNow();
    } catch (e) {
      toast(errMsg(e));
    } finally {
      busy = false;
    }
    await refreshStatus();
    app.rev++;
  }

  function openSettings() {
    server = status?.server ?? "";
    token = "";
    settings = true;
  }

  async function save(clear = false) {
    saving = true;
    try {
      await syncConfigSet(clear ? "" : server, clear ? "" : token);
      settings = false;
      token = "";
      await refreshStatus();
    } catch (e) {
      toast(errMsg(e));
    } finally {
      saving = false;
    }
  }

  function when(ms: number): string {
    return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
</script>

<div class="sync" aria-label="Sync">
  <div class="spread">
    <span class="label">Sync</span>
    <button type="button" class="btn ghost sm" onclick={openSettings}>Settings</button>
  </div>
  {#if status && !status.configured}
    <p class="muted small">Not configured.</p>
  {:else if status}
    <p class="muted small host" title={status.server ?? ""}>{status.server}</p>
    {#if status.last?.error}
      <p class="error-text small" role="status">Failed {when(status.last.at_ms)}: {status.last.error}</p>
    {:else if status.last?.report}
      {@const r = status.last.report}
      <p class="muted small" role="status">
        Synced {when(status.last.at_ms)} (sent {r.pushed}, received {r.pulled}{r.skipped.length
          ? `, skipped ${r.skipped.length}`
          : ""})
      </p>
    {/if}
    <button type="button" class="btn sm" disabled={busy || status.running} onclick={sync}>
      {busy || status.running ? "Syncing..." : "Sync now"}
    </button>
  {/if}
</div>

{#if settings}
  <Drawer title="Sync settings" onclose={() => (settings = false)}>
    <form
      class="stack"
      onsubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <label class="field">
        <span>Server URL</span>
        <input bind:value={server} placeholder="http://100.x.y.z:7878" autocomplete="off" spellcheck="false" />
      </label>
      <label class="field">
        <span>Token</span>
        <input
          type="password"
          bind:value={token}
          placeholder={status?.configured ? "Enter token to change settings" : "Bearer token"}
          autocomplete="off"
        />
      </label>
      <p class="muted small">
        The token is stored in plain text in your user config folder. Use a Tailscale address and
        never expose the server to the public internet. Leave the server empty and save to turn
        sync off.
      </p>
      <div class="row">
        <button type="submit" class="btn primary" disabled={saving}>Save</button>
        {#if status?.configured}
          <button type="button" class="btn ghost" disabled={saving} onclick={() => save(true)}>
            Turn off sync
          </button>
        {/if}
      </div>
    </form>
  </Drawer>
{/if}

<style>
  .sync {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 8px 0;
    border-top: 1px solid var(--border);
  }
  .label {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted);
    font-weight: 600;
  }
  .host {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  p {
    margin: 0;
  }
  .row {
    display: flex;
    gap: 8px;
  }
</style>
