// Runs the Tauri CLI from app/ui. The CLI looks for src-tauri under the app dir, which is the
// parent of this package, so point TAURI_APP_PATH at it (works the same on Windows and macOS).
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const cli = require.resolve("@tauri-apps/cli/tauri.js");
const appDir = fileURLToPath(new URL("../..", import.meta.url));
const child = spawn(process.execPath, [cli, ...process.argv.slice(2)], {
  stdio: "inherit",
  env: { ...process.env, TAURI_APP_PATH: appDir },
});
child.on("exit", (code) => process.exit(code ?? 1));
