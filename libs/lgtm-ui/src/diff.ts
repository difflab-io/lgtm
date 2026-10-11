export type DiffRow = {
  kind: "header" | "hunk" | "added" | "removed" | "context";
  text: string;
  oldLine: number;
  newLine: number;
  side: "LEFT" | "RIGHT";
  line: number;
};

export function diffRows(text: string, source = false): DiffRow[] {
  if (source)
    return text
      .replace(/\n$/, "")
      .split("\n")
      .map((text, i) => ({
        kind: "context",
        text,
        oldLine: 0,
        newLine: i + 1,
        side: "RIGHT",
        line: i + 1,
      }));
  let oldLine = 0,
    newLine = 0,
    inHunk = false;
  return text
    .replace(/\n$/, "")
    .split("\n")
    .map((raw) => {
      const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(raw);
      if (hunk) {
        oldLine = Number(hunk[1]);
        newLine = Number(hunk[2]);
        inHunk = true;
        return {
          kind: "hunk",
          text: raw,
          oldLine: 0,
          newLine: 0,
          side: "RIGHT",
          line: 0,
        };
      }
      if (
        !inHunk ||
        raw.startsWith("\\") ||
        raw.startsWith("diff --git") ||
        raw.startsWith("--- ") ||
        raw.startsWith("+++ ")
      ) {
        if (raw.startsWith("diff --git")) inHunk = false;
        return {
          kind: "header",
          text: raw,
          oldLine: 0,
          newLine: 0,
          side: "RIGHT",
          line: 0,
        };
      }
      if (raw.startsWith("+"))
        return {
          kind: "added",
          text: raw.slice(1),
          oldLine: 0,
          newLine: newLine,
          side: "RIGHT",
          line: newLine++,
        };
      if (raw.startsWith("-"))
        return {
          kind: "removed",
          text: raw.slice(1),
          oldLine: oldLine,
          newLine: 0,
          side: "LEFT",
          line: oldLine++,
        };
      if (raw.startsWith(" "))
        return {
          kind: "context",
          text: raw.slice(1),
          oldLine: oldLine++,
          newLine: newLine,
          side: "RIGHT",
          line: newLine++,
        };
      return {
        kind: "header",
        text: raw,
        oldLine: 0,
        newLine: 0,
        side: "RIGHT",
        line: 0,
      };
    });
}
