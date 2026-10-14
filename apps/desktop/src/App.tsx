import { useEffect, useRef, useState } from "react";
import {LocaleProvider, LanguageSelector, useLocale, type MessageKey, type MessageValues} from "./i18n";
import * as Dialog from "@radix-ui/react-dialog";
import {
  ArrowRight,
  Archive,
  Broom,
  Check,
  ChevronRight,
  CircleCheck,
  Clock3,
  FileText,
  Folder,
  GitBranch,
  Layers3,
  LayoutDashboard,
  LockKeyhole,
  RefreshCw,
  Search,
  ShieldCheck,
  Sparkles,
  X,
} from "lucide-react";
import {
  loadInventory,
  getPreview,
  getReport,
  errorText,
  sourceLabel,
  nativeRuntime,
} from "./bridge";
import {
  bytes,
  type Inventory,
  type Thread,
  type Preview,
  type ActivityEntry,
} from "./types";
type Page = "overview" | "conversations" | "activity";
const categoryNames: Record<string, MessageKey> = {
  sessions: "Session files",
  archives: "Archived sessions",
  history: "History database",
  logs: "Logs & WAL",
  assets: "Generated assets",
  worktrees: "Worktrees", cache: "Tools & cache", other: "Other local files",
};
const categoryColors: Record<string, string> = {
  sessions: "#d49a35",
  archives: "#78977d",
  history: "#8999ba",
  logs: "#b5a1b2",
  assets: "#b7b3a5",
  worktrees: "#7499a6", cache: "#9a8b70", other: "#787878",
};
function Modal({
  open,
  onOpenChange,
  title,
  description,
  children,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  const {t} = useLocale();
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content className="modal">
          <Dialog.Title>{title}</Dialog.Title>
          <Dialog.Description>{description}</Dialog.Description>
          <div className="modal-language"><LanguageSelector/></div>
          <Dialog.Close
            className="icon-button modal-close"
            aria-label={t("Close dialog")}
          >
            <X size={19} />
          </Dialog.Close>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export default function App() { return <LocaleProvider><AppContent/></LocaleProvider>; }
function AppContent() {
  const {locale, persistenceUnavailable, t, systemText} = useLocale();
  const displayBytes = (n: number | null) => systemText(bytes(n));
  const [inventory, setInventory] = useState<Inventory | null>(null),
    [page, setPage] = useState<Page>("overview"),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(true);
  const [search, setSearch] = useState(""),
    [filter, setFilter] = useState("all"),
    [detailId, setDetailId] = useState<string | null>(null),
    [selected, setSelected] = useState<string[]>([]),
    [kept, setKept] = useState<string[]>([]);
  const [preview, setPreview] = useState<Preview | null>(null),
    [report, setReport] = useState<string | null>(null),
    [busy, setBusy] = useState(false),
    [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [source, setSource] = useState<"demo" | "codex">("demo");
  const [visibleLimit, setVisibleLimit] = useState(50);
  const isReal = inventory?.sourceKind === "codex";
  const scanSeq = useRef(0),
    reviewSeq = useRef(0),
    entryId = useRef(0);
  const addActivity = (title: MessageKey, description: MessageKey, values: MessageValues = {}) =>
    setActivity((a) =>
      [
        {
          id: ++entryId.current,
          title,
          description,
          values,
          time: new Date().toISOString(),
        },
        ...a,
      ].slice(0, 20),
    );
  const invalidate = () => {
    reviewSeq.current++;
    setPreview(null);
    setReport(null);
    setBusy(false);
  };
  async function rescan(nextSource: "demo" | "codex" = source) {
    setSource(nextSource);
    const seq = ++scanSeq.current;
    invalidate();
    setLoading(true);
    setError("");
    try {
      const next = await loadInventory(nextSource);
      if (seq !== scanSeq.current) return;
      setInventory(next);
      setSelected([]);
      setKept([]);
      setDetailId(null);
      addActivity(
        next.sourceKind === "codex" ? "Local Codex inspected read-only" : "Demo inventory inspected",
        next.sourceKind === "codex" ? "{count} files measured; no source files written, no database opened." : "{count} bundled synthetic files. No real agent data accessed.",
        {count: next.items.length},
      );
    } catch (e) {
      if (seq === scanSeq.current) {
        setError(errorText(e));
        setInventory(null);
      }
    } finally {
      if (seq === scanSeq.current) setLoading(false);
    }
  }
  useEffect(() => {
    void rescan();
    return () => {
      scanSeq.current++;
      reviewSeq.current++;
    };
  }, []);
  const roots =
    inventory?.threads.filter((record) => record.relation !== "spawned") ?? [];
  const total = inventory?.totals.reduce((n, c) => n + c.logicalBytes, 0) ?? 0;
  const protectedCount = (isReal ? inventory!.threads : roots).filter(
    (record) => record.protectionReasons.length > 0 || kept.includes(record.id),
  ).length;
  const reviewable = roots.filter(
    (record) => !record.protectionReasons.length && !kept.includes(record.id),
  );
  const records = isReal ? (inventory?.threads ?? []) : roots;
  const allocatedTotal = inventory?.totals.every((record) => record.allocatedBytes !== null) ? inventory.totals.reduce((n,t) => n + t.allocatedBytes!, 0) : null;
  useEffect(() => {setVisibleLimit(50);}, [search, filter, inventory]);
  const sorted = [...records].sort(
    (a, b) => b.inclusiveBytes - a.inclusiveBytes || a.id.localeCompare(b.id),
  );
  const matches = sorted.filter(
    (record) =>
      `${record.title} ${record.project} ${record.id}`
        .toLowerCase()
        .includes(search.toLowerCase()) &&
      (filter === "all" ||
        (filter === "protected" &&
          (record.protectionReasons.length > 0 || kept.includes(record.id))) ||
        (filter === "reviewable" &&
          !record.protectionReasons.length &&
          !kept.includes(record.id)) ||
        (filter === "kept" && kept.includes(record.id)) ||
        (["root", "spawned", "fork", "unknown"].includes(filter) && record.relation === filter)),
  );
  const shown = matches.slice(0, visibleLimit);
  const detail = inventory?.threads.find((record) => record.id === detailId);
  function toggleSelection(id: string) {
    invalidate();
    setSelected((s) =>
      s.includes(id) ? s.filter((x) => x !== id) : [...s, id],
    );
  }
  function toggleKeep(t: Thread) {
    invalidate();
    setKept((k) =>
      k.includes(t.id) ? k.filter((x) => x !== t.id) : [...k, t.id],
    );
    setSelected((s) => s.filter((x) => x !== t.id));
    addActivity(
      kept.includes(t.id) ? "Keep choice removed" : "Keep choice added",
      "{title} · choice exists only for this session.", {title:t.title},
    );
  }
  async function review(asReport = false) {
    if (!inventory || inventory.sourceKind !== "demo") return;
    const seq = ++reviewSeq.current;
    setBusy(true);
    setError("");
    try {
      if (asReport) {
        const text = await getReport(
          selected,
          kept,
          inventory.sourceFingerprint,
        );
        if (seq !== reviewSeq.current) return;
        setReport(text);
        addActivity(
          "Read-only report prepared",
          "A sample report, not a backup or deletion receipt.",
        );
      } else {
        const p = await getPreview(selected, kept, inventory.sourceFingerprint);
        if (seq !== reviewSeq.current) return;
        setPreview(p);
        addActivity(
          "Selection previewed",
          "{count} synthetic records · execution is disabled.", {count:p.affectedIds.length},
        );
      }
    } catch (e) {
      if (seq === reviewSeq.current) setError(errorText(e));
    } finally {
      if (seq === reviewSeq.current) setBusy(false);
    }
  }
  function openDetail(id: string) {
    setDetailId(id);
    setPage("conversations");
  }
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-icon">
            <Broom size={24} />
          </span>
          <span>
            AgentBroom<small>{t("STORAGE, CLEARLY.")}</small>
          </span>
        </div>
        <div className="nav-label">{t("WORKSPACE")}</div>
        <nav aria-label={t("Main navigation")}>
          {(
            [
              { id: "overview", label: "Overview", Icon: LayoutDashboard },
              { id: "conversations", label: "Conversations", Icon: Layers3 },
              { id: "activity", label: "Activity", Icon: Clock3 },
            ] as const
          ).map(({ id, label, Icon }) => (
            <button
              key={id}
              className={`nav-item ${page === id ? "active" : ""}`}
              aria-current={page === id ? "page" : undefined}
              onClick={() => setPage(id)}
            >
              <Icon size={18} />
              {t(label)}
              {id === "conversations" && inventory && (
                <span className="nav-count">{records.length}</span>
              )}
            </button>
          ))}
        </nav>
        <div className="sidebar-divider" />
        <div className="nav-label">{t("SOURCE")}</div>
        <div className="source-tile">
          <span className="source-dot" />
          <div>
            {source === "codex" ? t("Local Codex home") : t("Codex sample store")}<small>{source === "codex" ? t("Metadata inspection only") : t("Bundled synthetic fixtures")}</small>
          </div>
          <LockKeyhole size={14} />
        </div>
        <button className="button secondary source-switch" disabled={loading || !nativeRuntime} onClick={() => void rescan(source === "demo" ? "codex" : "demo")}>
          {source === "demo" ? t("Inspect local Codex") : t("Switch to demo")}
        </button>
        {!nativeRuntime && <p className="tiny-note source-switch">{t("Local inspection requires the desktop app.")}</p>}
        <div className="sidebar-bottom">
          <div className="safety-tile">
            <ShieldCheck size={22} />
            <strong>{t("Inspection comes first.")}</strong>
            <p>{source === "codex" ? t("Metadata stays in this window. No cleanup, export or database connections.") : t("Demo mode inspects bundled synthetic data only.")}</p>
          </div>
          <div className="version">
            <span>{t("v0.1.0 · prototype")}</span>
            <span className="green-dot" />
          </div>
        </div>
      </aside>
      <main className="main">
        <header className="topbar">
          <div className="breadcrumb">{t("Workspace")}<ChevronRight size={13} />{" "}
            <strong>
              {page === "overview"
                ? t("Overview")
                : page === "conversations"
                  ? t("Conversations")
                  : t("Activity")}
            </strong>
          </div>
          <div className="topbar-controls">
            <LanguageSelector/>
          <span className="readonly-badge">
            <ShieldCheck size={13} /> {source === "codex" ? t("Local Codex · read-only") : t("Demo dataset · read-only")}
          </span>
          </div>
        </header>
        <div className="content">
          <div className="page-heading">
            <div>
              <div className="eyebrow">{t("LOCAL STORAGE INSPECTION")}</div>
              <h1>
                {page === "overview"
                  ? (isReal ? t("See your local Codex files.") : t("Know what’s taking up space."))
                  : page === "conversations"
                    ? t("A clearer view of your history.")
                    : t("Inspection, with a paper trail.")}
              </h1>
              <p>
                {page === "overview"
                  ? (isReal ? t("Read-only inventory. Every record stays protected; no cleanup or export.") : t("Understand the footprint. Keep the context. Make an informed decision."))
                  : page === "conversations"
                    ? t("Segments, spawned agents and independent forks — kept distinct.")
                    : t("This session’s inspection events. These are not backups or deletion receipts.")}
              </p>
            </div>
            <button
              className="button secondary"
              onClick={() => void rescan()}
              disabled={loading}
            >
              <RefreshCw size={15} className={loading ? "spin" : ""} />
              {loading ? t("Inspecting…") : source === "codex" ? t("Rescan Codex") : t("Rescan demo")}
            </button>
          </div>
          <div className="demo-banner">
            <span className="banner-symbol">
              <Sparkles size={17} />
            </span>
            <div>
              <strong>{source === "codex" ? t("Your local Codex metadata. Inspection only.") : t("You’re exploring a sample dataset.")}</strong>
              <span>
                {" "}
                {source === "codex" ? t("Databases are measured without opening them. Live status is unknown; every record remains protected.") : t("Only bundled synthetic files are inspected. No cleanup is available.")}
              </span>
            </div>
            <span className="demo-source">{source === "codex" ? t("Native Codex scanner") : systemText(sourceLabel)}</span>
          </div>
          {persistenceUnavailable && <div role="status" className="error-banner">{t("Language changed for this window; preferences could not be saved.")}</div>}
          {error && (
            <div role="alert" className="error-banner">
              {systemText(error)}
            </div>
          )}
          {loading ? (
            <div className="empty-state">
              <RefreshCw className="spin" />
              {source === "codex" ? t("Inspecting local file metadata…") : t("Loading the bundled demo inventory…")}
            </div>
          ) : !inventory ? (
            <div className="empty-state">
              <ShieldCheck />
              {source === "codex" ? t("Local inspection unavailable. No data was changed.") : t("Demo unavailable. No fallback to a real agent store.")}
              <button
                className="button secondary"
                onClick={() => void rescan()}
              >{t("Retry inspection")}</button>
            </div>
          ) : (
            <>
              {isReal && <div className="scope-banner" role="note">
                <strong>{inventory.partialCoverage ? t("Partial inventory · measured home files") : t("Measured home files only")}</strong>
                <p>{t("Only the configured Codex home is measured. External sqlite_home, symlink targets, directory metadata and inaccessible entries are excluded. Allocated blocks are not reclaimable space; shared hardlink blocks are credited to the first lexical path.")}</p>
              </div>}
              {isReal && <details className="scan-notes"><summary>{t("Scope and measurement limits ({count})",{count:inventory.warnings.length})}</summary>{inventory.warnings.map(w => <p key={w}>{systemText(w)}</p>)}</details>}
              {page === "overview" && (
                <>
                  <section className="metrics" aria-label={t("Inventory summary")}>
                    <Metric
                      label={isReal ? t("Measured file lengths") : t("Sample storage")}
                      value={displayBytes(total)}
                      detail={t(isReal ? "{count} local files measured" : "{count} fixture files measured",{count:inventory.items.length})}
                      Icon={Folder}
                    />
                    <Metric
                      label={isReal ? t("Main & fork conversations") : t("Conversations")}
                      value={String(isReal ? inventory.threads.filter(record => record.relation === "root" || record.relation === "fork").length : roots.length)}
                      detail={isReal ? t("{spawned} confirmed sub Agents · {unknown} unresolved",{spawned:inventory.threads.filter(record => record.relation === "spawned").length,unknown:inventory.threads.filter(record => record.relation === "unknown").length}) : t("{count} spawned records, counted separately",{count:inventory.threads.filter(record => record.relation === "spawned").length})}
                      Icon={Layers3}
                    />
                    <Metric
                      label={t("Protected")}
                      value={String(protectedCount)}
                      detail={isReal ? t("All real records remain protected") : t("Active, referenced or incomplete")}
                      Icon={ShieldCheck}
                    />
                    <Metric
                      label={isReal ? t("Allocated blocks") : t("Reviewable")}
                      value={isReal ? displayBytes(allocatedTotal) : String(reviewable.length)}
                      detail={isReal ? t("Filesystem accounting; not reclaimable space") : t("Metadata checks passed in the demo")}
                      Icon={CircleCheck}
                    />
                  </section>
                  <div className="overview-grid">
                    <section className="panel storage-panel">
                      <div className="panel-heading">
                        <div>
                          <h2>{t("Where the bytes live")}</h2>
                          <p>
                            {isReal ? t("File lengths and allocated blocks, grouped by category.") : t("Measured sample file sizes, grouped by category.")}
                          </p>
                        </div>
                        <span className="quiet-chip">{t("Logical size")}</span>
                      </div>
                      <div className="storage-bar">
                        {inventory.totals.map((c) => (
                          <span
                            key={c.category}
                            style={{
                              width: `${(c.logicalBytes / (total || 1)) * 100}%`,
                              background: categoryColors[c.category],
                            }}
                            title={`${t(categoryNames[c.category])}: ${displayBytes(c.logicalBytes)}`}
                          />
                        ))}
                      </div>
                      <div className="category-list">
                        {inventory.totals.map((c) => (
                          <div key={c.category}>
                            <span
                              className="category-dot"
                              style={{ background: categoryColors[c.category] }}
                            />
                            <span>
                              {t(categoryNames[c.category])}
                              <small>{t("{count} files",{count:c.files})}</small>
                            </span>
                            <strong>{displayBytes(c.logicalBytes)}{isReal && <small>{t("{size} allocated",{size:displayBytes(c.allocatedBytes)})}</small>}</strong>
                          </div>
                        ))}
                      </div>
                      <div className="panel-footnote">
                        {isReal ? t("Databases, WAL and SHM are stat-only. Shared database bytes are not attributed to individual conversations.") : t("Database and WAL placeholders are measured only. They are never opened as databases.")}
                      </div>
                    </section>
                    <section className="panel protection-panel">
                      <div className="panel-heading">
                        <div>
                          <h2>{t("What stays protected")}</h2>
                          <p>{t("Uncertainty is a reason to stop.")}</p>
                        </div>
                        <ShieldCheck size={23} />
                      </div>
                      <div className="protection-item">
                        <GitBranch size={19} />
                        <div>
                          <strong>{t("Shared history references")}</strong>
                          <p>{t("Independent forks do not become owned children.")}</p>
                        </div>
                      </div>
                      <div className="protection-item">
                        <Clock3 size={19} />
                        <div>
                          <strong>{isReal ? t("Unverified live status") : t("Active conversations")}</strong>
                          <p>{isReal ? t("Pinned, referenced and incomplete records carry explicit reasons. Runtime activity is not queried.") : t("A completed reply is not task acceptance.")}</p>
                        </div>
                      </div>
                      <div className="protection-item">
                        <LockKeyhole size={19} />
                        <div>
                          <strong>{t("Incomplete metadata")}</strong>
                          <p>{t("Oversized and unread records stay unresolved.")}</p>
                        </div>
                      </div>
                      <button
                        className="text-button"
                        onClick={() => {
                          setFilter("protected");
                          setPage("conversations");
                        }}
                      >{t("Inspect protected records")}<ArrowRight size={15} />
                      </button>
                    </section>
                  </div>
                  <section className="panel">
                    <div className="panel-heading">
                      <div>
                        <h2>{t("Largest conversations")}</h2>
                        <p>{t("Includes proved spawned records. Shared database overhead stays separate.")}</p>
                      </div>
                      <button
                        className="text-button"
                        onClick={() => setPage("conversations")}
                      >{t("View all")}<ArrowRight size={14} />
                      </button>
                    </div>
                    <div className="largest-list">
                      {sorted.slice(0, 4).map((record) => (
                        <button key={record.id} onClick={() => openDetail(record.id)}>
                          <span className="conversation-icon">
                            <FileText size={18} />
                          </span>
                          <span className="largest-title">
                            {record.title}
                            <small>
                              {record.project} · {t("{count} segments", {count:record.files.length})}
                            </small>
                          </span>
                          <Status thread={record} kept={kept.includes(record.id)} real={isReal} />
                          <strong>{isReal && !record.files.length ? t("Unattributed") : displayBytes(record.inclusiveBytes)}</strong>
                          <ChevronRight size={16} />
                        </button>
                      ))}
                    </div>
                  </section>
                  <div className="bottom-note">
                    <ShieldCheck size={15} />
                    {isReal ? t("Only local metadata is displayed. All real records are protected; no cleanup or export is available.") : t("No disk-free-space query, real agent discovery or deletion was performed.")}
                    {!isReal && <button
                      className="text-button"
                      onClick={() => void review(true)}
                    >{t("View sample report")}<FileText size={14} />
                    </button>}
                  </div>
                </>
              )}
              {page === "conversations" && (
                <>
                  <div className="toolbar">
                    <label className="search">
                      <Search size={16} />
                      <input
                        aria-label={t("Search conversations")}
                        placeholder={t("Search title, project or identity…")}
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                      />
                    </label>
                    <select
                      aria-label={t("Filter conversations")}
                      value={filter}
                      onChange={(e) => setFilter(e.target.value)}
                    >
                      <option value="all">{t("All conversations")}</option>
                      {!isReal && <option value="reviewable">{t("Reviewable")}</option>}
                      {isReal && <><option value="root">{t("Main conversations")}</option><option value="spawned">{t("Sub Agents")}</option><option value="fork">{t("Independent forks")}</option><option value="unknown">{t("Unresolved")}</option></>}
                      <option value="protected">{t("Protected")}</option>
                      <option value="kept">{t("Kept this session")}</option>
                    </select>
                    <span className="results-count">
                      {t("{count} records · showing {shown}",{count:matches.length,shown:shown.length})}
                    </span>
                  </div>
                  <div
                    className={`conversation-layout ${detail ? "with-detail" : ""}`}
                  >
                    <section className="panel conversation-table">
                      <div className="table-head">
                        <span />
                        <span>{t("CONVERSATION")}</span>
                        <span>{t("STATUS")}</span>
                        <span>{t("FOOTPRINT")}</span>
                      </div>
                      {shown.length ? (
                        shown.map((record) => (
                          <div
                            key={record.id}
                            className={`table-row ${detailId === record.id ? "focused" : ""}`}
                          >
                            <input
                              type="checkbox"
                              aria-label={t("Select {title}",{title:record.title})}
                              checked={selected.includes(record.id)}
                              disabled={
                                record.protectionReasons.length > 0 ||
                                kept.includes(record.id)
                              }
                              onChange={() => toggleSelection(record.id)}
                            />
                            <button
                              className="row-title"
                              onClick={() => setDetailId(record.id)}
                            >
                              <strong>{record.title}</strong>
                              <span>
                                {isReal ? `${systemText(record.relation)} · ${systemText(record.sourceType || t("unknown"))} · ` : ""}{record.project} · {t("{count} segments",{count:record.files.length})}
                                {record.ownedDescendantIds.length ? ` · ${t("{count} spawned", {count:record.ownedDescendantIds.length})}` : ""}
                              </span>
                            </button>
                            <Status thread={record} kept={kept.includes(record.id)} real={isReal} />
                            <strong className="row-size">
                              {isReal && !record.files.length ? t("Unattributed") : displayBytes(record.inclusiveBytes)}
                            </strong>
                          </div>
                        ))
                      ) : (
                        <div className="empty-state">{t("No conversations match this view.")}</div>
                      )}
                      {matches.length > shown.length && <button className="button secondary" onClick={() => setVisibleLimit(v => v + 50)}>{t("Show 50 more")}</button>}
                      <div className="table-footer">
                        <span>
                          {t("{count} selected · inspection only",{count:selected.length})}
                        </span>
                        <button
                          className="button primary"
                          disabled={!selected.length || busy}
                          onClick={() => void review()}
                        >
                          {busy ? t("Preparing…") : t("Review selection")}
                          <ArrowRight size={14} />
                        </button>
                      </div>
                    </section>
                    {detail && (
                      <aside className="panel detail-panel">
                        <div className="detail-top">
                          <span className="eyebrow">{t("CONVERSATION DETAILS")}</span>
                          <button
                            className="icon-button"
                            aria-label={t("Close details")}
                            onClick={() => setDetailId(null)}
                          >
                            <X size={16} />
                          </button>
                        </div>
                        <h2>{detail.title}</h2>
                        <Status
                          thread={detail}
                          kept={kept.includes(detail.id)}
                          real={isReal}
                        />
                        <div className="detail-facts">
                          <span>{t("Project")}<strong>{detail.project}</strong>
                          </span>
                          <span>{t("Own files")}<strong>{isReal && !detail.files.length ? t("Unknown / unattributed") : displayBytes(detail.ownBytes)}</strong>
                          </span>
                          <span>{t("With spawned records")}<strong>{isReal && !detail.files.length ? t("Unknown / unattributed") : displayBytes(detail.inclusiveBytes)}</strong>
                          </span>
                          <span>{t("Relation")}<strong>{systemText(detail.relation)}</strong>
                          </span>
                          <span>
                            {isReal ? t("Index / header timestamp") : t("Recorded activity")}
                            <strong>
                              {detail.updatedAt
                                ? detail.updatedAt.slice(0, 10)
                                : t("Unknown")}
                            </strong>
                          </span>
                        </div>
                        <div className="id-box">{detail.id}</div>
                        {detail.protectionReasons.length > 0 && (
                          <div className="protection-reasons">
                            <ShieldCheck size={17} />
                            <div>
                              {detail.protectionReasons.map((r) => (
                                <p key={r}>{systemText(r)}</p>
                              ))}
                            </div>
                          </div>
                        )}
                        {isReal && <>
                          <h3>{t("Relationship evidence")}</h3>
                          {(detail.relationEvidence ?? []).map(e => <p className="tiny-note" key={e}>{systemText(e)}</p>)}
                          {detail.parentId && <p>{t("Parent")}<button className="text-button" disabled={!inventory.threads.some(record => record.id === detail.parentId)} onClick={() => setDetailId(detail.parentId)}>{detail.parentId}</button></p>}
                          {(detail.forkSources ?? []).map(id => <p key={id}>{t("Fork lineage")}<button className="text-button" disabled={!inventory.threads.some(record => record.id === id)} onClick={() => setDetailId(id)}>{id}</button></p>)}
                          {(detail.historySources ?? []).map(id => <p key={id}>{t("Shared history")}<button className="text-button" disabled={!inventory.threads.some(record => record.id === id)} onClick={() => setDetailId(id)}>{id}</button></p>)}
                        </>}
                        <h3>{isReal ? t("Attributed rollout files") : t("Owned sample files")}</h3>
                        <ul className="file-list">
                          {detail.files.map((f) => (
                            <li key={f}>
                              <FileText size={13} />
                              {f}
                            </li>
                          ))}
                        </ul>
                        {detail.ownedDescendantIds.length > 0 && (
                          <>
                            <h3>{t("Spawned records")}</h3>
                            <ul className="file-list">
                              {detail.ownedDescendantIds.map((id) => (
                                <li key={id}>
                                  <Layers3 size={13} />
                                  <button className="text-button" onClick={() => setDetailId(id)}>{id}</button>
                                </li>
                              ))}
                            </ul>
                          </>
                        )}
                        {!isReal && detail.sharedHistoryFrom && (
                          <div className="fork-note">
                            <GitBranch size={15} />
                            {t("Independent fork referencing {id}. The source is not owned by this record.", {id:detail.sharedHistoryFrom})}
                          </div>
                        )}
                        <button
                          className={`button ${kept.includes(detail.id) ? "kept-button" : "secondary"}`}
                          onClick={() => toggleKeep(detail)}
                        >
                          <ShieldCheck size={15} />
                          {kept.includes(detail.id)
                            ? t("Remove keep choice")
                            : t("Keep this conversation")}
                        </button>
                        <p className="tiny-note">{t("Keep choices exist in memory for this session only. They do not change an agent’s retention rules.")}</p>
                      </aside>
                    )}
                  </div>
                  <div className="bottom-note">
                    <LockKeyhole size={15} />
                    {isReal ? t("Sizes include attributed rollout files only. Shared databases, fork histories and unresolved children are not added to a conversation’s footprint.") : t("Reviewable means demo metadata checks passed. It does not mean safe to delete.")}
                  </div>
                </>
              )}
              {page === "activity" && (
                <section className="panel activity-panel">
                  <div className="panel-heading">
                    <div>
                      <h2>{t("This session")}</h2>
                      <p>{t("Inspection events only. No records have been deleted.")}</p>
                    </div>
                    <span className="quiet-chip">{t("In-memory")}</span>
                  </div>
                  {activity.length ? (
                    activity.map((a) => (
                      <div className="activity-row" key={a.id}>
                        <span className="activity-icon">
                          <Check size={16} />
                        </span>
                        <div>
                          <strong>{t(a.title)}</strong>
                          <p>{t(a.description,a.values)}</p>
                        </div>
                        <time>{new Intl.DateTimeFormat(locale,{hour:"2-digit",minute:"2-digit"}).format(new Date(a.time))}</time>
                      </div>
                    ))
                  ) : (
                    <div className="empty-state">{t("Inspection events will appear here.")}</div>
                  )}
                  <div className="panel-footnote">{t("This activity log is not a backup and cannot restore anything. It clears when the app reloads.")}</div>
                </section>
              )}
            </>
          )}
        </div>
      </main>
      <Modal
        open={preview !== null}
        onOpenChange={(v) => {
          if (!v) setPreview(null);
        }}
        title={t("Review the sample selection")}
        description={t("An exact, read-only preview. There is no executable cleanup action.")}
      >
        {preview && (
          <>
            <div className="preview-summary">
              <span>{t("{count} selected conversations",{count:preview.selectedIds.length})}</span>
              <strong>{displayBytes(preview.logicalBytes)}</strong>
              <small>{t("sample file bytes · not freed space")}</small>
            </div>
            <div className="preview-rows">
              {preview.rows.map((record) => (
                <div key={record.id}>
                  <FileText size={16} />
                  <span>
                    {record.title}
                    <small>
                      {record.id} · {systemText(record.relation)}
                    </small>
                  </span>
                  <strong>{displayBytes(record.ownBytes)}</strong>
                </div>
              ))}
            </div>
            <div className="locked-callout">
              <LockKeyhole size={18} />
              <div>
                <strong>{t("Execution is disabled.")}</strong>
                <p>{t("No deletion, database changes or backup operations exist in this prototype.")}</p>
              </div>
            </div>
            <button
              className="button secondary"
              onClick={() => {
                setPreview(null);
                void review(true);
              }}
            >
              <FileText size={15} />{t("View JSON report")}</button>
          </>
        )}
      </Modal>
      <Modal
        open={report !== null}
        onOpenChange={(v) => {
          if (!v) setReport(null);
        }}
        title={t("Read-only sample report")}
        description={t("Sanitized fixture metadata. This report is not a backup or a deletion receipt.")}
      >
        <textarea
          className="report-text"
          aria-label={t("Sample JSON report")}
          readOnly
          value={report ?? ""}
        />
        <p className="tiny-note">{t("All identities and paths belong to bundled synthetic fixtures. No real history is included.")}</p>
      </Modal>
    </div>
  );
}
function Metric({
  label,
  value,
  detail,
  Icon,
}: {
  label: string;
  value: string;
  detail: string;
  Icon: typeof Folder;
}) {
  return (
    <div className="metric">
      <div className="metric-label">
        {label}
        <Icon size={17} />
      </div>
      <strong>{value}</strong>
      <span>{detail}</span>
    </div>
  );
}
function Status({ thread, kept, real = false }: { thread: Thread; kept: boolean; real?: boolean }) {
  const {t} = useLocale();
  if (real) return <span className="status protected"><ShieldCheck size={12}/>{t("Protected")}{kept ? t(" · kept this session") : ""}</span>;
  return (
    <span
      className={`status ${kept ? "kept" : thread.protectionReasons.length ? "protected" : "reviewable"}`}
    >
      {kept ? (
        <ShieldCheck size={11} />
      ) : thread.protectionReasons.length ? (
        <LockKeyhole size={11} />
      ) : thread.activity === "archived" ? (
        <Archive size={11} />
      ) : (
        <CircleCheck size={11} />
      )}{" "}
      {kept
        ? t("Kept")
        : thread.protectionReasons.length
          ? t("Protected")
          : thread.activity === "archived"
            ? t("Archived")
            : t("Reviewable")}
    </span>
  );
}
