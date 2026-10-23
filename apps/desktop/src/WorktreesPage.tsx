import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { GitBranch, RefreshCw, Search, LockKeyhole } from "lucide-react";
import { useLocale, type MessageKey } from "./i18n";
import { bytes } from "./types";
import {
  inspectWorktrees,
  worktreeInspectionAvailable,
  worktreeMessages,
  worktreeRows,
  sortWorktrees,
  type WorktreeInventory,
} from "./worktrees";

const recommendations: Record<string, MessageKey> = {
  candidate: "Consider cleanup",
  keep: "Keep worktree",
  review: "Review required",
};
const mergeLabels: Record<string, MessageKey> = {
  contained: "HEAD contained",
  notContained: "HEAD not contained",
  unknown: "Containment unknown",
};
const basename = (path: string) =>
  path.split("/").filter(Boolean).at(-1) ?? path;

export default function WorktreesPage({ projects }: { projects: string[] }) {
  const { t, locale } = useLocale();
  const [repository, setRepository] = useState(""),
    [target, setTarget] = useState("");
  const [inventory, setInventory] = useState<WorktreeInventory | null>(null),
    [loading, setLoading] = useState(false),
    [error, setError] = useState("");
  const [query, setQuery] = useState(""),
    [filter, setFilter] = useState("all"),
    [sort, setSort] = useState("size"),
    [offset, setOffset] = useState(0);
  const generation = useRef(0),
    initialProjects = useRef(projects);
  const lastRequest = useRef<{
    repository: string | null;
    target: string | null;
    projects: string[];
  } | null>(null);
  const available = worktreeInspectionAvailable();
  const message = (code: string) =>
    t(worktreeMessages[code] ?? "Worktree inspection failed. Scan again.");
  const date = (value: number | null) => {
    const instant = value === null ? null : new Date(value * 1000);
    return !instant || Number.isNaN(instant.getTime())
      ? t("Unknown")
      : new Intl.DateTimeFormat(locale, {
          year: "numeric",
          month: "short",
          day: "numeric",
          hour: "2-digit",
          minute: "2-digit",
        }).format(instant);
  };
  async function scan(repo = repository, ref = target, candidates = projects) {
    const request = {
      repository: repo.trim() || null,
      target: ref.trim() || null,
      projects: repo.trim() ? [] : candidates.slice(0, 512),
    };
    lastRequest.current = request;
    const seq = ++generation.current;
    setLoading(true);
    setError("");
    try {
      const next = await inspectWorktrees(request);
      if (seq !== generation.current) return;
      setInventory(next);
      setOffset(0);
    } catch (e) {
      if (seq !== generation.current) return;
      setInventory(null);
      setError(
        message(
          e && typeof e === "object" && "code" in e
            ? String(e.code)
            : "worktreeGitUnavailable",
        ),
      );
    } finally {
      if (seq === generation.current) setLoading(false);
    }
  }
  useEffect(() => {
    if (available) void scan("", "", initialProjects.current);
    return () => {
      generation.current++;
    };
  }, [available]);
  useEffect(() => {
    const previous = lastRequest.current;
    // Inventory arrives independently. Wait for the active read, then fill in newly
    // discovered candidates without replacing an explicitly submitted repository.
    if (
      available &&
      !loading &&
      previous &&
      previous.repository === null &&
      JSON.stringify(previous.projects) !==
        JSON.stringify(projects.slice(0, 512))
    ) {
      void scan("", previous.target ?? "", projects);
    }
  }, [available, loading, projects]);
  useEffect(() => {
    setOffset(0);
  }, [query, filter, sort]);
  const rows = useMemo(() => worktreeRows(inventory), [inventory]);
  const matching = useMemo(
    () =>
      sortWorktrees(
        rows.filter(
          (row) =>
            `${row.path} ${row.branch ?? ""} ${row.repository.path}`
              .toLowerCase()
              .includes(query.toLowerCase()) &&
            (filter === "all" ||
              row.recommendation === filter ||
              (filter === "contained" && row.mergeStatus === "contained")),
        ),
        sort,
      ),
    [rows, query, filter, sort],
  );
  const visible = matching.slice(offset, offset + 50);
  const complete = rows.every((row) => row.complete);
  function submit(e: FormEvent) {
    e.preventDefault();
    if (!loading && available) void scan();
  }
  return (
    <section className="worktrees-page" aria-label={t("Worktrees")}>
      <div className="page-heading">
        <div>
          <h1>{t("Worktrees")}</h1>
          <p>
            {t(
              "Inspect merged work and local changes before deciding what to keep.",
            )}
          </p>
        </div>
        <span className="read-only-pill">{t("Read-only")}</span>
      </div>
      {!available && (
        <p className="notice">
          {t("Worktree inspection is available in the regular desktop app.")}
        </p>
      )}
      <form className="worktree-scan-form" onSubmit={submit}>
        <label>
          {t("Repository path")}
          <input
            aria-label={t("Repository path")}
            placeholder={t(
              "Auto-discover Codex worktrees and conversation projects",
            )}
            value={repository}
            onChange={(e) => setRepository(e.target.value)}
            disabled={loading || !available}
          />
        </label>
        <label>
          {t("Target branch")}
          <input
            aria-label={t("Target branch")}
            placeholder={t("Auto: origin/HEAD, main, master")}
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            disabled={loading || !available}
          />
        </label>
        <button className="button primary" disabled={loading || !available}>
          <RefreshCw size={16} className={loading ? "spinning" : ""} />
          {t(loading ? "Inspecting worktrees…" : "Scan worktrees")}
        </button>
      </form>
      <p className="tiny-note">
        {t(
          "Uses local Git references without fetching. No worktrees or branches are modified.",
        )}
      </p>
      {error && (
        <p role="alert" className="alert">
          {error}
        </p>
      )}
      {loading && <p role="status">{t("Inspecting worktrees…")}</p>}
      {inventory && (
        <>
          <div className="worktree-stats">
            <div>
              <strong>{inventory.repositories.length}</strong>
              <span>{t("Repositories")}</span>
            </div>
            <div>
              <strong>{rows.length}</strong>
              <span>{t("Worktrees")}</span>
            </div>
            <div>
              <strong>
                {rows.filter((r) => r.recommendation === "candidate").length}
              </strong>
              <span>{t("Consider cleanup")}</span>
            </div>
            <div>
              <strong>
                {!complete ? "≥ " : ""}
                {bytes(rows.reduce((n, r) => n + r.logicalBytes, 0))}
              </strong>
              <span>{t("Measured worktree files")}</span>
            </div>
          </div>
          <p className="tiny-note">
            {t(
              "Excludes shared Git storage. File lengths are not reclaimable disk space.",
            )}{" "}
            {t("Scanned {time}", { time: date(inventory.scannedAt) })}
          </p>
          {!!inventory.warnings.length && (
            <div className="notice" role="status">
              {inventory.warnings.map((code) => (
                <p key={code}>{message(code)}</p>
              ))}
            </div>
          )}
          <div className="worktree-controls">
            <label className="worktree-search">
              <Search size={16} />
              <input
                aria-label={t("Search worktrees")}
                placeholder={t("Search worktrees")}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </label>
            <select
              aria-label={t("Filter worktrees")}
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            >
              <option value="all">{t("All worktrees")}</option>
              <option value="contained">{t("HEAD contained")}</option>
              {Object.entries(recommendations).map(([value, label]) => (
                <option key={value} value={value}>
                  {t(label)}
                </option>
              ))}
            </select>
            <select
              aria-label={t("Sort worktrees")}
              value={sort}
              onChange={(e) => setSort(e.target.value)}
            >
              <option value="size">{t("Largest first")}</option>
              <option value="recent">{t("Recently modified first")}</option>
              <option value="oldest">{t("Oldest modification first")}</option>
              <option value="commit">{t("Latest commit first")}</option>
              <option value="name">{t("Name")}</option>
            </select>
          </div>
          <div className="table-scroll worktree-table" aria-busy={loading}>
            <table>
              <thead>
                <tr>
                  <th>{t("Repository / worktree")}</th>
                  <th>{t("Containment")}</th>
                  <th>{t("Local changes")}</th>
                  <th>{t("Recent source modification")}</th>
                  <th>{t("Size")}</th>
                  <th>{t("Assessment")}</th>
                </tr>
              </thead>
              <tbody>
                {visible.map((row) => (
                  <tr key={row.path}>
                    <td>
                      <strong className="worktree-name">
                        {basename(row.path)}
                      </strong>
                      <span className="muted worktree-secondary">
                        <GitBranch size={13} />
                        {row.branch ?? t("Detached HEAD")}
                        {row.locked && (
                          <LockKeyhole size={13} aria-label={t("Locked")} />
                        )}
                      </span>
                      <span className="muted worktree-secondary">
                        {basename(row.repository.path)}
                        {row.mainCheckout ? ` · ${t("Main checkout")}` : ""}
                      </span>
                    </td>
                    <td>
                      <span className={`worktree-merge ${row.mergeStatus}`}>
                        {t(mergeLabels[row.mergeStatus])}
                      </span>
                      <span className="worktree-secondary muted">
                        {row.aheadCount !== null
                          ? t("{count} commits outside target", {
                              count: row.aheadCount,
                            })
                          : t("Unknown")}
                      </span>
                    </td>
                    <td>
                      <span>
                        {t("Modified {count}", {
                          count: row.modifiedCount ?? t("Unknown"),
                        })}
                      </span>
                      <span className="worktree-secondary muted">
                        {t("Untracked {count}", {
                          count: row.untrackedCount ?? t("Unknown"),
                        })}
                      </span>
                      <span className="worktree-secondary muted">
                        {t("Ignored entries {count}", {
                          count: row.ignoredCount ?? t("Unknown"),
                        })}
                      </span>
                    </td>
                    <td>
                      <span>{date(row.sourceModifiedAt)}</span>
                      <span className="worktree-secondary muted">
                        {t("Commit: {time}", { time: date(row.lastCommitAt) })}
                      </span>
                    </td>
                    <td className="worktree-size">
                      {!row.complete ? "≥ " : ""}
                      {bytes(row.logicalBytes)}
                      {!row.complete && (
                        <span className="worktree-secondary muted">
                          {t("Incomplete")}
                        </span>
                      )}
                    </td>
                    <td>
                      <strong
                        className={`worktree-assessment ${row.recommendation}`}
                      >
                        {t(recommendations[row.recommendation])}
                      </strong>
                      <details className="worktree-details">
                        <summary>{t("Reasons and paths")}</summary>
                        <ul>
                          {row.reasons.map((code) => (
                            <li key={code}>{message(code)}</li>
                          ))}
                          {!row.reasons.length && (
                            <li>
                              {t(
                                "HEAD is contained and no local changes were observed. Confirm the directory is no longer needed.",
                              )}
                            </li>
                          )}
                        </ul>
                        <dl>
                          <dt>{t("Path")}</dt>
                          <dd>{row.path}</dd>
                          <dt>{t("HEAD")}</dt>
                          <dd>{row.head || t("Unknown")}</dd>
                          <dt>{t("Target branch")}</dt>
                          <dd>{row.repository.targetRef ?? t("Unknown")}</dd>
                          <dt>{t("Target commit")}</dt>
                          <dd>{row.repository.targetOid ?? t("Unknown")}</dd>
                          {row.lockReason && (
                            <>
                              <dt>{t("Lock reason")}</dt>
                              <dd>{row.lockReason}</dd>
                            </>
                          )}
                        </dl>
                        {row.repository.shallow && (
                          <p>
                            {t(
                              "Shallow history can make containment inconclusive.",
                            )}
                          </p>
                        )}
                        {row.repository.warnings.map((code) => (
                          <p key={code}>{message(code)}</p>
                        ))}
                      </details>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!matching.length && (
              <div className="empty-state">
                <GitBranch size={28} />
                <p>
                  {t(
                    rows.length
                      ? "No worktrees match these filters."
                      : "No worktrees found. Enter a repository path and scan again.",
                  )}
                </p>
              </div>
            )}
          </div>
          <div className="table-footer">
            <span>
              {t("{count} worktrees shown", { count: matching.length })}
            </span>
            <div className="pagination">
              <button
                className="button secondary"
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - 50))}
              >
                {t("Previous page")}
              </button>
              <span>
                {Math.floor(offset / 50) + 1} /{" "}
                {Math.max(1, Math.ceil(matching.length / 50))}
              </span>
              <button
                className="button secondary"
                disabled={offset + 50 >= matching.length}
                onClick={() => setOffset(offset + 50)}
              >
                {t("Next page")}
              </button>
            </div>
          </div>
        </>
      )}
      <p className="tiny-note">
        {t(
          "Source modification is an observed file timestamp, not last use. Dependencies and build output are excluded; deletions may not be reflected. Containment alone does not prove a squash or cherry-pick merge.",
        )}
      </p>
    </section>
  );
}
