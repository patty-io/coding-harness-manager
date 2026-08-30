import { useEffect, useMemo, useRef, useState } from "react";

export type DiffLine = {
  kind: "context" | "added" | "removed";
  text: string;
  oldLine?: number;
  newLine?: number;
  changeId?: number;
};

function splitLines(content: string | null): string[] {
  if (content === null) return [];
  return content.split(/\r?\n/);
}

/**
 * Produces a line-oriented diff without requiring a native diff package. The
 * LCS backtrack keeps unchanged lines as context and groups adjacent changed
 * lines into navigable hunks.
 */
export function buildLineDiff(before: string | null, after: string | null): DiffLine[] {
  const oldLines = splitLines(before);
  const newLines = splitLines(after);
  const lcs = Array.from({ length: oldLines.length + 1 }, () =>
    new Array<number>(newLines.length + 1).fill(0),
  );

  for (let oldIndex = oldLines.length - 1; oldIndex >= 0; oldIndex -= 1) {
    for (let newIndex = newLines.length - 1; newIndex >= 0; newIndex -= 1) {
      lcs[oldIndex][newIndex] =
        oldLines[oldIndex] === newLines[newIndex]
          ? lcs[oldIndex + 1][newIndex + 1] + 1
          : Math.max(lcs[oldIndex + 1][newIndex], lcs[oldIndex][newIndex + 1]);
    }
  }

  const lines: DiffLine[] = [];
  let oldIndex = 0;
  let newIndex = 0;
  let oldLine = 1;
  let newLine = 1;
  while (oldIndex < oldLines.length || newIndex < newLines.length) {
    if (
      oldIndex < oldLines.length &&
      newIndex < newLines.length &&
      oldLines[oldIndex] === newLines[newIndex]
    ) {
      lines.push({
        kind: "context",
        text: oldLines[oldIndex],
        oldLine,
        newLine,
      });
      oldIndex += 1;
      newIndex += 1;
      oldLine += 1;
      newLine += 1;
    } else if (
      newIndex < newLines.length &&
      (oldIndex >= oldLines.length || lcs[oldIndex][newIndex + 1] > lcs[oldIndex + 1][newIndex])
    ) {
      lines.push({ kind: "added", text: newLines[newIndex], newLine });
      newIndex += 1;
      newLine += 1;
    } else {
      lines.push({ kind: "removed", text: oldLines[oldIndex], oldLine });
      oldIndex += 1;
      oldLine += 1;
    }
  }

  let currentChange: number | undefined;
  let nextChange = 0;
  return lines.map((line) => {
    if (line.kind === "context") {
      currentChange = undefined;
      return line;
    }
    if (currentChange === undefined) {
      currentChange = nextChange;
      nextChange += 1;
    }
    return { ...line, changeId: currentChange };
  });
}

export function ConfigDiffViewer({
  before,
  after,
}: {
  before: string | null;
  after: string | null;
}) {
  const lines = useMemo(() => buildLineDiff(before, after), [before, after]);
  const changeIds = useMemo(
    () => Array.from(new Set(lines.flatMap((line) => (line.changeId === undefined ? [] : [line.changeId])))),
    [lines],
  );
  const [activeIndex, setActiveIndex] = useState(0);
  const activeChange = changeIds[activeIndex];
  const firstActiveLine = lines.findIndex((line) => line.changeId === activeChange);
  const activeRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setActiveIndex(0);
  }, [before, after]);

  useEffect(() => {
    const element = activeRef.current;
    if (element && typeof element.scrollIntoView === "function") {
      element.scrollIntoView({ block: "nearest" });
    }
  }, [activeChange]);

  const baselineMissing = before === null;
  const fileMissing = after === null;
  const changeLabel = `${changeIds.length} ${changeIds.length === 1 ? "change" : "changes"}`;

  return (
    <section
      aria-label="Configuration diff"
      className="mt-3 overflow-hidden rounded border border-slate-700 bg-slate-950"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-slate-700 bg-slate-900 px-3 py-2">
        <div className="flex items-center gap-3 text-xs">
          <span className="font-medium text-slate-200">Configuration diff</span>
          <span className="text-slate-500" aria-live="polite">
            {changeLabel}
          </span>
          <span className="text-slate-500">
            <span className="text-red-300">− removed</span>
            <span className="ml-2 text-green-300">+ added</span>
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            aria-label="Previous change"
            onClick={() => setActiveIndex((index) => Math.max(0, index - 1))}
            disabled={activeIndex === 0 || changeIds.length === 0}
            className="rounded border border-slate-600 px-2 py-1 text-xs text-slate-300 hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-40"
          >
            Previous
          </button>
          <button
            type="button"
            aria-label="Next change"
            onClick={() =>
              setActiveIndex((index) => Math.min(changeIds.length - 1, index + 1))
            }
            disabled={activeIndex >= changeIds.length - 1 || changeIds.length === 0}
            className="rounded border border-slate-600 px-2 py-1 text-xs text-slate-300 hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-40"
          >
            Next
          </button>
        </div>
      </div>
      {(baselineMissing || fileMissing) && (
        <p className="border-b border-slate-800 px-3 py-2 text-xs text-amber-300">
          {baselineMissing
            ? "No app baseline is available; all current lines are shown as additions."
            : "The config file is missing; all baseline lines are shown as removals."}
        </p>
      )}
      <div className="max-h-96 overflow-auto p-2 font-mono text-[11px] leading-relaxed">
        {lines.length === 0 ? (
          <p className="px-2 py-3 text-slate-500">No content to compare.</p>
        ) : (
          lines.map((line, index) => {
            const active = line.changeId !== undefined && line.changeId === activeChange;
            return (
              <div
                key={`${line.kind}:${line.oldLine ?? ""}:${line.newLine ?? ""}:${index}`}
                ref={active && index === firstActiveLine ? activeRef : undefined}
                data-change-id={line.changeId}
                data-active={active ? "true" : "false"}
                className={`flex min-w-max rounded-sm px-1 ${
                  line.kind === "added"
                    ? "bg-green-950/70 text-green-200"
                    : line.kind === "removed"
                      ? "bg-red-950/70 text-red-200"
                      : "text-slate-400"
                } ${active ? "ring-1 ring-blue-400/80" : ""}`}
              >
                <span className="w-10 shrink-0 select-none text-right text-slate-600">
                  {line.oldLine ?? ""}
                </span>
                <span className="w-10 shrink-0 select-none text-right text-slate-600">
                  {line.newLine ?? ""}
                </span>
                <span className="w-5 shrink-0 select-none text-center">
                  {line.kind === "added" ? "+" : line.kind === "removed" ? "−" : " "}
                </span>
                <code className="whitespace-pre px-1">{line.text || " "}</code>
              </div>
            );
          })
        )}
      </div>
    </section>
  );
}
