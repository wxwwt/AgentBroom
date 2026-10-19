import { useEffect, useMemo, useRef, useState } from "react";
import {
  Broom,
  Sun,
  Moon,
  RefreshCw,
  Search,
  LockKeyhole,
  LockOpen,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  FileText,
  GitBranch,
  X,
  Clock3,
  AlertCircle,
  ArrowUpDown,
  Trash2,
} from "lucide-react";
import {
  LocaleProvider,
  LanguageSelector,
  useLocale,
  type MessageKey,
} from "./i18n";
import { ThemeProvider, ThemeToggle } from "./theme";
import { loadInventory, loadCleanupMode, errorText } from "./bridge";
import { conversationLocked, cleanupMessage, selectable } from "./cleanup";
import { bytes, type Inventory, type Thread } from "./types";
import { sortThreads, type SortKey } from "./viewModel";
import {
  readConversationLocks,
  setConversationLocks,
  readDiagnostics,
  readOperations,
  type DiagnosticInventory,
  type OperationEvent,
} from "./nativeCleanup";
import NativeCleanupPanel from "./NativeCleanupPanel";
import DiagnosticPanel from "./DiagnosticPanel";
type Page = "overview" | "conversations" | "activity";
const categories: Record<string, MessageKey> = {
  sessions: "Session files",
  archives: "Archived sessions",
  history: "History database",
  logs: "Logs & WAL",
  assets: "Generated assets",
  worktrees: "Worktrees",
  cache: "Tools & cache",
  other: "Other local files",
};
const actions: Record<string, MessageKey> = {
  scan: "Storage scan",
  preview: "Cleanup preview",
  lock: "Conversation locked",
  unlock: "Conversation unlocked",
  cleanup: "Conversation cleanup",
  crashCleanup: "Crash report cleanup",
};
const statuses: Record<string, MessageKey> = {
  verified: "Completed",
  ready: "Ready",
  blocked: "Blocked",
  started: "In progress",
  unknown: "Needs verification",
  partialFailure: "Cleanup partially failed",
  verificationFailed: "Cleanup verification failed",
};
export default function App() {
  return (
    <LocaleProvider>
      <ThemeProvider>
        <Workspace />
      </ThemeProvider>
    </LocaleProvider>
  );
}
function Workspace() {
  const { t, systemText, locale, persistenceUnavailable } = useLocale();
  const [page, setPage] = useState<Page>("overview"),
    [inventory, setInventory] = useState<Inventory | null>(null),
    [mode, setMode] = useState<"readOnly" | "syntheticNativeTest" | "native">(
      "readOnly",
    ),
    [loading, setLoading] = useState(true),
    [scanAt, setScanAt] = useState<number | null>(null),
    [error, setError] = useState("");
  const [locked, setLocked] = useState<string[]>([]),
    [lockFailure, setLockFailure] = useState(false),
    [busy, setBusy] = useState(false),
    [operationError, setOperationError] = useState("");
  const [diagnostics, setDiagnostics] = useState<DiagnosticInventory | null>(
      null,
    ),
    [diagnosticError, setDiagnosticError] = useState("");
  const [query, setQuery] = useState(""),
    [filter, setFilter] = useState("all"),
    [sort, setSort] = useState<SortKey>("updatedDesc"),
    [offset, setOffset] = useState(0),
    [selected, setSelected] = useState<string[]>([]),
    [detail, setDetail] = useState<string | null>(null);
  const [cleanupIds, setCleanupIds] = useState<string[] | null>(null),
    [reportsOpen, setReportsOpen] = useState(false),
    [events, setEvents] = useState<OperationEvent[]>([]),
    [historyError, setHistoryError] = useState(""),
    [eventDetail, setEventDetail] = useState<string | null>(null);
  const generation = useRef(0);
  const message = (e: unknown) =>
    systemText(
      cleanupMessage(
        e && typeof e === "object" && "code" in e
          ? String(e.code)
          : errorText(e),
      ),
    );
  async function history() {
    try {
      setEvents(await readOperations());
      setHistoryError("");
    } catch (e) {
      setHistoryError(message(e));
    }
  }
  async function rescan() {
    const seq = ++generation.current;
    setLoading(true);
    setError("");
    setOperationError("");
    setSelected([]);
    setCleanupIds(null);
    setDetail(null);
    try {
      const [next, nextMode] = await Promise.all([
        loadInventory(),
        loadCleanupMode(),
      ]);
      if (seq !== generation.current) return;
      setInventory(next);
      setScanAt(Date.now());
      setMode(nextMode);
      if (nextMode === "native") {
        try {
          const ids = await readConversationLocks();
          if (seq !== generation.current) return;
          setLocked(ids);
          setLockFailure(false);
        } catch (e) {
          if (seq !== generation.current) return;
          setLockFailure(true);
          setOperationError(message(e));
        }
        try {
          const reports = await readDiagnostics();
          if (seq !== generation.current) return;
          setDiagnostics(reports);
          setDiagnosticError("");
        } catch (e) {
          if (seq !== generation.current) return;
          setDiagnostics(null);
          setDiagnosticError(message(e));
        }
      } else {
        setLocked([]);
        setLockFailure(false);
        setDiagnostics(null);
      }
    } catch (e) {
      if (seq === generation.current) {
        setError(message(e));
        setInventory(null);
      }
    } finally {
      if (seq === generation.current) setLoading(false);
    }
  }
  useEffect(() => {
    void rescan();
    return () => {
      generation.current++;
    };
  }, []);
  useEffect(() => {
    if (page === "activity" && mode === "native") void history();
  }, [page, mode]);
  useEffect(() => {
    setOffset(0);
  }, [query, filter, sort, inventory]);
  const records = inventory?.threads ?? [];
  const byId = useMemo(
    () => new Map(records.map((r) => [r.id, r])),
    [inventory],
  );
  const lockSet = useMemo(() => new Set(locked), [locked]);
  const isLocked = (id: string) => conversationLocked(id, byId, lockSet);
  const matching = useMemo(
    () =>
      sortThreads(
        records.filter(
          (r) =>
            `${r.title} ${r.project} ${r.id}`
              .toLowerCase()
              .includes(query.toLowerCase()) &&
            (filter === "all" ||
              (filter === "locked" &&
                conversationLocked(r.id, byId, lockSet)) ||
              (filter === "manual" && lockSet.has(r.id.toLowerCase())) ||
              r.relation === filter),
        ),
        sort,
      ),
    [inventory, query, filter, sort, byId, lockSet],
  );
  const visible = matching.slice(offset, offset + 50);
  const pageIds = visible.filter(selectable).map((r) => r.id);
  const selectedRows = selected.flatMap((id) =>
    byId.has(id) ? [byId.get(id)!] : [],
  );
  const hasLockedSelection = selected.some(isLocked);
  const homeBytes =
    inventory?.totals.reduce((n, r) => n + r.logicalBytes, 0) ?? 0;
  const totalBytes = homeBytes + (diagnostics?.externalBytes ?? 0);
  const allocatedHome = inventory?.totals.every(
    (c) => c.allocatedBytes !== null,
  )
    ? inventory.totals.reduce((sum, c) => sum + c.allocatedBytes!, 0)
    : null;
  const ownSelectedBytes = selectedRows.reduce((n, r) => n + r.ownBytes, 0);
  function choose(id: string) {
    setSelected((current) =>
      current.includes(id)
        ? current.filter((k) => k !== id)
        : current.length >= 128
          ? (setOperationError(
              t("Select up to 128 conversations per cleanup."),
            ),
            current)
          : [...current, id],
    );
  }
  function choosePage() {
    if (pageIds.every((id) => selected.includes(id)))
      setSelected((s) => s.filter((id) => !pageIds.includes(id)));
    else {
      const next = Array.from(new Set([...selected, ...pageIds]));
      if (next.length > 128) {
        setOperationError(t("Select up to 128 conversations per cleanup."));
        return;
      }
      setSelected(next);
    }
  }
  async function changeLocks(ids: string[], value: boolean) {
    if (!ids.length || mode !== "native" || busy || lockFailure) return;
    setBusy(true);
    setOperationError("");
    try {
      setLocked(await setConversationLocks(ids, value));
      try {
        await readOperations();
      } catch {
        setOperationError(t("Locks saved. Operation history is unavailable."));
      }
    } catch (e) {
      setOperationError(message(e));
    } finally {
      setBusy(false);
    }
  }
  function clean(ids: string[]) {
    setOperationError("");
    if (ids.some(isLocked)) {
      setOperationError(t("Unlock protected conversations before cleanup."));
      return;
    }
    setCleanupIds(ids);
  }
  const date = (value: string | number) => {
    const d = new Date(value);
    return Number.isNaN(d.getTime())
      ? t("Unknown")
      : new Intl.DateTimeFormat(locale, {
          year: "numeric",
          month: "short",
          day: "numeric",
          hour: "2-digit",
          minute: "2-digit",
        }).format(d);
  };
  const currentDetail = detail ? byId.get(detail) : null;
  const event = eventDetail ? events.find((e) => e.id === eventDetail) : null;
  return (
    <div className="workspace">
      <header className="app-header">
        <div className="brand">
          <Broom size={26} />
          <strong>AgentBroom</strong>
        </div>
        <nav aria-label={t("Main navigation")}>
          {(
            [
              { id: "overview", label: "Overview" },
              { id: "conversations", label: "Conversations" },
              { id: "activity", label: "Execution log" },
            ] as const
          ).map((p) => (
            <button
              key={p.id}
              className={page === p.id ? "active" : ""}
              aria-current={page === p.id ? "page" : undefined}
              onClick={() => setPage(p.id)}
            >
              {t(p.label)}
            </button>
          ))}
        </nav>
        <div className="header-actions">
          <LanguageSelector />
          <ThemeToggle />
          <button
            className="icon-button"
            aria-label={t("Rescan Codex")}
            disabled={loading || busy || !!cleanupIds || reportsOpen}
            onClick={() => void rescan()}
          >
            <RefreshCw size={18} className={loading ? "spinning" : ""} />
          </button>
        </div>
      </header>
      <main className="content">
        {persistenceUnavailable && (
          <p role="status">{t("Language preference could not be saved.")}</p>
        )}
        <div className="mode-note">
          {t(
            mode === "native"
              ? "Local data · manual cleanup"
              : mode === "syntheticNativeTest"
                ? "Generated native test store"
                : "Browser inspection · read-only",
          )}
        </div>
        {loading && (
          <div className="loading" role="status">
            <RefreshCw className="spinning" size={20} />
            {t("Inspecting local storage…")}
          </div>
        )}
        {error && (
          <div className="empty-state" role="alert">
            <AlertCircle />
            <h2>{t("Local inspection unavailable")}</h2>
            <p>{error}</p>
            <button className="button primary" onClick={() => void rescan()}>
              {t("Retry inspection")}
            </button>
          </div>
        )}
        {!loading && inventory && (
          <>
            {operationError && (
              <p className="alert" role="alert">
                {operationError}
              </p>
            )}
            {mode === "readOnly" && (
              <p className="notice">
                {t(
                  "Browser inspection is read-only. Use the desktop app to lock or delete conversations.",
                )}
              </p>
            )}
            {page === "overview" && (
              <>
                <div className="page-heading">
                  <div>
                    <h1>{t("Overview")}</h1>
                    <p>{t("Codex storage, including crash reports.")}</p>
                  </div>
                  <span className="muted">
                    {scanAt && t("Updated {time}", { time: date(scanAt) })}
                  </span>
                </div>
                <section className="storage-hero">
                  <div>
                    <div className="eyebrow">{t("Codex file usage")}</div>
                    <div className="storage-total">{bytes(totalBytes)}</div>
                    {allocatedHome !== null && (
                      <p className="muted">
                        {t("Measured filesystem blocks")}:{" "}
                        {bytes(
                          allocatedHome +
                            (diagnostics?.externalAllocatedBytes ?? 0),
                        )}{" "}
                        ·{" "}
                        {t(
                          "Blocks may be shared; this is not a freed-space estimate.",
                        )}
                      </p>
                    )}
                    <p className="muted">
                      {t(
                        "File lengths are measured; they are not a reclaimed-space estimate.",
                      )}
                    </p>
                    {(!diagnostics || !diagnostics.complete) && (
                      <p className="notice">
                        {t(
                          "Crash report coverage is incomplete or unavailable; the total includes measured files only.",
                        )}
                      </p>
                    )}
                  </div>
                  <div className="overview-counts">
                    <div>
                      <strong>{records.length.toLocaleString(locale)}</strong>
                      <span>{t("Conversations")}</span>
                    </div>
                    <div>
                      <strong>
                        {lockFailure
                          ? t("Unknown")
                          : records.filter((r) => isLocked(r.id)).length}
                      </strong>
                      <span>{t("Locked")}</span>
                    </div>
                  </div>
                </section>
                <div
                  className="storage-bar"
                  aria-label={t("Storage breakdown")}
                >
                  {inventory.totals
                    .filter((c) => c.logicalBytes)
                    .map((c, i) => (
                      <span
                        key={c.category}
                        style={{
                          width: `${(c.logicalBytes / Math.max(totalBytes, 1)) * 100}%`,
                          background: `var(--chart-${i % 5})`,
                        }}
                        title={`${t(categories[c.category] ?? "Other local files")}: ${bytes(c.logicalBytes)}`}
                      />
                    ))}
                  {!!diagnostics?.externalBytes && (
                    <span
                      style={{
                        width: `${(diagnostics.externalBytes / totalBytes) * 100}%`,
                        background: "var(--crash)",
                      }}
                    />
                  )}
                </div>
                <div className="overview-grid">
                  <section className="panel">
                    <h2>{t("Where the bytes live")}</h2>
                    <div className="breakdown">
                      {inventory.totals.map((c, i) => (
                        <div className="breakdown-row" key={c.category}>
                          <span
                            className="category-dot"
                            style={{ background: `var(--chart-${i % 5})` }}
                          />
                          <span>
                            {t(categories[c.category] ?? "Other local files")}
                          </span>
                          <strong>{bytes(c.logicalBytes)}</strong>
                          <small>
                            {c.files} {t("files")}
                          </small>
                        </div>
                      ))}
                      <div className="breakdown-row">
                        <span className="category-dot crash" />
                        <span>{t("External crash reports")}</span>
                        <strong>
                          {diagnostics
                            ? bytes(diagnostics.externalBytes)
                            : t("Unknown")}
                        </strong>
                        <small>
                          {diagnostics?.files.filter((f) => f.external)
                            .length ?? "—"}{" "}
                          {t("files")}
                        </small>
                        <button
                          className="text-button"
                          disabled={mode !== "native" || !diagnostics}
                          onClick={() => setReportsOpen(true)}
                        >
                          {t("Inspect and clean")}
                        </button>
                      </div>
                    </div>
                    {diagnosticError && <p role="alert">{diagnosticError}</p>}
                    {diagnostics?.warnings.map((w) => (
                      <p className="muted" key={w}>
                        {systemText(w)}
                      </p>
                    ))}
                    <p className="tiny-note">
                      {t(
                        "External crash reports are added once; reports already inside Codex home are not counted twice.",
                      )}
                    </p>
                  </section>
                  <section className="panel">
                    <h2>{t("Manual locks")}</h2>
                    <p>{t("Conversations are unlocked by default.")}</p>
                    <p className="muted">
                      {t(
                        "Only manually locked conversations and their descendants are protected.",
                      )}
                    </p>
                    <p className="muted">
                      {t(
                        "Locks are saved on this device and survive rescans and restarts.",
                      )}
                    </p>
                    <button
                      className="button secondary"
                      onClick={() => {
                        setFilter("locked");
                        setPage("conversations");
                      }}
                    >
                      <LockKeyhole size={15} />
                      {t("View locked conversations")}
                    </button>
                    <button
                      className="text-button"
                      onClick={() => setPage("conversations")}
                    >
                      {t("View all conversations")} →
                    </button>
                  </section>
                </div>
                <details className="scan-notes">
                  <summary>
                    {t("Scope and measurement limits ({count})", {
                      count: inventory.warnings.length,
                    })}
                  </summary>
                  {inventory.warnings.map((w) => (
                    <p key={w}>{systemText(w)}</p>
                  ))}
                </details>
              </>
            )}
            {page === "conversations" && (
              <>
                <div className="page-heading">
                  <div className="heading-inline">
                    <h1>{t("Conversations")}</h1>
                    <span className="muted">
                      {t("All conversations · {count}", {
                        count: records.length,
                      })}
                    </span>
                  </div>
                </div>
                <div className="table-tools">
                  <label className="search">
                    <Search size={18} />
                    <input
                      aria-label={t("Search conversations")}
                      placeholder={t("Search conversations or projects")}
                      value={query}
                      onChange={(e) => setQuery(e.target.value)}
                    />
                  </label>
                  <div className="segmented">
                    <button
                      className={filter === "all" ? "active" : ""}
                      onClick={() => setFilter("all")}
                    >
                      {t("All")}
                    </button>
                    <button
                      className={filter === "locked" ? "active" : ""}
                      onClick={() => setFilter("locked")}
                    >
                      {t("Locked")}
                    </button>
                  </div>
                  <select
                    aria-label={t("Filter conversations")}
                    value={filter}
                    onChange={(e) => setFilter(e.target.value)}
                  >
                    <option value="all">{t("All relations")}</option>
                    <option value="locked">{t("Locked")}</option>
                    <option value="manual">{t("Manually locked")}</option>
                    {["root", "spawned", "fork", "unknown"].map((k) => (
                      <option value={k} key={k}>
                        {systemText(k)}
                      </option>
                    ))}
                  </select>
                  <label className="sort-control">
                    <ArrowUpDown size={15} />
                    <select
                      aria-label={t("Sort conversations")}
                      value={sort}
                      onChange={(e) => setSort(e.target.value as SortKey)}
                    >
                      <option value="updatedDesc">
                        {t("Latest activity first")}
                      </option>
                      <option value="updatedAsc">
                        {t("Oldest activity first")}
                      </option>
                      <option value="sizeDesc">{t("Largest first")}</option>
                      <option value="sizeAsc">{t("Smallest first")}</option>
                      <option value="ownDesc">
                        {t("Largest own files first")}
                      </option>
                      <option value="title">{t("Title A–Z")}</option>
                    </select>
                  </label>
                </div>
                <div className="table-scroll">
                  <table className="conversation-table">
                    <thead>
                      <tr>
                        <th className="check-cell">
                          <input
                            type="checkbox"
                            aria-label={t("Select this page")}
                            disabled={!pageIds.length || busy}
                            checked={
                              !!pageIds.length &&
                              pageIds.every((id) => selected.includes(id))
                            }
                            onChange={choosePage}
                          />
                        </th>
                        <th>{t("Conversation")}</th>
                        <th>
                          <button
                            onClick={() =>
                              setSort(
                                sort === "updatedDesc"
                                  ? "updatedAsc"
                                  : "updatedDesc",
                              )
                            }
                          >
                            {t("Last activity")} ↕
                          </button>
                        </th>
                        <th>
                          <button
                            onClick={() =>
                              setSort(
                                sort === "sizeDesc" ? "sizeAsc" : "sizeDesc",
                              )
                            }
                          >
                            {t("Size")} ↕
                          </button>
                        </th>
                        <th>{t("Lock")}</th>
                        <th>{t("Actions")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {visible.map((r) => (
                        <tr
                          key={r.id}
                          className={selected.includes(r.id) ? "selected" : ""}
                        >
                          <td>
                            <input
                              type="checkbox"
                              checked={selected.includes(r.id)}
                              aria-label={t("Select {title}", {
                                title: r.title,
                              })}
                              disabled={busy || !selectable(r)}
                              onChange={() => choose(r.id)}
                            />
                          </td>
                          <td>
                            <button
                              aria-label={t("Inspect {title}", {
                                title: r.title,
                              })}
                              className={`conversation-title ${r.relation === "spawned" ? "child" : ""}`}
                              onClick={() => setDetail(r.id)}
                            >
                              {r.relation === "spawned" ? (
                                <span>└</span>
                              ) : r.relation === "fork" ? (
                                <GitBranch size={16} />
                              ) : (
                                <FileText size={16} />
                              )}
                              <span>
                                <strong>{r.title}</strong>
                                <small>
                                  {systemText(r.relation)} ·{" "}
                                  {r.project
                                    .split(/[\\/]/)
                                    .filter(Boolean)
                                    .at(-1) || t("Unknown")}
                                </small>
                              </span>
                            </button>
                          </td>
                          <td className="date-cell">
                            {r.updatedAt ? date(r.updatedAt) : t("Unknown")}
                          </td>
                          <td className="numeric">
                            {r.files.length
                              ? bytes(r.inclusiveBytes)
                              : t("Unattributed")}
                          </td>
                          <td>
                            {lockFailure ? (
                              <span className="status warning">
                                {t("Lock state unknown")}
                              </span>
                            ) : isLocked(r.id) ? (
                              <span className="status protected">
                                <LockKeyhole size={15} />
                                {t("Locked")}
                              </span>
                            ) : (
                              <span className="status unlocked">
                                <LockOpen size={15} />
                                <span className="sr-only">{t("Unlocked")}</span>
                              </span>
                            )}
                          </td>
                          <td>
                            <div className="row-actions">
                              <button
                                className="button compact secondary"
                                disabled={
                                  mode !== "native" ||
                                  busy ||
                                  lockFailure ||
                                  (!selectable(r) &&
                                    !lockSet.has(r.id.toLowerCase()))
                                }
                                aria-label={t(
                                  lockSet.has(r.id.toLowerCase())
                                    ? "Unlock {title}"
                                    : "Lock {title}",
                                  { title: r.title },
                                )}
                                onClick={() =>
                                  void changeLocks(
                                    [r.id],
                                    !lockSet.has(r.id.toLowerCase()),
                                  )
                                }
                              >
                                {t(
                                  lockSet.has(r.id.toLowerCase())
                                    ? "Unlock"
                                    : "Lock",
                                )}
                              </button>
                              <button
                                className="button compact danger-soft"
                                disabled={
                                  mode !== "native" ||
                                  busy ||
                                  lockFailure ||
                                  isLocked(r.id) ||
                                  !selectable(r)
                                }
                                aria-label={t("Clean {title}", {
                                  title: r.title,
                                })}
                                onClick={() => clean([r.id])}
                              >
                                {t("Clean…")}
                              </button>
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {!visible.length && (
                    <div className="empty-state">
                      <Search />
                      <p>{t("No conversations match.")}</p>
                    </div>
                  )}
                </div>
                <div className="pagination">
                  <span>
                    {t("{count} matching conversations", {
                      count: matching.length,
                    })}
                  </span>
                  <div>
                    <button
                      className="icon-button"
                      aria-label={t("Previous page")}
                      disabled={offset === 0}
                      onClick={() => setOffset(Math.max(0, offset - 50))}
                    >
                      <ChevronLeft size={18} />
                    </button>
                    <span>
                      {Math.floor(offset / 50) + 1} /{" "}
                      {Math.max(1, Math.ceil(matching.length / 50))}
                    </span>
                    <button
                      className="icon-button"
                      aria-label={t("Next page")}
                      disabled={offset + 50 >= matching.length}
                      onClick={() => setOffset(offset + 50)}
                    >
                      <ChevronRight size={18} />
                    </button>
                  </div>
                </div>
                <div className="selection-bar">
                  <strong>
                    {t("{count} selected", { count: selected.length })}
                  </strong>
                  <span className="muted">
                    {bytes(ownSelectedBytes)} ·{" "}
                    {t("Own file lengths; preview includes spawned scope.")}
                  </span>
                  <div className="selection-actions">
                    <button
                      className="button secondary"
                      disabled={
                        !selected.length ||
                        busy ||
                        lockFailure ||
                        mode !== "native"
                      }
                      onClick={() => void changeLocks(selected, true)}
                    >
                      <LockKeyhole size={15} />
                      {t("Lock selected")}
                    </button>
                    <button
                      className="button secondary"
                      disabled={
                        !selected.length ||
                        busy ||
                        lockFailure ||
                        mode !== "native"
                      }
                      onClick={() => void changeLocks(selected, false)}
                    >
                      {t("Unlock selected")}
                    </button>
                    <button
                      className="button danger"
                      disabled={
                        !selected.length ||
                        busy ||
                        lockFailure ||
                        mode !== "native" ||
                        hasLockedSelection
                      }
                      onClick={() => clean(selected)}
                    >
                      <Trash2 size={15} />
                      {t("Clean selected ({count})", {
                        count: selected.length,
                      })}
                    </button>
                    {!!selected.length && (
                      <button
                        className="icon-button"
                        aria-label={t("Clear selection")}
                        onClick={() => setSelected([])}
                      >
                        <X size={16} />
                      </button>
                    )}
                  </div>
                  {hasLockedSelection && (
                    <p className="tiny-note">
                      {t("Unlock protected conversations before cleanup.")}
                    </p>
                  )}
                </div>
              </>
            )}
            {page === "activity" && (
              <>
                <div className="page-heading">
                  <div>
                    <h1>{t("Execution log")}</h1>
                    <p>{t("Cleanup results and operations on this device.")}</p>
                  </div>
                  <span className="retention">
                    <Clock3 size={15} />
                    {t("Keep 7 days")}
                  </span>
                </div>
                <p className="muted">
                  {t("Only the last 7 days of operation history are kept.")}
                </p>
                {historyError && <p role="alert">{historyError}</p>}
                {mode !== "native" && (
                  <p className="notice">
                    {t("Operation history is available in the desktop app.")}
                  </p>
                )}
                <div className="table-scroll">
                  <table>
                    <thead>
                      <tr>
                        <th>{t("Time")}</th>
                        <th>{t("Operation")}</th>
                        <th>{t("Objects")}</th>
                        <th>{t("Result")}</th>
                        <th>{t("Actions")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {events.map((e) => (
                        <tr key={e.id}>
                          <td>{date(e.at * 1000)}</td>
                          <td>{t(actions[e.action] ?? "Operation")}</td>
                          <td>
                            {e.titles[0] ||
                              byId.get(e.ids[0])?.title ||
                              e.ids[0] ||
                              t("Local Codex home")}{" "}
                            {e.count > 1 ? ` · ${e.count}` : ""}
                          </td>
                          <td>
                            <span
                              className={`status ${e.status === "verified" ? "completed" : e.status === "ready" ? "unlocked" : "warning"}`}
                            >
                              {t(statuses[e.status] ?? "Needs verification")}
                            </span>
                          </td>
                          <td>
                            <button
                              className="text-button"
                              onClick={() => setEventDetail(e.id)}
                            >
                              {t("View details")}
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {!events.length && !historyError && (
                    <div className="empty-state">
                      <Clock3 />
                      <p>{t("No operations in the last 7 days.")}</p>
                    </div>
                  )}
                </div>
              </>
            )}
          </>
        )}
      </main>
      {currentDetail && (
        <div className="modal-overlay">
          <section
            className="panel detail-panel"
            role="dialog"
            aria-modal="true"
            aria-label={t("Conversation details")}
          >
            <div className="dialog-heading">
              <h2>{currentDetail.title}</h2>
              <button
                className="icon-button"
                aria-label={t("Close details")}
                onClick={() => setDetail(null)}
              >
                <X />
              </button>
            </div>
            <p className="muted">{currentDetail.id}</p>
            <div className="detail-grid">
              <div>
                {t("Own files")}
                <strong>
                  {currentDetail.files.length
                    ? bytes(currentDetail.ownBytes)
                    : t("Unknown / unattributed")}
                </strong>
              </div>
              <div>
                {t("With spawned records")}
                <strong>
                  {currentDetail.files.length
                    ? bytes(currentDetail.inclusiveBytes)
                    : t("Unknown / unattributed")}
                </strong>
              </div>
              <div>
                {t("Last activity")}
                <strong>{date(currentDetail.updatedAt)}</strong>
              </div>
            </div>
            <h3>{t("Relationship evidence")}</h3>
            {currentDetail.parentId && (
              <p>
                {t("Parent")}{" "}
                <button
                  className="text-button"
                  disabled={!byId.has(currentDetail.parentId)}
                  onClick={() => setDetail(currentDetail.parentId)}
                >
                  {byId.get(currentDetail.parentId)?.title ??
                    currentDetail.parentId}
                </button>
              </p>
            )}
            {(currentDetail.forkSources ?? []).map((id) => (
              <p key={id}>
                {t("Fork lineage")}{" "}
                <button
                  className="text-button"
                  disabled={!byId.has(id)}
                  onClick={() => setDetail(id)}
                >
                  {byId.get(id)?.title ?? id}
                </button>
              </p>
            ))}
            {currentDetail.ownedDescendantIds.map((id) => (
              <p key={id}>
                <button className="text-button" onClick={() => setDetail(id)}>
                  {byId.get(id)?.title ?? id}
                </button>
              </p>
            ))}
            {(currentDetail.relationEvidence ?? []).map((e) => (
              <p className="muted" key={e}>
                {systemText(e)}
              </p>
            ))}
            <details>
              <summary>{t("Attributed rollout files")}</summary>
              {currentDetail.files.map((f) => (
                <p key={f} className="file-path">
                  {f}
                </p>
              ))}
            </details>
            <div className="row-actions">
              <button
                className="button secondary"
                disabled={mode !== "native" || busy || lockFailure}
                onClick={() =>
                  void changeLocks(
                    [currentDetail.id],
                    !lockSet.has(currentDetail.id.toLowerCase()),
                  )
                }
              >
                {t(
                  lockSet.has(currentDetail.id.toLowerCase())
                    ? "Unlock conversation"
                    : "Lock conversation",
                )}
              </button>
              <button
                className="button danger"
                disabled={
                  mode !== "native" ||
                  busy ||
                  lockFailure ||
                  isLocked(currentDetail.id) ||
                  !selectable(currentDetail)
                }
                onClick={() => {
                  setDetail(null);
                  clean([currentDetail.id]);
                }}
              >
                {t("Clean…")}
              </button>
            </div>
            <p className="tiny-note">
              {t(
                "Locks are saved on this device and survive rescans and restarts.",
              )}
            </p>
          </section>
        </div>
      )}
      {cleanupIds && inventory && (
        <div className="modal-overlay">
          <section
            className="preview-shell"
            role="dialog"
            aria-modal="true"
            aria-label={t("Cleanup preview")}
          >
            <NativeCleanupPanel
              key={cleanupIds.join(",") + locked.join(",")}
              inventory={inventory}
              kept={locked}
              initialSelected={cleanupIds}
              testOnly={mode === "syntheticNativeTest"}
              onClose={() => setCleanupIds(null)}
              refresh={() => void rescan()}
            />
          </section>
        </div>
      )}
      {reportsOpen && diagnostics && (
        <div className="modal-overlay">
          <div
            className="preview-shell"
            role="dialog"
            aria-modal="true"
            aria-label={t("Crash reports")}
          >
            <DiagnosticPanel
              inventory={diagnostics}
              close={() => setReportsOpen(false)}
              refresh={() => void rescan()}
            />
          </div>
        </div>
      )}
      {event && (
        <div className="modal-overlay">
          <section
            className="panel detail-panel"
            role="dialog"
            aria-modal="true"
            aria-label={t("Operation details")}
          >
            <div className="dialog-heading">
              <h2>{t(actions[event.action] ?? "Operation")}</h2>
              <button
                className="icon-button"
                aria-label={t("Close details")}
                onClick={() => setEventDetail(null)}
              >
                <X />
              </button>
            </div>
            <p>
              {date(event.at * 1000)} ·{" "}
              {t(statuses[event.status] ?? "Needs verification")}
            </p>
            {event.ids.map((id, i) => (
              <p className="file-path" key={id}>
                {event.titles[i] ?? byId.get(id)?.title ?? id}
              </p>
            ))}
            {!!event.bytes && (
              <p>
                {t(
                  event.action === "preview"
                    ? "Measured file lengths"
                    : "Verified removed file lengths",
                )}
                : {bytes(event.bytes)}
              </p>
            )}
            {event.error && <p role="alert">{systemText(event.error)}</p>}
          </section>
        </div>
      )}
    </div>
  );
}
