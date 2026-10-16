import { useMemo, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useLocale } from "./i18n";
import { bytes, type Inventory } from "./types";
import {
  cleanupReasons,
  previewCleanup,
  selectable,
  type CleanupAnalysis,
} from "./cleanup";
export interface CleanupResult {
  status: "verified" | "partialFailure" | "verificationFailed" | "unknown";
  removed: string[];
  remaining: string[];
  observedRemovedLogicalBytes: number;
  freedBytes: null;
  error?: string;
  concurrentChanges?: string[];
}
/** The normal app has no driver; the dedicated native test build uses backend-owned plans. */
export function CleanupConfirmation({
  plan,
  close,
  testDriver,
  nativeDriver,
}: {
  plan: CleanupAnalysis;
  close: () => void;
  nativeDriver?: {
    planId: string;
    testOnly: boolean;
    executable: boolean;
    execute: (confirmation: {
      phrase: string;
      acknowledged: boolean;
      selectedIds: string[];
    }) => Promise<CleanupResult>;
    readReceipt: () => Promise<CleanupResult>;
  };
  testDriver?: (confirmation: {
    phrase: string;
    acknowledged: boolean;
    selectedIds: string[];
  }) => Promise<CleanupResult>;
}) {
  const { t, systemText } = useLocale();
  const [phrase, setPhrase] = useState(""),
    [ack, setAck] = useState(false),
    [running, setRunning] = useState(false),
    [result, setResult] = useState<CleanupResult | null>(null),
    [error, setError] = useState(""),
    [attempted, setAttempted] = useState(false);
  const submitted = useRef(false);
  const allowed =
    (!!testDriver &&
      plan.sourceKind === "synthetic" &&
      !plan.blockedReasons.length) ||
    (!!nativeDriver &&
      nativeDriver.executable &&
      !!nativeDriver.planId &&
      plan.sourceKind === "codex");
  async function execute() {
    if (
      !allowed ||
      submitted.current ||
      running ||
      attempted ||
      phrase !== "DELETE" ||
      !ack
    )
      return;
    submitted.current = true;
    setAttempted(true);
    setRunning(true);
    setError("");
    try {
      setError("");
      setResult(
        await (nativeDriver?.execute ?? testDriver!)({
          phrase,
          acknowledged: ack,
          selectedIds: plan.selectedIds,
        }),
      );
    } catch (e) {
      const code =
        e && typeof e === "object" && "code" in e ? String(e.code) : "";
      setError(
        t(
          code === "lockedConversation"
            ? "This conversation was locked after the preview. Create a new preview to review the locks."
            : nativeDriver &&
            [
              "planExpired",
              "staleSnapshot",
              "lockedConversation",
              "planBlocked",
              "invalidConfirmation",
              "invalidPlan",
              "cleanupBusy",
              "runtimeStopped",
            ].includes(code)
            ? "Cleanup was rejected before deletion. Create a fresh preview."
            : "Execution outcome is unknown. Do not automatically retry.",
        ),
      );
    } finally {
      setRunning(false);
    }
  }
  async function readReceipt() {
    if (!nativeDriver || running) return;
    setRunning(true);
    try {
      setResult(await nativeDriver.readReceipt());
      setError("");
    } catch {
      setError(t("No receipt is available yet. Do not repeat deletion."));
    } finally {
      setRunning(false);
    }
  }
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open && !running) close();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="cleanup-overlay" />
        <Dialog.Content
          className="cleanup-dialog"
          onEscapeKeyDown={(e) => {
            if (running) e.preventDefault();
          }}
          onPointerDownOutside={(e) => {
            if (running) e.preventDefault();
          }}
        >
          <Dialog.Title>
            {t(allowed ? "Confirm cleanup" : "Cleanup execution unavailable")}
          </Dialog.Title>
          <Dialog.Description>
            {t(
              allowed
                ? "Permanent deletion cannot be undone. A receipt is not a backup."
                : "Real Codex deletion is locked in this release; native effects are not fully verified.",
            )}
          </Dialog.Description>
          {nativeDriver && (
            <p className="scope-banner">
              {t(
                nativeDriver.testOnly
                  ? "This build only uses a newly generated test store. Real Agent data is never loaded or deleted."
                  : "This operation permanently deletes real conversations and their native metadata. A receipt is not a backup.",
              )}
            </p>
          )}
          {!allowed && (
            <p className="scope-banner">
              {t("Confirming here will not delete local Codex data.")}
            </p>
          )}
          <p>
            {t("{count} observed records · {files} measured files", {
              count: plan.includedIds.length,
              files: plan.observedFiles.length,
            })}
          </p>
          <details open>
            <summary>{t("Observed main and spawned scope")}</summary>
            <ul>
              {plan.includedIds.map((id) => (
                <li key={id}>{id}</li>
              ))}
            </ul>
          </details>
          <details>
            <summary>
              {t("Observed files ({count})", {
                count: plan.observedFiles.length,
              })}
            </summary>
            {plan.observedFiles.map((f) => (
              <p key={f.key}>{f.key}</p>
            ))}
          </details>
          {allowed && (
            <>
              <label>
                <input
                  type="checkbox"
                  checked={ack}
                  disabled={running || !!result}
                  onChange={(e) => setAck(e.target.checked)}
                />
                {t(
                  "I reviewed the scope and understand deletion is permanent.",
                )}
              </label>
              <label>
                {t("Type DELETE to confirm")}
                <input
                  value={phrase}
                  disabled={running || !!result}
                  onChange={(e) => setPhrase(e.target.value)}
                  aria-label={t("Type DELETE to confirm")}
                />
              </label>
            </>
          )}
          {running && <p role="status">{t("Executing and verifying…")}</p>}
          {error && <p role="alert">{error}</p>}
          {result && (
            <div role={result.status === "verified" ? "status" : "alert"}>
              <strong>
                {t(
                  result.status === "verified"
                    ? nativeDriver
                      ? nativeDriver.testOnly
                        ? "Native synthetic cleanup verified"
                        : "Native cleanup verified"
                      : "Synthetic cleanup verified"
                    : result.status === "unknown"
                      ? "Cleanup outcome is unknown. Do not repeat deletion."
                      : result.status === "partialFailure"
                        ? "Cleanup partially failed"
                        : "Cleanup verification failed",
                )}
              </strong>
              <p>
                {t(
                  nativeDriver
                    ? result.status === "verified"
                      ? nativeDriver.testOnly
                        ? "Official Codex deletion ran only against generated test data."
                        : "Official native cleanup completed for the confirmed scope."
                      : "Native deletion was not fully verified. Inspect the result before closing."
                    : result.removed.length
                      ? "Synthetic files actually removed; this is not verification of native Codex deletion."
                      : "No synthetic files were removed.",
                )}
              </p>
              {nativeDriver && result.status === "verified" && (
                <p>
                  {t(
                    nativeDriver.testOnly
                      ? "Verified: deleted notifications, remaining file hashes, thread rows and spawn edges."
                      : "Verified: deletion scope, native state and retained files; concurrent additions are tracked separately.",
                  )}
                </p>
              )}
              {!!result.concurrentChanges?.length && (
                <details>
                  <summary>
                    {t("Concurrent changes detected ({count})", {
                      count: result.concurrentChanges.length,
                    })}
                  </summary>
                  {result.concurrentChanges.map((path, i) => (
                    <p key={i}>{path}</p>
                  ))}
                </details>
              )}
              {result.error && (
                <p>{t("Verification error: {code}", { code: result.error })}</p>
              )}
              <p>
                {t("{removed} removed · {remaining} remaining", {
                  removed: result.removed.length,
                  remaining: result.remaining.length,
                })}
              </p>
              <p>
                {systemText(bytes(result.observedRemovedLogicalBytes))} ·{" "}
                {t("Removed file lengths, not disk space reclaimed.")}
              </p>
              {result.removed.length > 0 && (
                <details>
                  <summary>{t("Removed files")}</summary>
                  {result.removed.map((f) => (
                    <p key={f}>{f}</p>
                  ))}
                </details>
              )}
              {result.remaining.length > 0 && (
                <details>
                  <summary>{t("Remaining files")}</summary>
                  {result.remaining.map((f) => (
                    <p key={f}>{f}</p>
                  ))}
                </details>
              )}
            </div>
          )}
          {nativeDriver && attempted && !result && (
            <button
              className="button secondary"
              disabled={running}
              onClick={() => void readReceipt()}
            >
              {t("Check execution result")}
            </button>
          )}
          <div className="cleanup-actions">
            <button
              className="button secondary"
              disabled={running}
              onClick={close}
            >
              {t("Close dialog")}
            </button>
            {allowed && (
              <button
                className="button primary"
                disabled={
                  !allowed ||
                  running ||
                  attempted ||
                  !!result ||
                  !ack ||
                  phrase !== "DELETE"
                }
                onClick={() => void execute()}
              >
                {t("Confirm execution")}
              </button>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export default function CleanupPanel({
  inventory,
  kept,
}: {
  inventory: Inventory;
  kept: string[];
}) {
  const { t, systemText } = useLocale();
  const [selected, setSelected] = useState<string[]>([]),
    [preview, setPreview] = useState<CleanupAnalysis | null>(null),
    [confirm, setConfirm] = useState(false),
    [error, setError] = useState(""),
    [search, setSearch] = useState(""),
    [limit, setLimit] = useState(50);
  const recordsById = useMemo(
    () => new Map(inventory.threads.map((record) => [record.id, record])),
    [inventory],
  );
  const candidates = useMemo(
    () =>
      inventory.threads
        .filter(selectable)
        .sort(
          (a, b) =>
            b.inclusiveBytes - a.inclusiveBytes || a.id.localeCompare(b.id),
        ),
    [inventory],
  );
  const shown = candidates.filter((c) =>
    `${c.title} ${c.id} ${c.project}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  function select(id: string) {
    setSelected((s) =>
      s.includes(id)
        ? s.filter((x) => x !== id)
        : s.length < 128
          ? [...s, id]
          : s,
    );
    setPreview(null);
    setConfirm(false);
    setError("");
  }
  function review() {
    try {
      setPreview(previewCleanup(inventory, selected, kept));
      setError("");
    } catch (e) {
      const code = e instanceof Error ? e.message : "";
      setError(
        code in cleanupReasons
          ? t(cleanupReasons[code as keyof typeof cleanupReasons])
          : t("Local inspection failed."),
      );
    }
  }
  const title = (id: string) => recordsById.get(id)?.title ?? id;
  return (
    <section className="cleanup-page">
      <div className="scope-banner">
        <strong>
          {t(
            "Real Codex deletion is locked in this release; native effects are not fully verified.",
          )}
        </strong>
        <p>{t("Review candidates; no automatic selection.")}</p>
      </div>
      <div className="toolbar">
        <label className="search">
          <input
            aria-label={t("Search cleanup candidates")}
            placeholder={t("Search cleanup candidates")}
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setLimit(50);
            }}
          />
        </label>
        <span>
          {t("{count} review candidates", { count: candidates.length })}
        </span>
      </div>
      <section className="panel cleanup-candidates">
        <h2>{t("Cleanup candidates")}</h2>
        <p>
          {t(
            "Candidates are for review, not a safety approval. Sub Agents are included through their main conversation.",
          )}
        </p>
        {shown.slice(0, limit).map((c) => (
          <label className="cleanup-candidate" key={c.id}>
            <input
              type="checkbox"
              aria-label={t("Select {title}", { title: c.title })}
              checked={selected.includes(c.id)}
              disabled={!selected.includes(c.id) && selected.length >= 128}
              onChange={() => select(c.id)}
            />
            <span>
              <strong>{c.title}</strong>
              <small>
                {systemText(c.relation)} · {c.id}
                {kept.includes(c.id) ? ` · ${t("Kept this session")}` : ""}
              </small>
            </span>
            <span>{systemText(bytes(c.inclusiveBytes))}</span>
          </label>
        ))}
        {!shown.length && (
          <p>{t("No selectable main or fork conversations.")}</p>
        )}
        {shown.length > limit && (
          <button
            className="text-button"
            onClick={() => setLimit((l) => l + 50)}
          >
            {t("Show 50 more")}
          </button>
        )}
      </section>
      <div className="cleanup-actions">
        <span>
          {t("{count} selected conversations", { count: selected.length })}
        </span>
        <button
          className="button secondary"
          onClick={() => {
            setSelected([]);
            setPreview(null);
            setConfirm(false);
            setError("");
          }}
        >
          {t("Clear selection")}
        </button>
        <button
          className="button primary"
          disabled={!selected.length}
          onClick={review}
        >
          {t("Preview impact")}
        </button>
      </div>
      {error && <p role="alert">{error}</p>}
      {preview && (
        <section className="panel cleanup-impact">
          <h2>{t("Impact preview")}</h2>
          <p>{t("Observed scope, not an executable deletion list.")}</p>
          <p>
            {t("{count} observed records · {files} measured files", {
              count: preview.includedIds.length,
              files: preview.observedFiles.length,
            })}{" "}
            · {systemText(bytes(preview.observedLogicalBytes))}
          </p>
          <h3>{t("Blocking reasons")}</h3>
          <ul>
            {preview.blockedReasons.map((code) => (
              <li key={code}>
                {code in cleanupReasons
                  ? t(cleanupReasons[code as keyof typeof cleanupReasons])
                  : code}
              </li>
            ))}
          </ul>
          <h3>{t("Observed main and spawned scope")}</h3>
          <ul>
            {preview.includedIds.map((id) => (
              <li key={id}>
                {title(id)} · {id} · {systemText(recordsById.get(id)!.relation)}
              </li>
            ))}
          </ul>
          <details open>
            <summary>{t("Retained — this plan cannot run.")}</summary>
            {preview.retainedIds.map((id) => (
              <p key={id}>
                {title(id)} · {id}
                {kept.includes(id) ? ` · ${t("Kept this session")}` : ""}
              </p>
            ))}
          </details>
          <p>{t("Independent forks are never automatically selected.")}</p>
          <details open>
            <summary>{t("Outside dependencies retained")}</summary>
            {!preview.outsideDependents.length && (
              <p>
                {t(
                  "No observed outside dependencies. Native reference coverage remains unverified.",
                )}
              </p>
            )}
            {preview.outsideDependents.map((id) => (
              <p key={id}>
                {title(id)} · {id}
              </p>
            ))}
          </details>
          <details>
            <summary>
              {t("Observed files ({count})", {
                count: preview.observedFiles.length,
              })}
            </summary>
            {preview.observedFiles.map((f) => (
              <p key={f.key}>
                {f.key} · {systemText(bytes(f.logicalBytes))}
              </p>
            ))}
          </details>
          <p>
            {t(
              "Database, index and asset changes are not fully covered. No reclaimed-space estimate is available.",
            )}
          </p>
          <button className="button secondary" onClick={() => setConfirm(true)}>
            {t("Review confirmation")}
          </button>
        </section>
      )}
      {confirm && preview && (
        <CleanupConfirmation plan={preview} close={() => setConfirm(false)} />
      )}
    </section>
  );
}
