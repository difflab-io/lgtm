//! Diff parsing + per-line new-file line numbering. Mirrors the SPA's
//! src/lib/diff-lines.ts and the counter in src/components/ReviewLayout.tsx.

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DiffLineKind {
    Added,
    Removed,
    Hunk,
    Header,
    Context,
}

pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
    /// 1-indexed new-file line number, or 0 when the line isn't anchorable
    /// (header, hunk, removed).
    pub new_line: i32,
}

pub fn parse_diff(diff_text: &str) -> Vec<DiffLine> {
    let mut out = Vec::new();
    let mut cursor: i32 = 0;
    for text in diff_text.lines() {
        let kind = classify(text);
        let mut new_line = 0;
        match kind {
            DiffLineKind::Hunk => {
                cursor = parse_hunk_new_start(text).unwrap_or(0);
            }
            DiffLineKind::Header | DiffLineKind::Removed => {}
            DiffLineKind::Added | DiffLineKind::Context => {
                if cursor > 0 {
                    new_line = cursor;
                    cursor += 1;
                }
            }
        }
        out.push(DiffLine { kind, text: text.to_owned(), new_line });
    }
    out
}

fn classify(line: &str) -> DiffLineKind {
    if line.starts_with("diff --git")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
    {
        DiffLineKind::Header
    } else if line.starts_with("@@") {
        DiffLineKind::Hunk
    } else if line.starts_with('+') {
        DiffLineKind::Added
    } else if line.starts_with('-') {
        DiffLineKind::Removed
    } else {
        DiffLineKind::Context
    }
}

fn parse_hunk_new_start(line: &str) -> Option<i32> {
    // `@@ -<old_start>,<old_len> +<new_start>,<new_len> @@ …`
    let plus = line.find('+')?;
    let rest = &line[plus + 1..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}
