import { useEffect, useRef, useState } from "react";
import {LocaleProvider, LanguageSelector, useLocale, type MessageKey, type MessageValues} from "./i18n";
import {
  ArrowRight,
  Broom,
  Check,
  ChevronRight,
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
  loadCleanupMode,
  errorText,
  sourceLabel,
} from "./bridge";
import {
  bytes,
  type Inventory,
  type Thread,
  type ActivityEntry,
} from "./types";
import CleanupPanel from "./CleanupPanel";
import NativeCleanupPanel from "./NativeCleanupPanel";
type Page = "overview" | "conversations" | "activity" | "cleanup";
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
export default function App() { return <LocaleProvider><AppContent/></LocaleProvider>; }
function AppContent() {
  const {locale, persistenceUnavailable, t, systemText} = useLocale();
  const displayBytes = (n: number | null) => systemText(bytes(n));
  const [cleanupMode, setCleanupMode] = useState<"readOnly" | "syntheticNativeTest" | "native">("readOnly");
  const [inventory, setInventory] = useState<Inventory | null>(null),
    [page, setPage] = useState<Page>("overview"),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(true);
  const [search, setSearch] = useState(""),
    [filter, setFilter] = useState("all"),
    [detailId, setDetailId] = useState<string | null>(null),
    [kept, setKept] = useState<string[]>([]);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [visibleLimit, setVisibleLimit] = useState(50);
  const scanSeq = useRef(0),
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
  async function rescan() {
    const seq = ++scanSeq.current;
    setLoading(true);
    setError("");
    try {
      const [next, mode] = await Promise.all([loadInventory(), loadCleanupMode()]);
      if (seq !== scanSeq.current) return;
      setCleanupMode(mode);
      setInventory(next);
      setKept([]);
      setDetailId(null);
      addActivity(
        mode === "syntheticNativeTest" ? "Generated native test store loaded" : "Local Codex inspected read-only",
        mode === "syntheticNativeTest" ? "{count} generated test files measured; no real Agent data accessed." : "{count} files measured; no source files written, no database opened.",
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
    };
  }, []);
  const total = inventory?.totals.reduce((n, c) => n + c.logicalBytes, 0) ?? 0;
  const protectedCount = (inventory?.threads ?? []).filter(
    (record) => record.protectionReasons.length > 0 || kept.includes(record.id),
  ).length;
  const records = inventory?.threads ?? [];
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
        (filter === "kept" && kept.includes(record.id)) ||
        (["root", "spawned", "fork", "unknown"].includes(filter) && record.relation === filter)),
  );
  const shown = matches.slice(0, visibleLimit);
  const detail = inventory?.threads.find((record) => record.id === detailId);
  function toggleKeep(t: Thread) {
    setKept((k) =>
      k.includes(t.id) ? k.filter((x) => x !== t.id) : [...k, t.id],
    );
    addActivity(
      kept.includes(t.id) ? "Keep choice removed" : "Keep choice added",
      "{title} · choice exists only for this session.", {title:t.title},
    );
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
              { id: "cleanup", label: "Cleanup preview", Icon: Broom },
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
            {t(cleanupMode === "syntheticNativeTest" ? "Generated native test store" : "Local Codex home")}<small>{t("Metadata inspection only")}</small>
          </div>
          <LockKeyhole size={14} />
        </div>
        <div className="sidebar-bottom">
          <div className="safety-tile">
            <ShieldCheck size={22} />
            <strong>{t("Inspection comes first.")}</strong>
            <p>{t(cleanupMode === "syntheticNativeTest" ? "Only generated data can be deleted in this test build." : "Preview local deletion scope. No sample mode, export or database connections.")}</p>
          </div>
          <div className="version">
            <span>{t("v0.1.0 · prototype")}</span>
            <span className="green-dot" />
          </div>
        </div>
      </aside>
      <main className="main">
        {cleanupMode === "syntheticNativeTest" && <p className="scope-banner">{t("Synthetic native cleanup test")} · {t("This build only uses a newly generated test store. Real Agent data is never loaded or deleted.")}</p>}
        <header className="topbar">
          <div className="breadcrumb">{t("Workspace")}<ChevronRight size={13} />{" "}
            <strong>
              {page === "overview"
                ? t("Overview")
                : page === "conversations"
                  ? t("Conversations")
                  : page === "cleanup" ? t("Cleanup preview") : t("Activity")}
            </strong>
          </div>
          <div className="topbar-controls">
            <LanguageSelector/>
          <span className="readonly-badge">
            <ShieldCheck size={13} /> {t(cleanupMode === "syntheticNativeTest" ? "Synthetic native cleanup test" : cleanupMode === "native" ? "Real data · manual cleanup" : "Local Codex · read-only")}
          </span>
          </div>
        </header>
        <div className="content">
          <div className="page-heading">
            <div>
              <div className="eyebrow">{t("LOCAL STORAGE INSPECTION")}</div>
              <h1>
                {page === "overview"
                  ? (t("See your local Codex files."))
                  : page === "conversations"
                    ? t("A clearer view of your history.")
                    : page === "cleanup" ? t("Review cleanup before any deletion.") : t("Inspection, with a paper trail.")}
              </h1>
              <p>
                {page === "overview"
                  ? (t(cleanupMode === "native" ? "Inspect real data; deletion requires your selection, preview and confirmation." : "Read-only inventory and cleanup preview. Real deletion remains locked."))
                  : page === "conversations"
                    ? t("Segments, spawned agents and independent forks — kept distinct.")
                    : page === "cleanup" ? t("Review candidates; no automatic selection.") : t("This session’s inspection events. These are not backups or deletion receipts.")}
              </p>
            </div>
            <button
              className="button secondary"
              onClick={() => void rescan()}
              disabled={loading}
            >
              <RefreshCw size={15} className={loading ? "spin" : ""} />
              {loading ? t("Inspecting…") : t("Rescan Codex")}
            </button>
          </div>
          <div className="inspection-banner">
            <span className="banner-symbol">
              <Sparkles size={17} />
            </span>
            <div>
              <strong>{t(cleanupMode === "syntheticNativeTest" ? "Generated native test store" : "Your local Codex metadata. Inspection only.")}</strong>
              <span>
                {" "}
                {t(cleanupMode === "syntheticNativeTest" ? "The native test backend opens only its generated state database; real Agent data stays untouched." : cleanupMode === "native" ? "Inventory is read-only. Cleanup previews check native state; execution requires explicit confirmation." : "Databases are measured without opening them. Live status is unknown; every record remains protected.")}
              </span>
            </div>
            <span className="scanner-source">{systemText(sourceLabel)}</span>
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
              {t("Inspecting local file metadata…")}
            </div>
          ) : !inventory ? (
            <div className="empty-state">
              <ShieldCheck />
              {t("Local inspection unavailable. No data was changed.")}
              <button
                className="button secondary"
                onClick={() => void rescan()}
              >{t("Retry inspection")}</button>
            </div>
          ) : (
            <>
              {<div className="scope-banner" role="note">
                <strong>{inventory.partialCoverage ? t("Partial inventory · measured home files") : t("Measured home files only")}</strong>
                <p>{t("Only the configured Codex home is measured. External sqlite_home, symlink targets, directory metadata and inaccessible entries are excluded. Allocated blocks are not reclaimable space; shared hardlink blocks are credited to the first lexical path.")}</p>
              </div>}
              {<details className="scan-notes"><summary>{t("Scope and measurement limits ({count})",{count:inventory.warnings.length})}</summary>{inventory.warnings.map(w => <p key={w}>{systemText(w)}</p>)}</details>}
              {page === "overview" && (
                <>
                  <section className="metrics" aria-label={t("Inventory summary")}>
                    <Metric
                      label={t("Measured file lengths")}
                      value={displayBytes(total)}
                      detail={t("{count} local files measured",{count:inventory.items.length})}
                      Icon={Folder}
                    />
                    <Metric
                      label={t("Main & fork conversations")}
                      value={String(inventory.threads.filter(record => record.relation === "root" || record.relation === "fork").length)}
                      detail={t("{spawned} confirmed sub Agents · {unknown} unresolved",{spawned:inventory.threads.filter(record => record.relation === "spawned").length,unknown:inventory.threads.filter(record => record.relation === "unknown").length})}
                      Icon={Layers3}
                    />
                    <Metric
                      label={t("Protected")}
                      value={String(protectedCount)}
                      detail={t(cleanupMode === "native" ? "Cleanup eligibility is checked for each preview." : "All real records remain protected")}
                      Icon={ShieldCheck}
                    />
                    <Metric
                      label={t("Allocated blocks")}
                      value={displayBytes(allocatedTotal)}
                      detail={t("Filesystem accounting; not reclaimable space")}
                      Icon={Folder}
                    />
                  </section>
                  <div className="overview-grid">
                    <section className="panel storage-panel">
                      <div className="panel-heading">
                        <div>
                          <h2>{t("Where the bytes live")}</h2>
                          <p>
                            {t("File lengths and allocated blocks, grouped by category.")}
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
                            <strong>{displayBytes(c.logicalBytes)}{<small>{t("{size} allocated",{size:displayBytes(c.allocatedBytes)})}</small>}</strong>
                          </div>
                        ))}
                      </div>
                      <div className="panel-footnote">
                        {t("Databases, WAL and SHM are stat-only. Shared database bytes are not attributed to individual conversations.")}
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
                          <strong>{t("Unverified live status")}</strong>
                          <p>{t("Pinned, referenced and incomplete records carry explicit reasons. Runtime activity is not queried.")}</p>
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
                          <Status thread={record} kept={kept.includes(record.id)} />
                          <strong>{!record.files.length ? t("Unattributed") : displayBytes(record.inclusiveBytes)}</strong>
                          <ChevronRight size={16} />
                        </button>
                      ))}
                    </div>
                  </section>
                  <div className="bottom-note">
                    <ShieldCheck size={15} />
                    {t(cleanupMode === "native" ? "Inventory does not authorize deletion. Review native cleanup before confirming execution." : "All real records remain protected. Cleanup previews do not authorize deletion.")}

                  </div>
                </>
              )}
              {page === "cleanup" && (cleanupMode !== "readOnly" ? <NativeCleanupPanel key={`${inventory.sourceFingerprint}:${kept.join(",")}`} inventory={inventory} kept={kept} testOnly={cleanupMode === "syntheticNativeTest"} refresh={() => void rescan()}/> : <CleanupPanel key={`${inventory.sourceFingerprint}:${kept.join(",")}`} inventory={inventory} kept={kept}/>)}
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

                      {<><option value="root">{t("Main conversations")}</option><option value="spawned">{t("Sub Agents")}</option><option value="fork">{t("Independent forks")}</option><option value="unknown">{t("Unresolved")}</option></>}
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
                            <button
                              className="row-title"
                              onClick={() => setDetailId(record.id)}
                            >
                              <strong>{record.title}</strong>
                              <span>
                                {`${systemText(record.relation)} · ${systemText(record.sourceType || t("unknown"))} · `}{record.project} · {t("{count} segments",{count:record.files.length})}
                                {record.ownedDescendantIds.length ? ` · ${t("{count} spawned", {count:record.ownedDescendantIds.length})}` : ""}
                              </span>
                            </button>
                            <Status thread={record} kept={kept.includes(record.id)} />
                            <strong className="row-size">
                              {!record.files.length ? t("Unattributed") : displayBytes(record.inclusiveBytes)}
                            </strong>
                          </div>
                        ))
                      ) : (
                        <div className="empty-state">{t("No conversations match this view.")}</div>
                      )}
                      {matches.length > shown.length && <button className="button secondary" onClick={() => setVisibleLimit(v => v + 50)}>{t("Show 50 more")}</button>}
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
                        />
                        <div className="detail-facts">
                          <span>{t("Project")}<strong>{detail.project}</strong>
                          </span>
                          <span>{t("Own files")}<strong>{!detail.files.length ? t("Unknown / unattributed") : displayBytes(detail.ownBytes)}</strong>
                          </span>
                          <span>{t("With spawned records")}<strong>{!detail.files.length ? t("Unknown / unattributed") : displayBytes(detail.inclusiveBytes)}</strong>
                          </span>
                          <span>{t("Relation")}<strong>{systemText(detail.relation)}</strong>
                          </span>
                          <span>
                            {t("Index / header timestamp")}
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
                        {<>
                          <h3>{t("Relationship evidence")}</h3>
                          {(detail.relationEvidence ?? []).map(e => <p className="tiny-note" key={e}>{systemText(e)}</p>)}
                          {detail.parentId && <p>{t("Parent")}<button className="text-button" disabled={!inventory.threads.some(record => record.id === detail.parentId)} onClick={() => setDetailId(detail.parentId)}>{detail.parentId}</button></p>}
                          {(detail.forkSources ?? []).map(id => <p key={id}>{t("Fork lineage")}<button className="text-button" disabled={!inventory.threads.some(record => record.id === id)} onClick={() => setDetailId(id)}>{id}</button></p>)}
                          {(detail.historySources ?? []).map(id => <p key={id}>{t("Shared history")}<button className="text-button" disabled={!inventory.threads.some(record => record.id === id)} onClick={() => setDetailId(id)}>{id}</button></p>)}
                        </>}
                        <h3>{t("Attributed rollout files")}</h3>
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
                    {t("Sizes include attributed rollout files only. Shared databases, fork histories and unresolved children are not added to a conversation’s footprint.")}
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
function Status({ kept }: { thread: Thread; kept: boolean }) {
  const {t} = useLocale();
  return <span className="status protected"><ShieldCheck size={12}/>{t("Protected")}{kept ? t(" · kept this session") : ""}</span>;
}
