import { useRef, useState } from "react";
import { useLocale } from "./i18n";
import { bytes, type Inventory } from "./types";
import { selectable, cleanupReasons } from "./cleanup";
import { CleanupConfirmation } from "./CleanupPanel";
import {
  previewNative,
  executeNative,
  readNativeReceipt,
  type NativePlan,
} from "./nativeCleanup";
/** Only displayed after the native backend identifies its isolated test build. */
export default function NativeCleanupPanel({
  inventory,
  kept,
  refresh,
}: {
  inventory: Inventory;
  kept: string[];
  refresh: () => void;
}) {
  const { t, systemText } = useLocale();
  const [selected, setSelected] = useState<string[]>([]);
  const [plan, setPlan] = useState<NativePlan | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [stopped, setStopped] = useState(false);
  const generation = useRef(0);
  async function preview() {
    const current = ++generation.current;
    setBusy(true);
    setPlan(null);
    setError("");
    try {
      const next = await previewNative(selected, kept);
      if (current === generation.current) {
        if (next.testOnly !== true || !next.planId) throw Error("invalidPlan");
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
          t(
            runtimeStopped
              ? "The test runtime is stopped. Restart the test app to generate a new store."
              : "Native cleanup request failed. Create a fresh preview.",
          ),
        );
      }
    } finally {
      if (current === generation.current) setBusy(false);
    }
  }
  const title = (id: string) =>
    inventory.threads.find((thread) => thread.id === id)?.title ?? id;
  return (
    <section className="cleanup-page">
      <div className="scope-banner">
        <strong>{t("Synthetic native cleanup test")}</strong>
        <p>
          {t(
            "This build only uses a newly generated test store. Real Agent data is never loaded or deleted.",
          )}
        </p>
      </div>
      <section className="panel cleanup-candidates">
        <h2>{t("Cleanup candidates")}</h2>
        <p>
          {t(
            "Select one main conversation or independent fork per native plan.",
          )}
        </p>
        {inventory.threads.filter(selectable).map((thread) => (
          <label className="cleanup-candidate" key={thread.id}>
            <input
              type="radio"
              name="native-candidate"
              aria-label={t("Select {title}", { title: thread.title })}
              checked={selected[0] === thread.id}
              disabled={busy || confirm}
              onChange={() => {
                generation.current++;
                setSelected([thread.id]);
                setPlan(null);
                setError("");
              }}
            />
            <span>
              <strong>{thread.title}</strong>
              <small>
                {thread.id} · {systemText(thread.relation)}
              </small>
            </span>
            <span>{systemText(bytes(thread.inclusiveBytes))}</span>
          </label>
        ))}
      </section>
      <div className="cleanup-actions">
        <button
          className="button primary"
          disabled={stopped || busy || confirm || !selected.length}
          onClick={() => void preview()}
        >
          {t(busy ? "Creating native preview…" : "Preview impact")}
        </button>
      </div>
      {error && <p role="alert">{error}</p>}
      {plan && (
        <section className="panel cleanup-impact">
          <h2>{t("Impact preview")}</h2>
          <p>
            {t(
              "Plan expires after {seconds} seconds. Any file or state change requires a fresh preview.",
              { seconds: plan.expiresInSeconds },
            )}
          </p>
          <p>
            {t("{count} observed records · {files} measured files", {
              count: plan.nativeIds.length,
              files: plan.analysis.observedFiles.length,
            })}{" "}
            · {systemText(bytes(plan.analysis.observedLogicalBytes))}
          </p>
          <h3>{t("Observed main and spawned scope")}</h3>
          <ul>
            {plan.nativeIds.map((id) => (
              <li key={id}>
                {title(id)} · {id}
              </li>
            ))}
          </ul>
          <h3>{t("Retained conversations")}</h3>
          <ul>
            {plan.retainedIds.map((id) => (
              <li key={id}>
                {title(id)} · {id}
              </li>
            ))}
          </ul>
          <p>{t("Independent forks are never automatically selected.")}</p>
          <details>
            <summary>
              {t("Observed files ({count})", {
                count: plan.analysis.observedFiles.length,
              })}
            </summary>
            {plan.analysis.observedFiles.map((file) => (
              <p key={file.key}>{file.key}</p>
            ))}
          </details>
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
          <p>
            {t(
              "Native deletion is verified against files, thread rows, spawn edges and deleted notifications. No reclaimed-space estimate is available.",
            )}
          </p>
          <button
            className="button secondary"
            disabled={!plan.executable || confirm}
            onClick={() => setConfirm(true)}
          >
            {t("Review confirmation")}
          </button>
        </section>
      )}
      {confirm && plan && (
        <CleanupConfirmation
          plan={plan.analysis}
          close={() => {
            setConfirm(false);
            setPlan(null);
            setSelected([]);
            refresh();
          }}
          nativeDriver={{
            planId: plan.planId,
            testOnly: plan.testOnly,
            executable: plan.executable,
            execute: (confirmation) => executeNative(plan.planId, confirmation),
            readReceipt: () => readNativeReceipt(plan.planId),
          }}
        />
      )}
    </section>
  );
}
