<script lang="ts">
  import { app, ask, mutate, navigate, query, toast } from "../app.svelte";
  import type { Scenario } from "../generated/output";
  import { chrome } from "../platform";
  import { discardWhatIf, finishWhatIf, isDraft, startWhatIf, switchWhatIf } from "../whatif";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import WindowControls from "./WindowControls.svelte";

  // The top strip of the content area: it is the title bar (drag to move, double-click to maximize)
  // and holds the what-if control. On Windows it also draws the window buttons.
  let { scenarios, active }: { scenarios: Scenario[]; active: Scenario | null } = $props();

  const terms = query(() => () => ask({ command: "list_terms" }, "terms").then((o) => o.terms));
  const courseName = $derived.by(() => {
    const r = app.route;
    if (r.name !== "course") return null;
    for (const t of terms.value ?? []) for (const c of t.courses) if (c.id === r.id) return c.name;
    return null;
  });

  let menuOpen = $state(false);
  let renaming = $state(false);
  let name = $state("");
  let confirming = $state(false);

  // Same shape as the What-if page: new (for this course), the saved ones, then actions.
  const items = $derived.by((): MenuItem[] => {
    const out: MenuItem[] = [];
    if (courseName) out.push({ label: `New what-if for ${courseName}`, onselect: () => startWhatIf(courseName) });
    scenarios.forEach((s, i) =>
      out.push({
        label: s.name,
        separator: i === 0 && out.length > 0,
        // A check column only while one is open, marking it.
        checked: active ? s.id === active.id : undefined,
        onselect: () => switchWhatIf(s.id),
      }),
    );
    if (active)
      out.push(
        { label: "Rename…", separator: true, onselect: startRename },
        { label: "Discard…", destructive: true, onselect: discard },
      );
    out.push({ label: "All what-ifs", separator: out.length > 0 && !active, onselect: () => navigate({ name: "scenarios" }) });
    return out;
  });

  function startRename() {
    if (!active) return;
    name = active.name;
    renaming = true;
  }

  async function commitRename() {
    if (!renaming || !active) return;
    renaming = false;
    const n = name.trim();
    if (!n) return toast("Name is required");
    if (n !== active.name) await mutate({ command: "rename_scenario", id: active.id, name: n });
  }

  function discard() {
    if (!active) return;
    if (isDraft(active.id) || (active.overrides ?? []).length === 0) discardWhatIf();
    else confirming = true;
  }

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<header class="toolbar" class:mac={chrome === "mac"} data-tauri-drag-region>
  <div class="grow" data-tauri-drag-region></div>

  {#if renaming && active}
    <input
      use:focusSelect
      class="rename"
      bind:value={name}
      aria-label="What-if name"
      spellcheck="false"
      onblur={commitRename}
      onkeydown={(e) => {
        if (e.key === "Enter") commitRename();
        if (e.key === "Escape") renaming = false;
      }}
    />
  {:else}
    <span class="anchor">
      <button
        type="button"
        class="btn whatif"
        class:on={!!active}
        aria-haspopup="menu"
        title={active ? "What-if: hypothetical scores, real grades unchanged" : "Try hypothetical scores"}
        onclick={() => (menuOpen = !menuOpen)}
      >
        <span class="ellipsis">{active ? active.name : "What if"}</span>
        <span class="chev"><Icon name="chevron" size={12} /></span>
      </button>
      {#if menuOpen}
        <Menu end {items} onclose={() => (menuOpen = false)} />
      {/if}
    </span>
  {/if}
  {#if active}
    <button type="button" class="btn prominent" onclick={finishWhatIf}>Done</button>
  {/if}

  {#if chrome === "windows"}
    <WindowControls />
  {/if}
</header>

{#if confirming && active}
  {@const s = active}
  <Alert
    title="Discard {s.name}?"
    message="Its hypothetical scores are deleted. Your real grades stay as they are."
    confirmLabel="Discard"
    onconfirm={() => {
      confirming = false;
      discardWhatIf();
    }}
    oncancel={() => (confirming = false)}
  />
{/if}

<style>
  .toolbar {
    flex: none;
    height: var(--bar-h);
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding-left: var(--gutter);
    padding-right: var(--s3);
    user-select: none;
  }
  /* Window buttons sit flush with the edge. */
  .toolbar:has(:global(.controls)) {
    padding-right: 0;
  }
  .whatif {
    max-width: 260px;
    min-height: 28px;
  }
  .whatif.on {
    background: var(--accent-tint);
    color: var(--accent);
    font-weight: 600;
  }
  .chev {
    display: inline-flex;
    transform: rotate(90deg);
    opacity: 0.7;
  }
  .rename {
    width: 220px;
  }
</style>
