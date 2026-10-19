import { useEffect, useRef, useState } from "react";
import { useLocale } from "./i18n";
import { bytes } from "./types";
import {
  cancelDiagnostics,
  previewDiagnostics,
  executeDiagnostics,
  readDiagnosticResult,
  type DiagnosticInventory,
  type DiagnosticPlan,
  type DiagnosticResult,
} from "./nativeCleanup";
export default function DiagnosticPanel({
  inventory,
  close,
  refresh,
}: {
  inventory: DiagnosticInventory;
  close: () => void;
  refresh: () => void;
}) {
  const { t, systemText } = useLocale();
  const [selected, setSelected] = useState<string[]>([]),
    [plan, setPlan] = useState<DiagnosticPlan | null>(null),
    [phrase, setPhrase] = useState(""),
    [ack, setAck] = useState(false),
    [busy, setBusy] = useState(false),
    [attempted, setAttempted] = useState(false),
    [result, setResult] = useState<DiagnosticResult | null>(null),
    [error, setError] = useState(""),
    [rejected, setRejected] = useState(false);
  const submitted = useRef(false),
    active = useRef<string | null>(null);
  useEffect(
    () => () => {
      if (active.current)
        void cancelDiagnostics(active.current).catch(() => {});
    },
    [],
  );
  async function preview() {
    setBusy(true);
    setError("");
    setPhrase("");
    setAck(false);
    setAttempted(false);
    setRejected(false);
    submitted.current = false;
    setResult(null);
    try {
      const p = await previewDiagnostics(selected);
      active.current = p.planId;
      setPlan(p);
    } catch {
      setError(
        t("Crash reports changed or could not be read. Rescan and try again."),
      );
    } finally {
      setBusy(false);
    }
  }
  async function execute() {
    if (!plan || busy || submitted.current) return;
    submitted.current = true;
    setBusy(true);
    setAttempted(true);
    try {
      setResult(await executeDiagnostics(plan.planId, phrase, ack));
    } catch (e) {
      const code =
        e && typeof e === "object" && "code" in e ? String(e.code) : "";
      if (
        [
          "planExpired",
          "reportChanged",
          "invalidPlan",
          "invalidConfirmation",
          "cleanupBusy",
          "linkedOrSpecialFile",
        ].includes(code)
      ) {
        setRejected(true);
        setError(
          t(
            "Nothing was deleted. Create a fresh preview before confirming again.",
          ),
        );
      } else {
        setError(
          t("Execution outcome is unknown. Do not automatically retry."),
        );
      }
    } finally {
      setBusy(false);
    }
  }
  async function read() {
    if (!plan) return;
    setBusy(true);
    try {
      setResult(await readDiagnosticResult(plan.planId));
      setError("");
    } catch {
      setError(t("No receipt is available yet. Do not repeat deletion."));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="panel report-panel">
      <div className="dialog-heading">
        <h2>{t("Crash reports")}</h2>
        <button
          className="button secondary"
          disabled={busy}
          onClick={() => {
            close();
            if (attempted) refresh();
          }}
        >
          {t("Close dialog")}
        </button>
      </div>
      <p className="muted">
        {t(
          "Only identified Codex crash reports are included. Other applications are excluded.",
        )}
      </p>
      {!plan && (
        <>
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>{t("Select")}</th>
                  <th>{t("File")}</th>
                  <th>{t("Location")}</th>
                  <th>{t("Size")}</th>
                </tr>
              </thead>
              <tbody>
                {inventory.files.map((f) => (
                  <tr key={f.key}>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={t("Select {title}", { title: f.name })}
                        disabled={busy}
                        checked={selected.includes(f.key)}
                        onChange={() =>
                          setSelected((s) =>
                            s.includes(f.key)
                              ? s.filter((k) => k !== f.key)
                              : [...s, f.key],
                          )
                        }
                      />
                    </td>
                    <td>{f.name}</td>
                    <td>{systemText(f.location)}</td>
                    <td>{bytes(f.bytes)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {!inventory.files.length && (
            <p>{t("No Codex crash reports found.")}</p>
          )}
          <button
            className="button danger"
            disabled={!selected.length || busy}
            onClick={() => void preview()}
          >
            {t("Preview impact")}
          </button>
        </>
      )}
      {plan && (
        <>
          <h3>{t("Review selected crash reports")}</h3>
          <p>
            {plan.files.length} · {bytes(plan.bytes)}
          </p>
          <ul className="file-list">
            {plan.files.map((f) => (
              <li key={f.key}>
                {f.name} · {systemText(f.location)} · {bytes(f.bytes)}
              </li>
            ))}
          </ul>
          <p>
            {t(
              "Permanent deletion cannot be undone. A receipt is not a backup.",
            )}
          </p>
          <label className="ack">
            <input
              type="checkbox"
              disabled={busy || attempted}
              checked={ack}
              onChange={(e) => setAck(e.target.checked)}
            />
            {t("I have reviewed the selected files.")}
          </label>
          <label>
            {t("Type DELETE to confirm")}
            <input
              className="confirm-input"
              aria-label={t("Type DELETE to confirm")}
              disabled={busy || attempted}
              value={phrase}
              onChange={(e) => setPhrase(e.target.value)}
            />
          </label>
          <button
            className="button danger"
            disabled={busy || attempted || !ack || phrase !== "DELETE"}
            onClick={() => void execute()}
          >
            {t("Confirm execution")}
          </button>
        </>
      )}
      {error && <p role="alert">{error}</p>}
      {rejected && (
        <button
          className="button secondary"
          disabled={busy}
          onClick={() => void preview()}
        >
          {t("Create fresh preview")}
        </button>
      )}
      {attempted && !rejected && !result && (
        <button
          className="button secondary"
          disabled={busy}
          onClick={() => void read()}
        >
          {t("Check execution result")}
        </button>
      )}
      {result && (
        <div
          role="status"
          className={
            result.status === "verified" ? "result success" : "result warning"
          }
        >
          <strong>
            {t(
              result.status === "verified"
                ? "Crash report cleanup verified"
                : result.status === "partialFailure"
                  ? "Cleanup partially failed"
                  : result.status === "unknown"
                    ? "Cleanup outcome is unknown. Do not repeat deletion."
                    : "Cleanup verification failed",
            )}
          </strong>
          <p>
            {t("{removed} removed · {remaining} remaining", {
              removed: result.removed.length,
              remaining: result.remaining.length,
            })}
          </p>
          {result.warnings?.map((warning, i) => (
            <p key={i}>{systemText(warning)}</p>
          ))}
          {result.error && <p>{systemText(result.error)}</p>}
        </div>
      )}
    </section>
  );
}
