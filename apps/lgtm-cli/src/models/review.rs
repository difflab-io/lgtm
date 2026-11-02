//! View models for `lgtm review` commands, sessions, diff hunks, and thread rendering.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::Context;
use serde_json::{Value, json};

use crate::{
    backend::proto::lgtm::v1::{
        CloseReviewResponse, CreateReviewResponse, DiffHunkDetail, DiffLineDetail,
        ListReviewsResponse, RefreshReviewResponse, ReviewSummary, SessionSummary,
        ShowReviewResponse,
    },
    models::{
        comment::CommentView,
        common::{
            AnchorView, IdentityView, ReviewGroupBy, ReviewOutcome, format_diff_line_kind,
            format_review_status, format_section_banner, format_session_outcome,
            format_session_state, format_sub_banner, short_sha,
        },
    },
};

/// View model for a review summary header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewSummaryView {
    pub id: String,
    pub number: u64,
    pub name: String,
    pub local: bool,
    pub git_dir: String,
    pub branch: String,
    pub repository_url: Option<String>,
    pub pr_number: Option<u64>,
    pub forge_id: Option<String>,
    pub status: &'static str,
    pub base_sha: String,
    pub head_sha: String,
    pub reviewed_files: BTreeMap<String, String>,
}

impl ReviewSummaryView {
    // Public ------------------------------------------------------------------

    /// Return `"local"` or `"remote"` scope label.
    pub fn scope_label(&self) -> &'static str {
        if self.local { "local" } else { "remote" }
    }

    /// Serialize this review summary to its JSON contract.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "number": self.number,
            "name": self.name,
            "local": self.local,
            "git_dir": self.git_dir,
            "branch": self.branch,
            "repository_url": self.repository_url,
            "pr_number": self.pr_number,
            "forge_id": self.forge_id,
            "status": self.status,
            "base_sha": self.base_sha,
            "head_sha": self.head_sha,
            "reviewed_files": self.reviewed_files,
        })
    }
}

impl From<&ReviewSummary> for ReviewSummaryView {
    fn from(review: &ReviewSummary) -> Self {
        Self {
            id: review.id.clone(),
            number: review.number,
            name: review.name.clone(),
            local: review.local,
            git_dir: review.git_dir.clone(),
            branch: review.branch.clone(),
            repository_url: review.repository_url.clone(),
            pr_number: review.pr_number,
            forge_id: review.forge_id.clone(),
            status: format_review_status(review.status.as_known()),
            base_sha: review.base_sha.clone(),
            head_sha: review.head_sha.clone(),
            reviewed_files: review
                .reviewed_files
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }
    }
}

/// View model for a review session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSummaryView {
    pub id: String,
    pub review_id: String,
    pub number: u64,
    pub author: Option<IdentityView>,
    pub state: &'static str,
    pub outcome: Option<&'static str>,
    pub body: Option<String>,
    pub forge_id: Option<String>,
    pub published_at: Option<String>,
}

impl SessionSummaryView {
    // Public ------------------------------------------------------------------

    /// Return the display label for the session author.
    pub fn author_label(&self) -> String {
        self.author
            .as_ref()
            .map_or_else(|| "unknown".to_owned(), IdentityView::display_label)
    }

    /// Serialize this session summary to its JSON contract.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "review_id": self.review_id,
            "number": self.number,
            "author": self.author.as_ref().map(IdentityView::to_json),
            "state": self.state,
            "outcome": self.outcome,
            "body": self.body,
            "forge_id": self.forge_id,
            "published_at": self.published_at,
        })
    }
}

impl From<&SessionSummary> for SessionSummaryView {
    fn from(session: &SessionSummary) -> Self {
        Self {
            id: session.id.clone(),
            review_id: session.review_id.clone(),
            number: session.number,
            author: session.author.as_option().map(IdentityView::from),
            state: format_session_state(session.state.as_known()),
            outcome: format_session_outcome(session.outcome.as_known()),
            body: session.body.clone(),
            forge_id: session.forge_id.clone(),
            published_at: session.published_at.clone(),
        }
    }
}

/// View model for a single line within a diff hunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLineView {
    pub line_number: u32,
    pub kind: &'static str,
    pub marker: char,
    pub content: String,
}

impl DiffLineView {
    // Public ------------------------------------------------------------------

    /// Serialize this diff line to its JSON contract.
    pub fn to_json(&self) -> Value {
        json!({
            "kind": self.kind,
            "line_number": self.line_number,
            "content": self.content,
        })
    }
}

impl From<&DiffLineDetail> for DiffLineView {
    fn from(line: &DiffLineDetail) -> Self {
        let (kind, marker) = format_diff_line_kind(line.kind.as_known());
        Self {
            line_number: line.line_number,
            kind,
            marker,
            content: line.content.clone(),
        }
    }
}

/// View model for a diff hunk anchored to a file and line range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunkView {
    pub file_path: String,
    pub commit_sha: String,
    pub start_line: u32,
    pub lines: Vec<DiffLineView>,
}

impl DiffHunkView {
    // Public ------------------------------------------------------------------

    /// Render the numbered diff lines for terminal display.
    pub fn render_lines(&self) -> String {
        self.lines
            .iter()
            .map(|line| {
                format!(
                    " {} {:>2} │ {}",
                    line.marker, line.line_number, line.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Serialize this diff hunk to its JSON contract.
    pub fn to_json(&self) -> Value {
        let lines: Vec<Value> = self.lines.iter().map(DiffLineView::to_json).collect();
        json!({
            "file_path": self.file_path,
            "commit_sha": self.commit_sha,
            "start_line": self.start_line,
            "lines": lines,
        })
    }
}

impl From<&DiffHunkDetail> for DiffHunkView {
    fn from(hunk: &DiffHunkDetail) -> Self {
        Self {
            file_path: hunk.file_path.clone(),
            commit_sha: hunk.commit_sha.clone(),
            start_line: hunk.start_line,
            lines: hunk.lines.iter().map(DiffLineView::from).collect(),
        }
    }
}

/// View model for `lgtm review list`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewListView {
    pub reviews: Vec<ReviewSummaryView>,
}

impl ReviewListView {
    // Public ------------------------------------------------------------------

    /// Print the review list in terminal or `--json` format.
    pub fn print(&self, json: bool) {
        if json {
            let reviews: Vec<Value> = self
                .reviews
                .iter()
                .map(ReviewSummaryView::to_json)
                .collect();
            println!("{}", json!({ "reviews": reviews }));
        } else if self.reviews.is_empty() {
            println!("No reviews found.");
        } else {
            for review in &self.reviews {
                println!(
                    "Review #{}  {}  [{}]  {}  ({}..{})",
                    review.number,
                    review.name,
                    review.scope_label(),
                    review.status,
                    short_sha(&review.base_sha),
                    short_sha(&review.head_sha),
                );
            }
        }
    }
}

impl From<ListReviewsResponse> for ReviewListView {
    fn from(response: ListReviewsResponse) -> Self {
        Self {
            reviews: response
                .reviews
                .iter()
                .map(ReviewSummaryView::from)
                .collect(),
        }
    }
}

/// View model for `lgtm review new` and `lgtm review close`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewSessionView {
    pub review: ReviewSummaryView,
    pub session: SessionSummaryView,
}

impl ReviewSessionView {
    // Public ------------------------------------------------------------------

    /// Print a newly created review and draft session in terminal or `--json` format.
    pub fn print_created(&self, json: bool) {
        if json {
            println!(
                "{}",
                json!({
                    "review": self.review.to_json(),
                    "session": self.session.to_json(),
                })
            );
        } else {
            println!(
                "Created review #{}  {}  [{}]  (session #{} {})",
                self.review.number,
                self.review.name,
                self.review.scope_label(),
                self.session.number,
                self.session.state,
            );
        }
    }

    /// Print a closed review session in terminal or `--json` format.
    pub fn print_closed(&self, outcome: ReviewOutcome, json: bool) {
        if json {
            println!(
                "{}",
                json!({
                    "review": self.review.to_json(),
                    "session": self.session.to_json(),
                })
            );
        } else {
            println!(
                "Closed review #{}  {}  with {} (session #{} {})",
                self.review.number,
                self.review.name,
                outcome.as_str(),
                self.session.number,
                self.session.state,
            );
        }
    }
}

impl TryFrom<CreateReviewResponse> for ReviewSessionView {
    type Error = anyhow::Error;

    fn try_from(response: CreateReviewResponse) -> anyhow::Result<Self> {
        let review = response
            .review
            .as_option()
            .context("CreateReview response missing review")?;
        let session = response
            .session
            .as_option()
            .context("CreateReview response missing session")?;
        Ok(Self {
            review: ReviewSummaryView::from(review),
            session: SessionSummaryView::from(session),
        })
    }
}

impl TryFrom<CloseReviewResponse> for ReviewSessionView {
    type Error = anyhow::Error;

    fn try_from(response: CloseReviewResponse) -> anyhow::Result<Self> {
        let review = response
            .review
            .as_option()
            .context("CloseReview response missing review")?;
        let session = response
            .session
            .as_option()
            .context("CloseReview response missing session")?;
        Ok(Self {
            review: ReviewSummaryView::from(review),
            session: SessionSummaryView::from(session),
        })
    }
}

/// View model for `lgtm review refresh`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewRefreshView {
    pub review: ReviewSummaryView,
    pub updated_anchors: u32,
}

impl ReviewRefreshView {
    // Public ------------------------------------------------------------------

    /// Print the refreshed review summary in terminal or `--json` format.
    pub fn print(&self, json: bool) {
        if json {
            println!(
                "{}",
                json!({
                    "review": self.review.to_json(),
                    "updated_anchors": self.updated_anchors,
                })
            );
        } else {
            println!(
                "Refreshed review #{}  {}  [{}]  Head: {}  ({} anchor(s) updated)",
                self.review.number,
                self.review.name,
                self.review.scope_label(),
                short_sha(&self.review.head_sha),
                self.updated_anchors,
            );
        }
    }
}

impl TryFrom<RefreshReviewResponse> for ReviewRefreshView {
    type Error = anyhow::Error;

    fn try_from(response: RefreshReviewResponse) -> anyhow::Result<Self> {
        let review = response
            .review
            .as_option()
            .context("RefreshReview response missing review")?;
        Ok(Self {
            review: ReviewSummaryView::from(review),
            updated_anchors: response.updated_anchors,
        })
    }
}

/// View model for `lgtm review show`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewDetailView {
    pub review: ReviewSummaryView,
    pub sessions: Vec<SessionSummaryView>,
    pub comments: Vec<CommentView>,
    pub diff_hunks: Vec<DiffHunkView>,
}

impl ReviewDetailView {
    // Public ------------------------------------------------------------------

    /// Print the full review detail in terminal or `--json` format.
    pub fn print(&self, group_by: ReviewGroupBy, unresolved: bool, json: bool) {
        if json {
            let sessions: Vec<Value> = self
                .sessions
                .iter()
                .map(SessionSummaryView::to_json)
                .collect();
            let comments: Vec<Value> = self.comments.iter().map(CommentView::to_json).collect();
            let diff_hunks: Vec<Value> =
                self.diff_hunks.iter().map(DiffHunkView::to_json).collect();
            println!(
                "{}",
                json!({
                    "review": self.review.to_json(),
                    "sessions": sessions,
                    "comments": comments,
                    "diff_hunks": diff_hunks,
                })
            );
        } else {
            print!("{}", self.render(group_by, unresolved));
        }
    }

    /// Render the human-readable terminal view for `lgtm review show`.
    pub fn render(&self, group_by: ReviewGroupBy, unresolved: bool) -> String {
        let mut out = String::new();
        let scope = self.review.scope_label();
        let status = self.review.status;
        let unresolved_roots = self
            .comments
            .iter()
            .filter(|c| c.reply_to_number.is_none() && !c.resolved)
            .count();
        let unresolved_suffix = if unresolved {
            format!(
                " ({unresolved_roots} unresolved {})",
                if unresolved_roots == 1 {
                    "thread"
                } else {
                    "threads"
                }
            )
        } else {
            String::new()
        };

        out.push_str(&format!(
            "Review #{}  {}  [{scope}]  {status}{unresolved_suffix}\n",
            self.review.number, self.review.name
        ));
        let head_suffix = if self.review.local { " (snapshot)" } else { "" };
        out.push_str(&format!(
            "Base: {} -> Head: {}{head_suffix}\n",
            short_sha(&self.review.base_sha),
            short_sha(&self.review.head_sha),
        ));

        let session_by_id: HashMap<&str, &SessionSummaryView> =
            self.sessions.iter().map(|s| (s.id.as_str(), s)).collect();
        let comment_by_num: HashMap<u64, &CommentView> =
            self.comments.iter().map(|c| (c.number, c)).collect();
        let hunk_by_root_num: HashMap<u64, &DiffHunkView> = self
            .comments
            .iter()
            .filter(|c| c.reply_to_number.is_none() && c.anchor.is_some())
            .zip(self.diff_hunks.iter())
            .map(|(c, h)| (c.number, h))
            .collect();

        match group_by {
            ReviewGroupBy::File => {
                self.render_by_file(&mut out, &session_by_id, &hunk_by_root_num);
            }
            ReviewGroupBy::Session => {
                self.render_by_session(&mut out, &comment_by_num, &hunk_by_root_num);
            }
        }

        out
    }

    // Private -----------------------------------------------------------------

    fn render_by_file(
        &self,
        out: &mut String,
        session_by_id: &HashMap<&str, &SessionSummaryView>,
        hunk_by_root_num: &HashMap<u64, &DiffHunkView>,
    ) {
        let roots: Vec<&CommentView> = self
            .comments
            .iter()
            .filter(|c| c.reply_to_number.is_none())
            .collect();
        let mut replies_by_root: HashMap<u64, Vec<&CommentView>> = HashMap::new();
        for comment in &self.comments {
            if let Some(root_num) = comment.reply_to_number {
                replies_by_root.entry(root_num).or_default().push(comment);
            }
        }

        let mut current_file: Option<&str> = None;
        for root in roots {
            let file_label = root
                .anchor
                .as_ref()
                .map(|a| a.file_path.as_str())
                .unwrap_or("General");
            if current_file != Some(file_label) {
                current_file = Some(file_label);
                out.push('\n');
                out.push_str(&format_section_banner(file_label));
                out.push('\n');
            }

            out.push('\n');
            if let Some(anchor) = root.anchor.as_ref() {
                out.push_str(&Self::format_file_anchor_header(anchor, root));
                out.push('\n');
                if let Some(hunk) = hunk_by_root_num
                    .get(&root.number)
                    .copied()
                    .or_else(|| self.find_matching_hunk(anchor))
                {
                    out.push_str(&hunk.render_lines());
                    out.push('\n');
                }
            } else {
                let res_str = if root.resolved {
                    "resolved"
                } else {
                    "unresolved"
                };
                out.push_str(&format_sub_banner(&format!("general • {res_str}")));
                out.push('\n');
            }

            out.push_str(&Self::format_comment_in_file_group(root, session_by_id));
            if let Some(replies) = replies_by_root.get(&root.number) {
                for reply in replies {
                    out.push_str(&Self::format_reply_in_file_group(reply, session_by_id));
                }
            }
        }
    }

    fn render_by_session(
        &self,
        out: &mut String,
        comment_by_num: &HashMap<u64, &CommentView>,
        hunk_by_root_num: &HashMap<u64, &DiffHunkView>,
    ) {
        for session in &self.sessions {
            let session_comments: Vec<&CommentView> = self
                .comments
                .iter()
                .filter(|c| c.session_id.as_deref() == Some(session.id.as_str()))
                .collect();
            if session_comments.is_empty() && session.body.as_deref().is_none_or(str::is_empty) {
                continue;
            }

            let author_label = session.author_label();
            let outcome_label = session.outcome.unwrap_or("comment");
            let header_title = format!(
                "Session #{} • {author_label} • {outcome_label} • {}",
                session.number, session.state
            );
            out.push('\n');
            out.push_str(&format_section_banner(&header_title));
            out.push('\n');

            if let Some(body) = session.body.as_deref().filter(|b| !b.trim().is_empty()) {
                out.push_str(&format!("  {}\n", body.trim()));
            }

            self.render_session_or_direct_group(
                out,
                &session_comments,
                comment_by_num,
                hunk_by_root_num,
            );
        }

        let direct_comments: Vec<&CommentView> = self
            .comments
            .iter()
            .filter(|c| c.session_id.is_none())
            .collect();
        if !direct_comments.is_empty() {
            out.push('\n');
            out.push_str(&format_section_banner("Direct Comments"));
            out.push('\n');

            self.render_session_or_direct_group(
                out,
                &direct_comments,
                comment_by_num,
                hunk_by_root_num,
            );
        }
    }

    fn render_session_or_direct_group(
        &self,
        out: &mut String,
        group_comments: &[&CommentView],
        comment_by_num: &HashMap<u64, &CommentView>,
        hunk_by_root_num: &HashMap<u64, &DiffHunkView>,
    ) {
        let root_nums_in_group: HashSet<u64> = group_comments
            .iter()
            .filter(|c| c.reply_to_number.is_none())
            .map(|c| c.number)
            .collect();
        let mut replies_in_group_by_root: HashMap<u64, Vec<&CommentView>> = HashMap::new();
        for comment in group_comments {
            if let Some(root_num) = comment.reply_to_number {
                replies_in_group_by_root
                    .entry(root_num)
                    .or_default()
                    .push(comment);
            }
        }

        for comment in group_comments {
            if let Some(reply_num) = comment.reply_to_number {
                if root_nums_in_group.contains(&reply_num) {
                    continue;
                }
                out.push('\n');
                let effective_root = comment_by_num.get(&reply_num).copied().unwrap_or(comment);
                if let Some(anchor) = effective_root.anchor.as_ref() {
                    out.push_str(&Self::format_session_anchor_header(anchor, effective_root));
                    out.push('\n');
                }
                if let Some(parent) = comment_by_num.get(&reply_num) {
                    out.push_str(&format!("  {}\n", parent.author_label()));
                    for line in parent.body.lines() {
                        out.push_str(&format!("    {line}\n"));
                    }
                }
                out.push_str(&format!("    ↳ {}\n", comment.author_label()));
                for line in comment.body.lines() {
                    out.push_str(&format!("      {line}\n"));
                }
                continue;
            }

            out.push('\n');
            if let Some(anchor) = comment.anchor.as_ref() {
                out.push_str(&Self::format_session_anchor_header(anchor, comment));
                out.push('\n');
                if let Some(hunk) = hunk_by_root_num
                    .get(&comment.number)
                    .copied()
                    .or_else(|| self.find_matching_hunk(anchor))
                {
                    out.push_str(&hunk.render_lines());
                    out.push('\n');
                }
            }
            out.push_str(&format!("  {}\n", comment.author_label()));
            for line in comment.body.lines() {
                out.push_str(&format!("    {line}\n"));
            }
            if let Some(replies) = replies_in_group_by_root.get(&comment.number) {
                for reply in replies {
                    out.push_str(&format!("    ↳ {}\n", reply.author_label()));
                    for line in reply.body.lines() {
                        out.push_str(&format!("      {line}\n"));
                    }
                }
            }
        }
    }

    fn find_matching_hunk(&self, anchor: &AnchorView) -> Option<&DiffHunkView> {
        self.diff_hunks.iter().find(|h| {
            h.file_path == anchor.file_path
                && h.lines.iter().any(|l| {
                    l.line_number == anchor.line || Some(l.line_number) == anchor.start_line
                })
        })
    }

    fn format_file_anchor_header(anchor: &AnchorView, root: &CommentView) -> String {
        let range = anchor.line_range_label();
        let status = if root.resolved {
            "resolved"
        } else {
            "unresolved"
        };
        let outdated = if root.outdated { " • outdated" } else { "" };
        format_sub_banner(&format!("{range} • {status}{outdated}"))
    }

    fn format_session_anchor_header(anchor: &AnchorView, root: &CommentView) -> String {
        let file_line = anchor.file_line_label();
        let status = if root.resolved {
            "resolved"
        } else {
            "unresolved"
        };
        let outdated = if root.outdated { " • outdated" } else { "" };
        format_sub_banner(&format!("{file_line} • {status}{outdated}"))
    }

    fn format_comment_in_file_group(
        comment: &CommentView,
        session_by_id: &HashMap<&str, &SessionSummaryView>,
    ) -> String {
        let author_label = comment.author_label();
        let session_info = comment
            .session_id
            .as_deref()
            .and_then(|id| session_by_id.get(id))
            .map(|s| format!(" • session #{} ({})", s.number, s.state))
            .unwrap_or_default();
        let mut out = format!("  {author_label}{session_info}\n");
        for line in comment.body.lines() {
            out.push_str(&format!("    {line}\n"));
        }
        out
    }

    fn format_reply_in_file_group(
        reply: &CommentView,
        session_by_id: &HashMap<&str, &SessionSummaryView>,
    ) -> String {
        let author_label = reply.author_label();
        let session_info = reply
            .session_id
            .as_deref()
            .and_then(|id| session_by_id.get(id))
            .map(|s| format!(" • session #{} ({})", s.number, s.state))
            .unwrap_or_default();
        let mut out = format!("    ↳ {author_label}{session_info}\n");
        for line in reply.body.lines() {
            out.push_str(&format!("      {line}\n"));
        }
        out
    }
}

impl TryFrom<ShowReviewResponse> for ReviewDetailView {
    type Error = anyhow::Error;

    fn try_from(response: ShowReviewResponse) -> anyhow::Result<Self> {
        let review = response
            .review
            .as_option()
            .context("ShowReview response missing review")?;
        Ok(Self {
            review: ReviewSummaryView::from(review),
            sessions: response
                .sessions
                .iter()
                .map(SessionSummaryView::from)
                .collect(),
            comments: response.comments.iter().map(CommentView::from).collect(),
            diff_hunks: response.diff_hunks.iter().map(DiffHunkView::from).collect(),
        })
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // fixtures ----------------------------------------------------------------

    fn sample_detail_view() -> ReviewDetailView {
        ReviewDetailView {
            review: ReviewSummaryView {
                id: "rev-1".into(),
                number: 1,
                name: "feat/auth".into(),
                local: true,
                git_dir: "/tmp/repo/.git".into(),
                branch: "feat/auth".into(),
                repository_url: None,
                pr_number: None,
                forge_id: None,
                status: "open",
                base_sha: "11111112222222".into(),
                head_sha: "33333334444444".into(),
                reviewed_files: BTreeMap::new(),
            },
            sessions: vec![SessionSummaryView {
                id: "sess-1".into(),
                review_id: "rev-1".into(),
                number: 1,
                author: Some(IdentityView {
                    id: "id-1".into(),
                    name: "claude-opus-4-6".into(),
                    kind: "agent".into(),
                    email: None,
                    forge_login: None,
                    provider: Some("anthropic".into()),
                    model: Some("claude-opus-4-6".into()),
                }),
                state: "published",
                outcome: Some("request"),
                body: Some("Needs error guard.".into()),
                forge_id: None,
                published_at: None,
            }],
            comments: vec![
                CommentView {
                    id: "c-1".into(),
                    number: 1,
                    review_id: "rev-1".into(),
                    session_id: Some("sess-1".into()),
                    author: Some(IdentityView {
                        id: "id-1".into(),
                        name: "claude-opus-4-6".into(),
                        kind: "agent".into(),
                        email: None,
                        forge_login: None,
                        provider: Some("anthropic".into()),
                        model: Some("claude-opus-4-6".into()),
                    }),
                    reply_to_number: None,
                    anchor: Some(AnchorView {
                        file_path: "src/lib.rs".into(),
                        start_line: None,
                        line: 2,
                        commit_sha: "33333334444444".into(),
                        original_start_line: None,
                        original_line: 2,
                        original_commit_sha: "33333334444444".into(),
                    }),
                    body: "Validate token expiry before decoding.".into(),
                    resolved: false,
                    outdated: false,
                    forge_id: None,
                },
                CommentView {
                    id: "c-2".into(),
                    number: 2,
                    review_id: "rev-1".into(),
                    session_id: None,
                    author: Some(IdentityView {
                        id: "id-2".into(),
                        name: "Ada Lovelace".into(),
                        kind: "human".into(),
                        email: Some("ada@example.com".into()),
                        forge_login: None,
                        provider: None,
                        model: None,
                    }),
                    reply_to_number: Some(1),
                    anchor: None,
                    body: "Added expiry validation in the token decoder.".into(),
                    resolved: false,
                    outdated: false,
                    forge_id: None,
                },
            ],
            diff_hunks: vec![DiffHunkView {
                file_path: "src/lib.rs".into(),
                commit_sha: "33333334444444".into(),
                start_line: 1,
                lines: vec![
                    DiffLineView {
                        line_number: 1,
                        kind: "context",
                        marker: ' ',
                        content: "pub fn verify() {".into(),
                    },
                    DiffLineView {
                        line_number: 2,
                        kind: "added",
                        marker: '+',
                        content: "    check_expiry();".into(),
                    },
                ],
            }],
        }
    }

    // tests -------------------------------------------------------------------

    mod given_review_detail_view {
        use super::*;

        mod when_rendering_by_file {
            use super::*;

            #[test]
            fn should_render_file_banner_hunk_and_nested_reply() {
                let view = sample_detail_view();
                let rendered = view.render(ReviewGroupBy::File, true);

                assert!(
                    rendered.contains("Review #1  feat/auth  [local]  open (1 unresolved thread)")
                );
                assert!(rendered.contains("══ src/lib.rs "));
                assert!(rendered.contains("── line 2 • unresolved "));
                assert!(rendered.contains(" +  2 │     check_expiry();"));
                assert!(rendered.contains("  claude-opus-4-6 (agent) • session #1 (published)"));
                assert!(rendered.contains("    ↳ Ada Lovelace"));
                assert!(rendered.contains("      Added expiry validation in the token decoder."));
            }
        }

        mod when_rendering_by_session {
            use super::*;

            #[test]
            fn should_render_session_section_and_direct_comments_section() {
                let view = sample_detail_view();
                let rendered = view.render(ReviewGroupBy::Session, false);

                assert!(
                    rendered
                        .contains("══ Session #1 • claude-opus-4-6 (agent) • request • published ")
                );
                assert!(rendered.contains("  Needs error guard."));
                assert!(rendered.contains("── src/lib.rs:2 • unresolved "));
                assert!(rendered.contains("══ Direct Comments "));
                assert!(rendered.contains("    ↳ Ada Lovelace"));
            }
        }
    }
}
