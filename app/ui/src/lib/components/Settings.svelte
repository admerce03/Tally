<script lang="ts">
  import { app, ask, query } from "../app.svelte";
  import AppSettings from "./AppSettings.svelte";
  import CourseSettings from "./CourseSettings.svelte";
  import Sheet from "./Sheet.svelte";

  // The one Settings sheet. On a course page it opens on that course; "All courses" holds the rest.
  const courseId = $derived(app.route.name === "course" ? app.route.id : null);
  const course = query(() => {
    if (!courseId) return null;
    const cmd = { command: "show_course" as const, course_id: courseId, scenario_id: app.scenarioId };
    return () => ask(cmd, "course").then((o) => o.course);
  });

  let pane = $state<"course" | "app">(app.route.name === "course" ? "course" : "app");
  const close = () => (app.settingsOpen = false);
</script>

<Sheet title="Settings" onclose={close}>
  {#if courseId}
    <div class="segmented" role="tablist" aria-label="Settings for">
      <button type="button" role="tab" aria-selected={pane === "course"} onclick={() => (pane = "course")}>
        <span class="ellipsis">{course.value?.name ?? "Course"}</span>
      </button>
      <button type="button" role="tab" aria-selected={pane === "app"} onclick={() => (pane = "app")}>
        All courses
      </button>
    </div>
  {/if}

  {#if courseId && pane === "course"}
    {#if course.value}
      {#key course.value.id}
        <CourseSettings course={course.value} onclose={close} />
      {/key}
    {:else if course.error}
      <p class="error-text">{course.error}</p>
    {/if}
  {:else}
    <AppSettings />
  {/if}
</Sheet>
