import type { RepertoireChapterRef } from "./types";
import { repertoireChapterKey } from "./navigation";

export function RepertoireChapterSelect({
  chapters,
  value,
  onChange,
  disabled = false,
}: {
  chapters: RepertoireChapterRef[];
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  if (!chapters.length) return null;
  const documents = [
    ...new Set(chapters.map((chapter) => chapter.document_id)),
  ];
  return (
    <label className="repertoire-chapter-select">
      Глава
      <select
        aria-label="Глава"
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">Весь репертуар</option>
        {documents.map((documentId) => {
          const items = chapters.filter(
            (chapter) => chapter.document_id === documentId,
          );
          return (
            <optgroup
              key={documentId}
              label={items[0].filename ?? items[0].document_title}
            >
              {items.map((chapter) => (
                <option
                  key={repertoireChapterKey(chapter)}
                  value={repertoireChapterKey(chapter)}
                >
                  {chapter.title}
                </option>
              ))}
            </optgroup>
          );
        })}
      </select>
    </label>
  );
}
