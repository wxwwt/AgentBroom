import { execFileSync } from "node:child_process";
import { writeFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const result = execFileSync(
  "cargo",
  ["run", "--quiet", "-p", "agentbroom-cli", "--", "fixtures", "web-catalog"],
  { cwd: root, encoding: "utf8", maxBuffer: 8 * 1024 * 1024 },
);
JSON.parse(result);
await mkdir(path.join(root, "apps/desktop/test-fixtures"), { recursive: true });
await writeFile(path.join(root, "apps/desktop/test-fixtures/catalog.json"), result);
console.log("Generated browser catalog from the Rust inspection core.");

const cleanup = execFileSync("cargo", ["run", "--quiet", "--locked", "-p", "agentbroom-cli", "--", "fixtures", "cleanup-catalog"], {cwd:root,encoding:"utf8",maxBuffer:8*1024*1024});
JSON.parse(cleanup);
await writeFile(path.join(root,"apps/desktop/test-fixtures/cleanup-results.json"),cleanup);
console.log("Generated synthetic cleanup expectations from the shared Rust planner.");
