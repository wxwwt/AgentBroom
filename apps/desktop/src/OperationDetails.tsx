import { useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { useLocale, type MessageKey } from "./i18n";
import { cleanupMessage } from "./cleanup";
import { bytes, type Thread } from "./types";
import {
  readNativeReceipt,
  readDiagnosticResult,
  type OperationEvent,
  type NativeReceipt,
  type DiagnosticResult,
} from "./nativeCleanup";

export function operationHasDetails(event: OperationEvent) {
  return (
    event.action === "cleanup" ||
    event.action === "crashCleanup" ||
    !!event.error ||
    event.ids.length > 1 ||
    (event.action === "preview" && event.bytes > 0)
  );
}
export const operationActionLabels: Record<string, MessageKey> = {
  scan: "Storage scan",
  preview: "Cleanup preview",
  lock: "Conversation locked",
  unlock: "Conversation unlocked",
  cleanup: "Conversation cleanup",
  crashCleanup: "Crash report cleanup",
};
export const operationStatusLabels: Record<string, MessageKey> = {
  verified: "Completed",
  ready: "Ready",
  blocked: "Blocked",
  started: "In progress",
  unknown: "Needs verification",
  partialFailure: "Cleanup partially failed",
  verificationFailed: "Cleanup verification failed",
};
export default function OperationDetails({
  event,
  records,
  close,
}: {
  event: OperationEvent;
  records: ReadonlyMap<string, Thread>;
  close: () => void;
}) {
  const { t, systemText, locale } = useLocale();
  const [receipt, setReceipt] = useState<
    NativeReceipt | DiagnosticResult | null
  >(null);
  const [loading, setLoading] = useState(false),
    [failed, setFailed] = useState(false),
    [revision, setRevision] = useState(0);
  const wantsReceipt =
    event.action === "crashCleanup" ||
    (event.action === "cleanup" && event.status !== "blocked");
  useEffect(() => {
    let current = true;
    setReceipt(null);
    setFailed(false);
    setLoading(wantsReceipt);
    if (wantsReceipt) {
      void Promise.resolve()
        .then<NativeReceipt | DiagnosticResult>(() =>
          event.action === "cleanup"
            ? readNativeReceipt(event.id)
            : readDiagnosticResult(event.id),
        )
        .then((value) => {
          if (value.planId !== event.id) throw new Error("invalidReceipt");
          if (current) setReceipt(value);
        })
        .catch(() => {
          if (current) setFailed(true);
        })
        .finally(() => {
          if (current) setLoading(false);
        });
    }
    return () => {
      current = false;
    };
  }, [event.id, event.action, wantsReceipt, revision]);
  const native = receipt && "selectedIds" in receipt ? receipt : null;
  const status = receipt?.status ?? event.status;
  const title = (id: string) => {
    const i = event.ids.indexOf(id);
    return (
      (i >= 0 ? event.titles[i] : undefined) || records.get(id)?.title || id
    );
  };
  const reason = (text: string) =>
    text
      .split(",")
      .map((code) => systemText(cleanupMessage(code.trim())))
      .join(" · ");
  const errors = [
    ...new Set([event.error, receipt?.error].filter((s): s is string => !!s)),
  ];
  const scope = native?.expectedIds?.length
    ? native.expectedIds
    : (native?.selectedIds ?? event.ids);
  const scopeLabel: MessageKey =
    event.action === "crashCleanup"
      ? "Selected reports"
      : native?.expectedIds?.length
        ? "Recorded cleanup scope"
        : "Selected conversations";
  const checks: Array<[MessageKey, boolean | undefined]> = native
    ? [
        ["Native deletion notifications", native.nativeNotificationsVerified],
        ["File verification", native.filesVerified],
        ["Conversation index verification", native.stateVerified],
        ["Associated data verification", native.relatedVerified],
      ]
    : [];
  function entryList(label: MessageKey, files: string[]) {
    return (
      <details className="operation-file-details">
        <summary>
          {t(label)} ({files.length})
        </summary>
        {files.length ? (
          <ul className="operation-file-list">
            {files.map((path, i) => (
              <li className="file-path" key={`${path}-${i}`}>
                {path}
              </li>
            ))}
          </ul>
        ) : (
          <p className="muted">{t("No entries recorded.")}</p>
        )}
      </details>
    );
  }
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="cleanup-overlay" />
        <Dialog.Content
          className="cleanup-dialog operation-details"
          aria-describedby="operation-summary"
        >
          <div className="dialog-heading">
            <Dialog.Title>
              {t(operationActionLabels[event.action] ?? "Operation")}
            </Dialog.Title>
            <Dialog.Close asChild>
              <button className="icon-button" aria-label={t("Close details")}>
                <X />
              </button>
            </Dialog.Close>
          </div>
          <Dialog.Description id="operation-summary" className="muted">
            {new Date(event.at * 1000).toLocaleString(locale)} ·{" "}
            {t(operationStatusLabels[status] ?? "Needs verification")}
          </Dialog.Description>
          {event.status === "blocked" && event.action === "cleanup" && (
            <p className="notice">{t("Deletion did not start.")}</p>
          )}
          {loading && (
            <p role="status">{t("Loading saved execution receipt…")}</p>
          )}
          {failed && (
            <div className="alert" role="alert">
              <p>
                {t(
                  "The saved execution receipt could not be read. Only the operation log is shown.",
                )}
              </p>
              <button
                className="text-button"
                onClick={() => setRevision((n) => n + 1)}
              >
                {t("Retry reading receipt")}
              </button>
            </div>
          )}
          {status === "unknown" && (
            <p className="alert">
              {t("Cleanup outcome is unknown. Do not repeat deletion.")}
            </p>
          )}
          {!!errors.length && (
            <section className="operation-errors" role="alert">
              <h3>{t("Recorded error")}</h3>
              {errors.map((error) => (
                <p key={error}>{reason(error)}</p>
              ))}
              <details>
                <summary>{t("Error codes")}</summary>
                {errors.map((error) => (
                  <code key={error}>{error}</code>
                ))}
              </details>
            </section>
          )}
          {!!scope.length && (
            <section>
              <h3>
                {t(scopeLabel)} ({scope.length})
              </h3>
              <ul className="operation-scope-list">
                {scope.map((id) => (
                  <li key={id}>
                    <strong>{title(id)}</strong>
                    {title(id) !== id && (
                      <span className="file-path">{id}</span>
                    )}
                  </li>
                ))}
              </ul>
            </section>
          )}
          {event.action === "preview" && (
            <p>
              {t("Observed scope")}: {event.count} ·{" "}
              {t("Measured file lengths")}: {bytes(event.bytes)}
            </p>
          )}
          {receipt ? (
            <>
              <div className="operation-metrics">
                <div>
                  {t("Observed removed files")}
                  <strong>{receipt.removed.length}</strong>
                </div>
                <div>
                  {t("Remaining files")}
                  <strong>{receipt.remaining.length}</strong>
                </div>
                <div>
                  {t("Observed removed file lengths")}
                  <strong>{bytes(receipt.observedRemovedLogicalBytes)}</strong>
                </div>
              </div>
              <p className="tiny-note">
                {t("Removed file lengths, not disk space reclaimed.")}
              </p>
              {!!checks.length && (
                <section>
                  <h3>{t("Verification results")}</h3>
                  <dl className="verification-list">
                    {checks.map(([label, passed]) => (
                      <div key={label}>
                        <dt>{t(label)}</dt>
                        <dd
                          className={
                            passed === true
                              ? "verification-pass"
                              : "verification-attention"
                          }
                        >
                          {t(
                            passed === true
                              ? "Passed"
                              : passed === false
                                ? "Not verified"
                                : "Not recorded",
                          )}
                        </dd>
                      </div>
                    ))}
                  </dl>
                  {native &&
                    entryList(
                      "Native deletion notifications",
                      native.nativeDeletedIds,
                    )}
                </section>
              )}
              {entryList("Removed files", receipt.removed)}
              {entryList("Remaining files", receipt.remaining)}
              {!!receipt.warnings?.length && (
                <section>
                  <h3>{t("Warnings")}</h3>
                  <ul>
                    {receipt.warnings.map((w, i) => (
                      <li key={i}>{systemText(w)}</li>
                    ))}
                  </ul>
                </section>
              )}
              {!!native?.concurrentChanges.length && (
                <section>
                  <h3>{t("Concurrent changes")}</h3>
                  <ul className="operation-file-list">
                    {native.concurrentChanges.map((path, i) => (
                      <li className="file-path" key={i}>
                        {systemText(path)}
                      </li>
                    ))}
                  </ul>
                </section>
              )}
            </>
          ) : (
            !loading &&
            event.action !== "preview" &&
            event.bytes > 0 && (
              <p>
                {t("Observed removed file lengths")}: {bytes(event.bytes)}
              </p>
            )
          )}
          <details className="preview-technical">
            <summary>{t("Operation identifier")}</summary>
            <p className="file-path">{event.id}</p>
          </details>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
