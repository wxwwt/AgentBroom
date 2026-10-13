import { useEffect, useRef, useState } from "react";
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
} from "./bridge";
import {
  bytes,
  type Inventory,
  type Thread,
  type Preview,
  type ActivityEntry,
} from "./types";
type Page = "overview" | "conversations" | "activity";
const categoryNames: Record<string, string> = {
  sessions: "Session files",
  archives: "Archived sessions",
  history: "History database",
  logs: "Logs & WAL",
  assets: "Generated assets",
};
const categoryColors: Record<string, string> = {
  sessions: "#d49a35",
  archives: "#78977d",
  history: "#8999ba",
  logs: "#b5a1b2",
  assets: "#b7b3a5",
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
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content className="modal">
          <Dialog.Title>{title}</Dialog.Title>
          <Dialog.Description>{description}</Dialog.Description>
          <Dialog.Close
            className="icon-button modal-close"
            aria-label="Close dialog"
          >
            <X size={19} />
          </Dialog.Close>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export default function App() {
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
  const scanSeq = useRef(0),
    reviewSeq = useRef(0),
    entryId = useRef(0);
  const addActivity = (title: string, description: string) =>
    setActivity((a) =>
      [
        {
          id: ++entryId.current,
          title,
          description,
          time: new Date().toLocaleTimeString([], {
            hour: "2-digit",
            minute: "2-digit",
          }),
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
  async function rescan() {
    const seq = ++scanSeq.current;
    invalidate();
    setLoading(true);
    setError("");
    try {
      const next = await loadInventory();
      if (seq !== scanSeq.current) return;
      setInventory(next);
      setSelected([]);
      setKept([]);
      setDetailId(null);
      addActivity(
        "Demo inventory inspected",
        `${next.items.length} bundled synthetic files. No real agent data accessed.`,
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
    inventory?.threads.filter((t) => t.relation !== "spawned") ?? [];
  const total = inventory?.totals.reduce((n, c) => n + c.logicalBytes, 0) ?? 0;
  const protectedCount = roots.filter(
    (t) => t.protectionReasons.length > 0 || kept.includes(t.id),
  ).length;
  const reviewable = roots.filter(
    (t) => !t.protectionReasons.length && !kept.includes(t.id),
  );
  const sorted = [...roots].sort(
    (a, b) => b.inclusiveBytes - a.inclusiveBytes || a.id.localeCompare(b.id),
  );
  const shown = sorted.filter(
    (t) =>
      `${t.title} ${t.project} ${t.id}`
        .toLowerCase()
        .includes(search.toLowerCase()) &&
      (filter === "all" ||
        (filter === "protected" &&
          (t.protectionReasons.length > 0 || kept.includes(t.id))) ||
        (filter === "reviewable" &&
          !t.protectionReasons.length &&
          !kept.includes(t.id)) ||
        (filter === "kept" && kept.includes(t.id))),
  );
  const detail = inventory?.threads.find((t) => t.id === detailId);
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
      `${t.title} · choice exists only for this session.`,
    );
  }
  async function review(asReport = false) {
    if (!inventory) return;
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
          `${p.affectedIds.length} synthetic records · execution is disabled.`,
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
            AgentBroom<small>STORAGE, CLEARLY.</small>
          </span>
        </div>
        <div className="nav-label">WORKSPACE</div>
        <nav aria-label="Main navigation">
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
              {label}
              {id === "conversations" && inventory && (
                <span className="nav-count">{roots.length}</span>
              )}
            </button>
          ))}
        </nav>
        <div className="sidebar-divider" />
        <div className="nav-label">SOURCE</div>
        <div className="source-tile">
          <span className="source-dot" />
          <div>
            Codex sample store<small>Bundled synthetic fixtures</small>
          </div>
          <LockKeyhole size={14} />
        </div>
        <div className="sidebar-bottom">
          <div className="safety-tile">
            <ShieldCheck size={22} />
            <strong>Inspection comes first.</strong>
            <p>This prototype has no access to your real agent history.</p>
          </div>
          <div className="version">
            <span>v0.1.0 · prototype</span>
            <span className="green-dot" />
          </div>
        </div>
      </aside>
      <main className="main">
        <header className="topbar">
          <div className="breadcrumb">
            Workspace <ChevronRight size={13} />{" "}
            <strong>
              {page === "overview"
                ? "Overview"
                : page === "conversations"
                  ? "Conversations"
                  : "Activity"}
            </strong>
          </div>
          <span className="readonly-badge">
            <ShieldCheck size={13} /> Demo dataset · read-only
          </span>
        </header>
        <div className="content">
          <div className="page-heading">
            <div>
              <div className="eyebrow">LOCAL STORAGE INSPECTION</div>
              <h1>
                {page === "overview"
                  ? "Know what’s taking up space."
                  : page === "conversations"
                    ? "A clearer view of your history."
                    : "Inspection, with a paper trail."}
              </h1>
              <p>
                {page === "overview"
                  ? "Understand the footprint. Keep the context. Make an informed decision."
                  : page === "conversations"
                    ? "Segments, spawned agents and independent forks — kept distinct."
                    : "This session’s inspection events. These are not backups or deletion receipts."}
              </p>
            </div>
            <button
              className="button secondary"
              onClick={() => void rescan()}
              disabled={loading}
            >
              <RefreshCw size={15} className={loading ? "spin" : ""} />
              {loading ? "Inspecting…" : "Rescan demo"}
            </button>
          </div>
          <div className="demo-banner">
            <span className="banner-symbol">
              <Sparkles size={17} />
            </span>
            <div>
              <strong>You’re exploring a sample dataset.</strong>
              <span>
                {" "}
                Only bundled synthetic files are inspected. No cleanup is
                available.
              </span>
            </div>
            <span className="demo-source">{sourceLabel}</span>
          </div>
          {error && (
            <div role="alert" className="error-banner">
              {error}
            </div>
          )}
          {loading ? (
            <div className="empty-state">
              <RefreshCw className="spin" />
              Loading the bundled demo inventory…
            </div>
          ) : !inventory ? (
            <div className="empty-state">
              <ShieldCheck />
              Demo unavailable. No fallback to a real agent store.
              <button
                className="button secondary"
                onClick={() => void rescan()}
              >
                Retry demo
              </button>
            </div>
          ) : (
            <>
              {page === "overview" && (
                <>
                  <section className="metrics" aria-label="Inventory summary">
                    <Metric
                      label="Sample storage"
                      value={bytes(total)}
                      detail={`${inventory.items.length} fixture files measured`}
                      Icon={Folder}
                    />
                    <Metric
                      label="Conversations"
                      value={String(roots.length)}
                      detail={`${inventory.threads.filter((t) => t.relation === "spawned").length} spawned records, counted separately`}
                      Icon={Layers3}
                    />
                    <Metric
                      label="Protected"
                      value={String(protectedCount)}
                      detail="Active, referenced or incomplete"
                      Icon={ShieldCheck}
                    />
                    <Metric
                      label="Reviewable"
                      value={String(reviewable.length)}
                      detail="Metadata checks passed in the demo"
                      Icon={CircleCheck}
                    />
                  </section>
                  <div className="overview-grid">
                    <section className="panel storage-panel">
                      <div className="panel-heading">
                        <div>
                          <h2>Where the bytes live</h2>
                          <p>
                            Measured sample file sizes, grouped by category.
                          </p>
                        </div>
                        <span className="quiet-chip">Logical size</span>
                      </div>
                      <div className="storage-bar">
                        {inventory.totals.map((c) => (
                          <span
                            key={c.category}
                            style={{
                              width: `${(c.logicalBytes / total) * 100}%`,
                              background: categoryColors[c.category],
                            }}
                            title={`${categoryNames[c.category]}: ${bytes(c.logicalBytes)}`}
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
                              {categoryNames[c.category]}
                              <small>{c.files} files</small>
                            </span>
                            <strong>{bytes(c.logicalBytes)}</strong>
                          </div>
                        ))}
                      </div>
                      <div className="panel-footnote">
                        Database and WAL placeholders are measured only. They
                        are never opened as databases.
                      </div>
                    </section>
                    <section className="panel protection-panel">
                      <div className="panel-heading">
                        <div>
                          <h2>What stays protected</h2>
                          <p>Uncertainty is a reason to stop.</p>
                        </div>
                        <ShieldCheck size={23} />
                      </div>
                      <div className="protection-item">
                        <GitBranch size={19} />
                        <div>
                          <strong>Shared history references</strong>
                          <p>Independent forks do not become owned children.</p>
                        </div>
                      </div>
                      <div className="protection-item">
                        <Clock3 size={19} />
                        <div>
                          <strong>Active conversations</strong>
                          <p>A completed reply is not task acceptance.</p>
                        </div>
                      </div>
                      <div className="protection-item">
                        <LockKeyhole size={19} />
                        <div>
                          <strong>Incomplete metadata</strong>
                          <p>Oversized and unread records stay unresolved.</p>
                        </div>
                      </div>
                      <button
                        className="text-button"
                        onClick={() => {
                          setFilter("protected");
                          setPage("conversations");
                        }}
                      >
                        Inspect protected records <ArrowRight size={15} />
                      </button>
                    </section>
                  </div>
                  <section className="panel">
                    <div className="panel-heading">
                      <div>
                        <h2>Largest conversations</h2>
                        <p>
                          Includes proved spawned records. Shared database
                          overhead stays separate.
                        </p>
                      </div>
                      <button
                        className="text-button"
                        onClick={() => setPage("conversations")}
                      >
                        View all <ArrowRight size={14} />
                      </button>
                    </div>
                    <div className="largest-list">
                      {sorted.slice(0, 4).map((t) => (
                        <button key={t.id} onClick={() => openDetail(t.id)}>
                          <span className="conversation-icon">
                            <FileText size={18} />
                          </span>
                          <span className="largest-title">
                            {t.title}
                            <small>
                              {t.project} · {t.files.length} segment
                              {t.files.length !== 1 ? "s" : ""}
                            </small>
                          </span>
                          <Status thread={t} kept={kept.includes(t.id)} />
                          <strong>{bytes(t.inclusiveBytes)}</strong>
                          <ChevronRight size={16} />
                        </button>
                      ))}
                    </div>
                  </section>
                  <div className="bottom-note">
                    <ShieldCheck size={15} />
                    No disk-free-space query, real agent discovery or deletion
                    was performed.
                    <button
                      className="text-button"
                      onClick={() => void review(true)}
                    >
                      View sample report <FileText size={14} />
                    </button>
                  </div>
                </>
              )}
              {page === "conversations" && (
                <>
                  <div className="toolbar">
                    <label className="search">
                      <Search size={16} />
                      <input
                        aria-label="Search conversations"
                        placeholder="Search title, project or identity…"
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                      />
                    </label>
                    <select
                      aria-label="Filter conversations"
                      value={filter}
                      onChange={(e) => setFilter(e.target.value)}
                    >
                      <option value="all">All conversations</option>
                      <option value="reviewable">Reviewable</option>
                      <option value="protected">Protected</option>
                      <option value="kept">Kept this session</option>
                    </select>
                    <span className="results-count">
                      {shown.length} records
                    </span>
                  </div>
                  <div
                    className={`conversation-layout ${detail ? "with-detail" : ""}`}
                  >
                    <section className="panel conversation-table">
                      <div className="table-head">
                        <span />
                        <span>CONVERSATION</span>
                        <span>STATUS</span>
                        <span>FOOTPRINT</span>
                      </div>
                      {shown.length ? (
                        shown.map((t) => (
                          <div
                            key={t.id}
                            className={`table-row ${detailId === t.id ? "focused" : ""}`}
                          >
                            <input
                              type="checkbox"
                              aria-label={`Select ${t.title}`}
                              checked={selected.includes(t.id)}
                              disabled={
                                t.protectionReasons.length > 0 ||
                                kept.includes(t.id)
                              }
                              onChange={() => toggleSelection(t.id)}
                            />
                            <button
                              className="row-title"
                              onClick={() => setDetailId(t.id)}
                            >
                              <strong>{t.title}</strong>
                              <span>
                                {t.project} · {t.files.length} segment
                                {t.files.length !== 1 ? "s" : ""}
                                {t.ownedDescendantIds.length
                                  ? ` · ${t.ownedDescendantIds.length} spawned`
                                  : ""}
                              </span>
                            </button>
                            <Status thread={t} kept={kept.includes(t.id)} />
                            <strong className="row-size">
                              {bytes(t.inclusiveBytes)}
                            </strong>
                          </div>
                        ))
                      ) : (
                        <div className="empty-state">
                          No conversations match this view.
                        </div>
                      )}
                      <div className="table-footer">
                        <span>
                          {selected.length} selected · inspection only
                        </span>
                        <button
                          className="button primary"
                          disabled={!selected.length || busy}
                          onClick={() => void review()}
                        >
                          {busy ? "Preparing…" : "Review selection"}
                          <ArrowRight size={14} />
                        </button>
                      </div>
                    </section>
                    {detail && (
                      <aside className="panel detail-panel">
                        <div className="detail-top">
                          <span className="eyebrow">CONVERSATION DETAILS</span>
                          <button
                            className="icon-button"
                            aria-label="Close details"
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
                          <span>
                            Project<strong>{detail.project}</strong>
                          </span>
                          <span>
                            Own files<strong>{bytes(detail.ownBytes)}</strong>
                          </span>
                          <span>
                            With spawned records
                            <strong>{bytes(detail.inclusiveBytes)}</strong>
                          </span>
                          <span>
                            Relation<strong>{detail.relation}</strong>
                          </span>
                          <span>
                            Recorded activity
                            <strong>
                              {detail.updatedAt
                                ? detail.updatedAt.slice(0, 10)
                                : "Unknown"}
                            </strong>
                          </span>
                        </div>
                        <div className="id-box">{detail.id}</div>
                        {detail.protectionReasons.length > 0 && (
                          <div className="protection-reasons">
                            <ShieldCheck size={17} />
                            <div>
                              {detail.protectionReasons.map((r) => (
                                <p key={r}>{r}</p>
                              ))}
                            </div>
                          </div>
                        )}
                        <h3>Owned sample files</h3>
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
                            <h3>Spawned records</h3>
                            <ul className="file-list">
                              {detail.ownedDescendantIds.map((id) => (
                                <li key={id}>
                                  <Layers3 size={13} />
                                  {id}
                                </li>
                              ))}
                            </ul>
                          </>
                        )}
                        {detail.sharedHistoryFrom && (
                          <div className="fork-note">
                            <GitBranch size={15} />
                            Independent fork referencing{" "}
                            {detail.sharedHistoryFrom}. The source is not owned
                            by this record.
                          </div>
                        )}
                        <button
                          className={`button ${kept.includes(detail.id) ? "kept-button" : "secondary"}`}
                          onClick={() => toggleKeep(detail)}
                        >
                          <ShieldCheck size={15} />
                          {kept.includes(detail.id)
                            ? "Remove keep choice"
                            : "Keep this conversation"}
                        </button>
                        <p className="tiny-note">
                          Keep choices exist in memory for this session only.
                          They do not change an agent’s retention rules.
                        </p>
                      </aside>
                    )}
                  </div>
                  <div className="bottom-note">
                    <LockKeyhole size={15} />
                    Reviewable means demo metadata checks passed. It does not
                    mean safe to delete.
                  </div>
                </>
              )}
              {page === "activity" && (
                <section className="panel activity-panel">
                  <div className="panel-heading">
                    <div>
                      <h2>This session</h2>
                      <p>
                        Inspection events only. No records have been deleted.
                      </p>
                    </div>
                    <span className="quiet-chip">In-memory</span>
                  </div>
                  {activity.length ? (
                    activity.map((a) => (
                      <div className="activity-row" key={a.id}>
                        <span className="activity-icon">
                          <Check size={16} />
                        </span>
                        <div>
                          <strong>{a.title}</strong>
                          <p>{a.description}</p>
                        </div>
                        <time>{a.time}</time>
                      </div>
                    ))
                  ) : (
                    <div className="empty-state">
                      Inspection events will appear here.
                    </div>
                  )}
                  <div className="panel-footnote">
                    This activity log is not a backup and cannot restore
                    anything. It clears when the app reloads.
                  </div>
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
        title="Review the sample selection"
        description="An exact, read-only preview. There is no executable cleanup action."
      >
        {preview && (
          <>
            <div className="preview-summary">
              <span>{preview.selectedIds.length} selected conversations</span>
              <strong>{bytes(preview.logicalBytes)}</strong>
              <small>sample file bytes · not freed space</small>
            </div>
            <div className="preview-rows">
              {preview.rows.map((t) => (
                <div key={t.id}>
                  <FileText size={16} />
                  <span>
                    {t.title}
                    <small>
                      {t.id} · {t.relation}
                    </small>
                  </span>
                  <strong>{bytes(t.ownBytes)}</strong>
                </div>
              ))}
            </div>
            <div className="locked-callout">
              <LockKeyhole size={18} />
              <div>
                <strong>Execution is disabled.</strong>
                <p>
                  No deletion, database changes or backup operations exist in
                  this prototype.
                </p>
              </div>
            </div>
            <button
              className="button secondary"
              onClick={() => {
                setPreview(null);
                void review(true);
              }}
            >
              <FileText size={15} />
              View JSON report
            </button>
          </>
        )}
      </Modal>
      <Modal
        open={report !== null}
        onOpenChange={(v) => {
          if (!v) setReport(null);
        }}
        title="Read-only sample report"
        description="Sanitized fixture metadata. This report is not a backup or a deletion receipt."
      >
        <textarea
          className="report-text"
          aria-label="Sample JSON report"
          readOnly
          value={report ?? ""}
        />
        <p className="tiny-note">
          All identities and paths belong to bundled synthetic fixtures. No real
          history is included.
        </p>
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
function Status({ thread, kept }: { thread: Thread; kept: boolean }) {
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
        ? "Kept"
        : thread.protectionReasons.length
          ? "Protected"
          : thread.activity === "archived"
            ? "Archived"
            : "Reviewable"}
    </span>
  );
}
