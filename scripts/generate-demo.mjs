import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const root = path.join(repo, "fixtures/demo-store");
const files = [];
const header = (
  threadId,
  segmentId,
  title,
  project,
  relation,
  activity,
  extra = {},
) => ({
  kind: "agentbroom-demo-session",
  threadId,
  segmentId,
  title,
  project,
  relation,
  activity,
  updatedAt: "2026-10-08T09:30:00Z",
  parentId: null,
  sharedHistoryFrom: null,
  ...extra,
});
async function put(relative, category, text) {
  await mkdir(path.dirname(path.join(root, relative)), { recursive: true });
  await writeFile(path.join(root, relative), text);
  files.push({ path: relative, category });
}
async function session(relative, meta, repeats) {
  await put(
    relative,
    relative.startsWith("archives/") ? "archives" : "sessions",
    JSON.stringify(meta) +
      "\n" +
      JSON.stringify({
        kind: "synthetic-payload",
        text: "Sample tool output. No private conversation is included. ".repeat(
          repeats,
        ),
      }) +
      "\n",
  );
}
await session(
  "sessions/atlas-1.jsonl",
  header(
    "demo-atlas",
    "segment-atlas-1",
    "Build the Atlas dashboard",
    "atlas-web",
    "root",
    "completed",
  ),
  1800,
);
await session(
  "sessions/atlas-2.jsonl",
  header(
    "demo-atlas",
    "segment-atlas-2",
    "Build the Atlas dashboard",
    "atlas-web",
    "root",
    "completed",
  ),
  900,
);
await session(
  "sessions/atlas-helper.jsonl",
  header(
    "demo-atlas-helper",
    "segment-atlas-helper",
    "Inspect dashboard components",
    "atlas-web",
    "spawned",
    "completed",
    { parentId: "demo-atlas" },
  ),
  500,
);
await session(
  "sessions/atlas-fork.jsonl",
  header(
    "demo-atlas-fork",
    "segment-atlas-fork",
    "Explore an alternative layout",
    "atlas-web",
    "fork",
    "completed",
    { sharedHistoryFrom: "demo-atlas" },
  ),
  1100,
);
await session(
  "sessions/orbit.jsonl",
  header(
    "demo-orbit",
    "segment-orbit",
    "Refactor the Orbit API",
    "orbit-api",
    "root",
    "completed",
  ),
  1500,
);
await session(
  "sessions/orbit-tests.jsonl",
  header(
    "demo-orbit-tests",
    "segment-orbit-tests",
    "Review API test coverage",
    "orbit-api",
    "spawned",
    "completed",
    { parentId: "demo-orbit" },
  ),
  650,
);
await session(
  "sessions/sprout.jsonl",
  header(
    "demo-sprout",
    "segment-sprout",
    "Ship the Sprout landing page",
    "sprout-site",
    "root",
    "active",
  ),
  1200,
);
await session(
  "archives/notebook.jsonl",
  header(
    "demo-notebook",
    "segment-notebook",
    "Summarize a sample investigation",
    "notebook",
    "root",
    "archived",
  ),
  600,
);
await put(
  "sessions/oversized-header.jsonl",
  "sessions",
  "x".repeat(70000) + "\n",
);
await put(
  "archives/compressed.jsonl.zst",
  "archives",
  "Synthetic compressed placeholder: deliberately never decoded.\n".repeat(90),
);
await put(
  "thread_history_1.sqlite",
  "history",
  "Synthetic DB placeholder, stat only.\n".repeat(1500),
);
await put(
  "logs_1.sqlite-wal",
  "logs",
  "Synthetic WAL placeholder, stat only.\n".repeat(500),
);
await put(
  "generated_assets/sample.txt",
  "assets",
  "Synthetic generated artifact: inspection only.\n".repeat(420),
);
await mkdir(root, { recursive: true });
await writeFile(
  path.join(root, "fixture-manifest.json"),
  JSON.stringify(
    { kind: "agentbroom-synthetic-fixture", schemaVersion: 1, files },
    null,
    2,
  ) + "\n",
);
console.log(
  `Generated ${files.length} synthetic files in the repository fixture root.`,
);
