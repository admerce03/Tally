import { execute, errMsg, type Command, type CommandOutput, type OutputOf } from "./api";

export type Route =
  | { name: "home" }
  | { name: "course"; id: string }
  | { name: "gpa" }
  | { name: "scenarios" };

export type Toast = { id: number; text: string; kind: "error" | "info" };

function parseHash(): Route {
  const parts = location.hash.replace(/^#\/?/, "").split("/");
  if (parts[0] === "course" && parts[1]) return { name: "course", id: parts[1] };
  if (parts[0] === "gpa") return { name: "gpa" };
  if (parts[0] === "scenarios") return { name: "scenarios" };
  return { name: "home" };
}

function toHash(r: Route): string {
  switch (r.name) {
    case "course":
      return `#/course/${r.id}`;
    case "gpa":
      return "#/gpa";
    case "scenarios":
      return "#/scenarios";
    default:
      return "#/";
  }
}

export const app = $state({
  route: parseHash() as Route,
  /** Scenario every query runs through, or null for real data. */
  scenarioId: null as string | null,
  /** Whether displayed grades apply curves (assumptions) or ignore them. */
  curveMode: "apply" as "apply" | "ignore",
  /** Bumped after every successful mutation; queries re-run when it changes. */
  rev: 0,
  toasts: [] as Toast[],
});

window.addEventListener("hashchange", () => {
  app.route = parseHash();
});

export function navigate(r: Route): void {
  location.hash = toHash(r);
}

let toastSeq = 0;
export function toast(text: string, kind: Toast["kind"] = "error"): void {
  const id = ++toastSeq;
  app.toasts.push({ id, text, kind });
  setTimeout(() => dismissToast(id), 6000);
}

export function dismissToast(id: number): void {
  app.toasts = app.toasts.filter((t) => t.id !== id);
}

/** Execute and assert the output variant. Throws on error or an unexpected shape. */
export async function ask<R extends CommandOutput["result"]>(
  command: Command,
  result: R,
): Promise<OutputOf<R>> {
  const out = await execute(command);
  if (out.result !== result) throw new Error(`Unexpected output "${out.result}" (wanted "${result}")`);
  return out as OutputOf<R>;
}

/** Run a mutation. Toasts and returns null on error; bumps `rev` so every view re-queries. */
export async function mutate(command: Command): Promise<CommandOutput | null> {
  try {
    const out = await execute(command);
    app.rev++;
    return out;
  } catch (e) {
    toast(errMsg(e));
    return null;
  }
}

/**
 * Reactive query. `source` runs inside an effect (so reactive reads are tracked) and returns a
 * loader thunk, or null to skip. Re-runs when tracked state or `app.rev` changes. Stale
 * responses are dropped. The previous value stays visible while reloading.
 */
export function query<T>(source: () => (() => Promise<T>) | null) {
  let value = $state<T | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let token = 0;
  $effect(() => {
    void app.rev;
    const thunk = source();
    const mine = ++token;
    if (!thunk) {
      value = null;
      error = null;
      loading = false;
      return;
    }
    loading = true;
    thunk()
      .then((v) => {
        if (mine === token) {
          value = v;
          error = null;
        }
      })
      .catch((e) => {
        if (mine === token) error = errMsg(e);
      })
      .finally(() => {
        if (mine === token) loading = false;
      });
  });
  return {
    get value() {
      return value;
    },
    get error() {
      return error;
    },
    get loading() {
      return loading;
    },
  };
}
