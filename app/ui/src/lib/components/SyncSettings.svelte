<script lang="ts">
  import { errMsg, syncConfigSet } from "../api";
  import { toast } from "../app.svelte";
  import { refreshSync, sync } from "../sync.svelte";

  let server = $state(sync.status?.server ?? "");
  let token = $state("");
  let saving = $state(false);

  const configured = $derived(!!sync.status?.configured);

  function when(ms: number): string {
    return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  async function save(clear = false) {
    saving = true;
    try {
      await syncConfigSet(clear ? "" : server, clear ? "" : token);
      token = "";
      if (clear) server = "";
      await refreshSync();
    } catch (e) {
      toast(errMsg(e));
    } finally {
      saving = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      save();
    }
  }
</script>

<section>
  <h3 class="section-label">Sync</h3>
  <div class="group">
    <label class="group-row">
      <span class="grow">Server</span>
      <input
        class="mid"
        bind:value={server}
        placeholder="http://100.x.y.z:7878"
        autocomplete="off"
        spellcheck="false"
        onkeydown={key}
      />
    </label>
    <label class="group-row">
      <span class="grow">Token</span>
      <input
        class="mid"
        type="password"
        bind:value={token}
        placeholder={configured ? "Enter to change" : "Bearer token"}
        autocomplete="off"
        onkeydown={key}
      />
    </label>
    <div class="group-row">
      {#if configured}
        <button type="button" class="btn destructive" disabled={saving} onclick={() => save(true)}>
          Turn off sync
        </button>
      {/if}
      <span class="grow"></span>
      <button type="button" class="btn borderless" disabled={saving} onclick={() => save()}>
        {configured ? "Update" : "Turn on sync"}
      </button>
    </div>
  </div>
  <p class="group-foot">
    {#if sync.status?.last?.error}
      <span class="neg">Failed at {when(sync.status.last.at_ms)}: {sync.status.last.error}</span><br />
    {:else if sync.status?.last?.report}
      {@const r = sync.status.last.report}
      Last synced {when(sync.status.last.at_ms)}: sent {r.pushed}, received {r.pulled}{r.skipped.length
        ? `, skipped ${r.skipped.length}`
        : ""}.<br />
    {/if}
    The token is stored in plain text in your user config folder. Use a Tailscale address and never expose the server
    to the public internet. Save with an empty server to turn sync off.
  </p>
</section>
