import { useLocale } from "./i18n";
import type { Inventory, Thread } from "./types";
import { bytes } from "./types";
/** Display the backend's exact affected IDs; never infer or change deletion scope. */
export default function CleanupScope({
  ids,
  selectedIds,
  records,
  files,
}: {
  ids: string[];
  selectedIds: string[];
  records: ReadonlyMap<string, Thread>;
  files: Inventory["items"];
}) {
  const { t } = useLocale();
  const selected = new Set(selectedIds);
  const measured = new Map(files.map((file) => [file.key, file.logicalBytes]));
  return (
    <ul
      className="cleanup-scope-list"
      tabIndex={0}
      aria-label={t("Conversations to delete")}
    >
      {ids.map((id) => {
        const record = records.get(id);
        const paths = [...new Set(record?.files ?? [])];
        const size =
          paths.length && paths.every((path) => measured.has(path))
            ? bytes(
                paths.reduce((total, path) => total + measured.get(path)!, 0),
              )
            : t("Unknown");
        const child = record?.relation === "spawned";
        return (
          <li
            key={id}
            className={child ? "scope-row scope-child" : "scope-row"}
          >
            <div>
              <strong>{record?.title || id}</strong>
              <small>
                {t(
                  selected.has(id)
                    ? "Selected conversation"
                    : child
                      ? "Related Sub Agent"
                      : "Included conversation",
                )}
                {child && record.parentId && records.has(record.parentId) && (
                  <>
                    {" "}
                    · {t("Parent")}: {records.get(record.parentId)!.title}
                  </>
                )}
              </small>
            </div>
            <span className="scope-size">{size}</span>
          </li>
        );
      })}
    </ul>
  );
}
