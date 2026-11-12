// What-if mode: a scenario the whole app is viewed through while you type hypothetical scores in place.
import { errMsg, newId } from "./api";
import { app, ask, mutate, toast } from "./app.svelte";

/** Scenarios started here this session. Done drops them while still empty; Discard needs no confirmation. */
const drafts = new Set<string>();

export function isDraft(id: string): boolean {
  return drafts.has(id);
}

async function scenarios() {
  return (await ask({ command: "list_scenarios" }, "scenarios")).scenarios;
}

/** Start a new what-if named after `base` ("ENGR 335 what-if", "… 2" when taken) and view through it. */
export async function startWhatIf(base: string): Promise<void> {
  await finishWhatIf();
  try {
    const taken = new Set((await scenarios()).map((s) => s.name.toLowerCase()));
    let name = `${base} what-if`;
    for (let i = 2; taken.has(name.toLowerCase()); i++) name = `${base} what-if ${i}`;
    const id = newId();
    if (await mutate({ command: "create_scenario", id, name })) {
      drafts.add(id);
      app.scenarioId = id;
    }
  } catch (e) {
    toast(errMsg(e));
  }
}

/** Leave what-if mode, keeping the scenario unless it is a draft with no changes. */
export async function finishWhatIf(): Promise<void> {
  const id = app.scenarioId;
  if (!id) return;
  app.scenarioId = null;
  if (!drafts.has(id)) return;
  drafts.delete(id);
  try {
    const s = (await scenarios()).find((x) => x.id === id);
    if (s && (s.overrides ?? []).length === 0) await mutate({ command: "delete_scenario", id });
  } catch (e) {
    toast(errMsg(e));
  }
}

/** Delete the scenario being viewed and leave what-if mode. */
export async function discardWhatIf(): Promise<void> {
  const id = app.scenarioId;
  if (!id) return;
  app.scenarioId = null;
  drafts.delete(id);
  await mutate({ command: "delete_scenario", id });
}

/** View through another saved what-if (an empty draft being left is dropped). */
export async function switchWhatIf(id: string): Promise<void> {
  if (app.scenarioId === id) return;
  await finishWhatIf();
  app.scenarioId = id;
}
