import { useEffect, useMemo, useRef, useState } from "react";
import { useLocale } from "./i18n";
import { bytes, type Inventory } from "./types";
import {
  selectable,
  cleanupReasons,
  conversationLocked,
  cleanupMessage,
} from "./cleanup";
import CleanupScope from "./CleanupScope";
import { CleanupConfirmation } from "./CleanupPanel";
import {
  previewNative,
  cancelNative,
  executeNative,
  readNativeReceipt,
  readNativeHistory,
  type NativeReceipt,
  type NativePlan,
} from "./nativeCleanup";
/** Native desktop flow; the backend identifies live versus generated-store builds. */
export default function NativeCleanupPanel({
  inventory,
  kept,
  refresh,
  testOnly = true,
  initialSelected,
  onClose,
}: {
  testOnly?: boolean;
  initialSelected?: string[];
  onClose?: () => void;
  inventory: Inventory;
  kept: string[];
  refresh: () => void;
}) {
  const { t, systemText } = useLocale();
  const [selected, setSelected] = useState<string[]>(initialSelected ?? []);
  const [plan, setPlan] = useState<NativePlan | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [stopped, setStopped] = useState(false);
  const generation = useRef(0);
  const [history, setHistory] = useState<NativeReceipt[]>([]);
  const [historyError, setHistoryError] = useState("");
  useEffect(() => {
    if (!testOnly && !initialSelected)
      void readNativeHistory()
        .then(setHistory)
        .catch(() => setHistoryError(t("Cleanup history could not be read.")));
  }, [testOnly]);
  async function inspectReceipt(id: string) {
    try {
      const latest = await readNativeReceipt(id);
      setHistory((items) =>
        items.map((item) => (item.planId === id ? latest : item)),
      );
    } catch {
      setHistoryError(
        t("No receipt is available yet. Do not repeat deletion."),
      );
    }
  }

  const latestPlan = useRef<string | null>(null);
  const [search, setSearch] = useState("");
  const [limit, setLimit] = useState(50);
  const recordsById = useMemo(
    () => new Map(inventory.threads.map((thread) => [thread.id, thread])),
    [inventory],
  );
  const lockedSet = useMemo(() => new Set(kept), [kept]);
  const isLocked = (id: string) =>
    conversationLocked(id, recordsById, lockedSet);
  useEffect(
    () => () => {
      if (latestPlan.current)
        void cancelNative(latestPlan.current).catch(() => {});
    },
    [],
  );
  async function preview() {
    const current = ++generation.current;
    setBusy(true);
    setPlan(null);
    setError("");
    try {
      const next = await previewNative(selected, kept);
      if (current === generation.current) {
        if (next.testOnly !== testOnly || !next.planId)
          throw Error("invalidPlan");
        latestPlan.current = next.planId;
        setPlan(next);
      }
    } catch (e) {
      const runtimeStopped =
        e &&
        typeof e === "object" &&
        "code" in e &&
        e.code === "runtimeStopped";
      if (current === generation.current) {
        if (runtimeStopped) setStopped(true);
        setError(
          runtimeStopped
            ? t(
                testOnly
                  ? "The test runtime is stopped. Restart the test app to generate a new store."
                  : "The native cleanup process stopped. Inspect the receipt before creating another plan.",
              )
            : systemText(
                cleanupMessage(
                  e && typeof e === "object" && "code" in e
                    ? String(e.code)
                    : "Native cleanup request failed. Create a fresh preview.",
                ),
              ),
        );
      }
    } finally {
      if (current === generation.current) setBusy(false);
    }
  }
  useEffect(() => {
    if (initialSelected?.length) void preview();
  }, []);
  const title = (id: string) => recordsById.get(id)?.title ?? id;
  return (
    <section className="cleanup-page">
      {onClose && (
        <div className="dialog-heading">
          <h2>{t("Impact preview")}</h2>
          <button
            className="button secondary"
            disabled={busy || confirm}
            onClick={() => {
              if (latestPlan.current) void cancelNative(latestPlan.current);
              onClose();
            }}
          >
            {t("Close preview")}
          </button>
        </div>
      )}
      {(!initialSelected || testOnly) && (
        <div className="scope-banner">
          <strong>
            {t(
              testOnly
                ? "Synthetic native cleanup test"
                : "Real data · manual cleanup",
            )}
          </strong>
          <p>
            {t(
              testOnly
                ? "This build only uses a newly generated test store. Real Agent data is never loaded or deleted."
                : "Nothing is selected automatically. Review the complete subtree and retained conversations before permanent deletion.",
            )}
          </p>
        </div>
      )}
      {!initialSelected && (
        <section className="panel cleanup-candidates">
          <h2>{t("Cleanup candidates")}</h2>
          <p>
            {t(
              "Select one conversation. Its native spawned subtree is included in the preview.",
            )}
          </p>
          <label className="search">
            <input
              value={search}
              aria-label={t("Search cleanup candidates")}
              placeholder={t("Search cleanup candidates")}
              onChange={(e) => {
                setSearch(e.target.value);
                setLimit(50);
              }}
            />
          </label>
          {inventory.threads
            .filter(selectable)
            .filter((thread) =>
              `${thread.title} ${thread.id}`
                .toLowerCase()
                .includes(search.toLowerCase()),
            )
            .sort((a, b) => b.inclusiveBytes - a.inclusiveBytes)
            .slice(0, limit)
            .map((thread) => (
              <label className="cleanup-candidate" key={thread.id}>
                <input
                  type={testOnly ? "radio" : "checkbox"}
                  name="native-candidate"
                  aria-label={t("Select {title}", { title: thread.title })}
                  checked={selected.includes(thread.id)}
                  disabled={busy || confirm || isLocked(thread.id)}
                  onChange={() => {
                    generation.current++;
                    setSelected((current) =>
                      testOnly
                        ? [thread.id]
                        : current.includes(thread.id)
                          ? current.filter((id) => id !== thread.id)
                          : [...current, thread.id],
                    );
                    setPlan(null);
                    setError("");
                  }}
                />
                <span>
                  <strong>{thread.title}</strong>
                  {isLocked(thread.id) && <small>{t("Locked")}</small>}
                  <small>
                    {thread.id} · {systemText(thread.relation)}
                  </small>
                </span>
                <span>{systemText(bytes(thread.inclusiveBytes))}</span>
              </label>
            ))}
          {inventory.threads.filter(selectable).length > limit && (
            <button
              className="text-button"
              onClick={() => setLimit((n) => n + 50)}
            >
              {t("Show 50 more")}
            </button>
          )}
        </section>
      )}
      {(!plan || busy) && (
        <div className="cleanup-actions">
          <button
            className="button primary"
            disabled={stopped || busy || confirm || !selected.length}
            onClick={() => void preview()}
          >
            {t(busy ? "Creating native preview…" : "Preview impact")}
          </button>
        </div>
      )}
      {error && <p role="alert">{error}</p>}
      {plan && (
        <section className="panel cleanup-impact">
          <div className="impact-summary">
            <div>
              <h2>{t("Conversations to delete")}</h2>
              <span className="muted">
                {t("{count} conversations · {files} files", {
                  count: plan.nativeIds.length,
                  files: plan.analysis.observedFiles.length,
                })}
              </span>
            </div>
            <strong>{bytes(plan.analysis.observedLogicalBytes)}</strong>
          </div>
          <CleanupScope
            ids={plan.nativeIds}
            selectedIds={plan.analysis.selectedIds}
            records={recordsById}
            files={plan.analysis.observedFiles}
          />
          <p className="retained-summary">
            {t("{count} other conversations will be kept.", {
              count: plan.retainedIds.length,
            })}
          </p>
          <p className="muted">
            {t("File sizes do not equal disk space reclaimed.")}
          </p>
          {!!plan.warnings?.length && (
            <>
              <h3>{t("Notices")}</h3>
              {plan.warnings.map((w) => (
                <p key={w}>{systemText(w)}</p>
              ))}
            </>
          )}
          {!!plan.lockedIds?.length && (
            <>
              <h3>{t("Locked conversations in this scope")}</h3>
              <ul>
                {plan.lockedIds.map((id) => (
                  <li key={id}>
                    {title(id)} · {id}
                  </li>
                ))}
              </ul>
            </>
          )}
          {!!plan.blockedReasons.length && (
            <>
              <h3>{t("Blocking reasons")}</h3>
              <ul>
                {plan.blockedReasons.map((code) => (
                  <li key={code}>
                    {code in cleanupReasons
                      ? t(cleanupReasons[code as keyof typeof cleanupReasons])
                      : systemText(code)}
                  </li>
                ))}
              </ul>
            </>
          )}
          <details className="preview-technical">
            <summary>{t("Files and technical details")}</summary>
            <p>
              {t(
                "Plan expires after {seconds} seconds. Any file or state change requires a fresh preview.",
                { seconds: plan.expiresInSeconds },
              )}
            </p>
            <p>{t("Independent forks are never automatically selected.")}</p>
            {plan.runtimeVersion && (
              <p>
                {t("Native runtime {version} · storage contract {profile}", {
                  version: plan.runtimeVersion,
                  profile: plan.storageProfile ?? "—",
                })}
              </p>
            )}
            {plan.nativeIds.map((id) => (
              <p className="file-path" key={id}>
                {title(id)} · {id}
              </p>
            ))}
            {plan.analysis.observedFiles.map((file) => (
              <p className="file-path" key={file.key}>
                {file.key} · {bytes(file.logicalBytes)}
              </p>
            ))}
          </details>
          <div className="cleanup-sticky-actions">
            <span className="muted">
              {t("Next step: review and type DELETE.")}
            </span>
            <div>
              <button
                className="button secondary"
                disabled={stopped || busy || confirm}
                onClick={() => void preview()}
              >
                {t("Refresh preview")}
              </button>
              <button
                className="button danger"
                disabled={!plan.executable || busy || confirm}
                onClick={() => setConfirm(true)}
              >
                {t("Continue to delete…")}
              </button>
            </div>
          </div>
        </section>
      )}
      {!testOnly && !initialSelected && (
        <details className="panel">
          <summary>
            {t("Recent cleanup results ({count})", { count: history.length })}
          </summary>
          {historyError && <p role="alert">{historyError}</p>}
          {history.map((receipt) => (
            <section key={receipt.planId}>
              <strong>
                {t(
                  receipt.status === "verified"
                    ? "Native cleanup verified"
                    : receipt.status === "unknown"
                      ? "Cleanup outcome is unknown. Do not repeat deletion."
                      : receipt.status === "partialFailure"
                        ? "Cleanup partially failed"
                        : "Cleanup verification failed",
                )}
              </strong>
              <p>{receipt.selectedIds.join(", ")}</p>
              <p>
                {t("{removed} removed · {remaining} remaining", {
                  removed: receipt.removed.length,
                  remaining: receipt.remaining.length,
                })}
              </p>
              {receipt.error && <p>{receipt.error}</p>}
              <button
                className="button secondary"
                onClick={() => void inspectReceipt(receipt.planId)}
              >
                {t("Check execution result")}
              </button>
            </section>
          ))}
        </details>
      )}
      {confirm && plan && (
        <CleanupConfirmation
          plan={plan.analysis}
          affectedIds={plan.nativeIds}
          records={recordsById}
          close={() => {
            void cancelNative(plan.planId).catch(() => {});
            latestPlan.current = null;
            setConfirm(false);
            setPlan(null);
            setSelected([]);
            refresh();
            onClose?.();
          }}
          nativeDriver={{
            planId: plan.planId,
            testOnly: plan.testOnly,
            executable: plan.executable,
            execute: (confirmation) =>
              executeNative(plan.planId, confirmation, kept),
            readReceipt: () => readNativeReceipt(plan.planId),
          }}
        />
      )}
    </section>
  );
}
