import { isTauri } from "@tauri-apps/api/core";

/**
 * Which window chrome the app draws itself. Windows: the window is frameless, so the toolbar holds
 * minimize / maximize / close. macOS: the title bar is an overlay; leave room for the traffic
 * lights. Anywhere else (browser, Linux): native chrome, draw nothing.
 */
export const chrome: "windows" | "mac" | "none" = !isTauri()
  ? "none"
  : /Windows/i.test(navigator.userAgent)
    ? "windows"
    : /Macintosh|Mac OS X/i.test(navigator.userAgent)
      ? "mac"
      : "none";
