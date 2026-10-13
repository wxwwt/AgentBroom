import { execFileSync } from "node:child_process";
import { writeFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const result = execFileSync(
  "cargo",
  ["run", "--quiet", "-p", "agentbroom-cli", "--", "web-catalog"],
  { cwd: root, encoding: "utf8", maxBuffer: 8 * 1024 * 1024 },
);
JSON.parse(result);
await mkdir(path.join(root, "apps/desktop/src/data"), { recursive: true });
await writeFile(path.join(root, "apps/desktop/src/data/catalog.json"), result);
console.log("Generated browser catalog from the Rust inspection core.");
