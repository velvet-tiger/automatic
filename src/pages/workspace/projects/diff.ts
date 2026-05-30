// Line-level diff utilities for the drift comparison UI.
//
// Extracted verbatim from Projects.tsx as part of a behavior-preserving refactor.

export interface DiffLine {
  type: "same" | "added" | "removed";
  content: string;
  lineNo: { a: number | null; b: number | null };
}

export interface SideBySideDiffRow {
  left: DiffLine | null;
  right: DiffLine | null;
}

/** Compute a simple line-level diff between two text strings.
 *  Uses a greedy longest-common-subsequence approach suitable for config files. */
export function computeLineDiff(expected: string, actual: string): DiffLine[] {
  const aLines = expected.split("\n");
  const bLines = actual.split("\n");

  // Build LCS table
  const m = aLines.length;
  const n = bLines.length;
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = m - 1; i >= 0; i--) {
    for (let j = n - 1; j >= 0; j--) {
      if (aLines[i] === bLines[j]) {
        dp[i]![j] = 1 + dp[i + 1]![j + 1]!;
      } else {
        dp[i]![j] = Math.max(dp[i + 1]![j]!, dp[i]![j + 1]!);
      }
    }
  }

  const result: DiffLine[] = [];
  let i = 0, j = 0;
  let lineA = 1, lineB = 1;

  while (i < m || j < n) {
    if (i < m && j < n && aLines[i] === bLines[j]) {
      result.push({ type: "same", content: aLines[i]!, lineNo: { a: lineA++, b: lineB++ } });
      i++; j++;
    } else if (j < n && (i >= m || dp[i]![j + 1]! >= dp[i + 1]![j]!)) {
      result.push({ type: "added", content: bLines[j]!, lineNo: { a: null, b: lineB++ } });
      j++;
    } else {
      result.push({ type: "removed", content: aLines[i]!, lineNo: { a: lineA++, b: null } });
      i++;
    }
  }

  return result;
}

export function buildSideBySideDiffRows(diffLines: DiffLine[]): SideBySideDiffRow[] {
  const rows: SideBySideDiffRow[] = [];
  let index = 0;

  while (index < diffLines.length) {
    const current = diffLines[index]!;

    if (current.type === "same") {
      rows.push({ left: current, right: current });
      index += 1;
      continue;
    }

    const removed: DiffLine[] = [];
    const added: DiffLine[] = [];

    while (index < diffLines.length && diffLines[index]!.type !== "same") {
      const line = diffLines[index]!;
      if (line.type === "removed") {
        removed.push(line);
      } else if (line.type === "added") {
        added.push(line);
      }
      index += 1;
    }

    const pairCount = Math.max(removed.length, added.length);
    for (let pairIndex = 0; pairIndex < pairCount; pairIndex += 1) {
      rows.push({
        left: removed[pairIndex] ?? null,
        right: added[pairIndex] ?? null,
      });
    }
  }

  return rows;
}
