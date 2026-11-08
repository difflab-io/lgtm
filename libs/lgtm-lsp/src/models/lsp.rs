use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use url::Url;

/// Lifecycle state of a managed language server session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum LspServerStatus {
    /// The language server process is launching or completing its `initialize` handshake.
    Starting,
    /// The language server is initialized and ready to serve requests.
    Ready,
    /// The language server binary is unavailable or failed to initialize.
    Unavailable,
    /// The language server encountered a runtime protocol error.
    Error,
}

/// Snapshot of a language server's status and aggregate diagnostic counts within a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LspServerStatusEvent {
    /// Canonical root directory of the workspace.
    pub workspace_root: String,
    /// Language identifier (`rust`, `typescript`, `javascript`, `python`, `go`, etc.).
    pub language_id: String,
    /// Canonical project root directory resolved from language root markers.
    pub project_root: String,
    /// Current lifecycle state of the language server.
    pub status: LspServerStatus,
    /// Executable command used to launch the language server.
    pub command: String,
    /// Optional status or failure detail message.
    pub message: Option<String>,
    /// Total number of error-severity diagnostics reported for this language in the workspace.
    pub error_count: usize,
    /// Total number of warning-severity diagnostics reported for this language in the workspace.
    pub warning_count: usize,
}

/// Severity level of an LSP diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum LspDiagnosticSeverity {
    /// Compile or type error (`severity = 1`).
    Error,
    /// Compiler or linter warning (`severity = 2`).
    Warning,
    /// Informational diagnostic (`severity = 3`).
    Info,
    /// Style or refactoring hint (`severity = 4`).
    Hint,
}

/// A single diagnostic normalized to both UTF-16 flat editor offsets and 0-based line/character coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LspDiagnostic {
    /// Start offset in UTF-16 code units from the beginning of the document.
    pub from: u32,
    /// End offset in UTF-16 code units from the beginning of the document.
    pub to: u32,
    /// Zero-based start line number.
    pub line: u32,
    /// Zero-based start UTF-16 character column.
    pub character: u32,
    /// Zero-based end line number.
    pub end_line: u32,
    /// Zero-based end UTF-16 character column.
    pub end_character: u32,
    /// Diagnostic severity level.
    pub severity: LspDiagnosticSeverity,
    /// Diagnostic message text.
    pub message: String,
    /// Optional diagnostic source tool name (for example `rustc`, `tsserver`, `Pyright`).
    pub source: Option<String>,
    /// Optional diagnostic error or rule code.
    pub code: Option<String>,
}

/// Diagnostics published by a language server for a single workspace file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LspFileDiagnosticsEvent {
    /// Canonical root directory of the workspace.
    pub workspace_root: String,
    /// Workspace-relative path of the file.
    pub path: String,
    /// Language identifier that produced the diagnostics.
    pub language_id: String,
    /// Current list of diagnostics for the file.
    pub diagnostics: Vec<LspDiagnostic>,
}

/// Hover documentation and optional highlight range returned for a document offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LspHoverResult {
    /// Formatted Markdown or plain-text hover contents.
    pub contents: String,
    /// Optional UTF-16 start offset of the hovered symbol.
    pub from: Option<u32>,
    /// Optional UTF-16 end offset of the hovered symbol.
    pub to: Option<u32>,
}

/// A target source location returned by `textDocument/definition`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LspLocation {
    /// Workspace-relative path of the target file.
    pub path: String,
    /// UTF-16 start offset of the target symbol.
    pub offset: u32,
    /// UTF-16 end offset of the target symbol.
    pub end_offset: u32,
    /// Zero-based start line number.
    pub line: u32,
    /// Zero-based start UTF-16 character column.
    pub character: u32,
}

/// Complete snapshot of all active language servers and file diagnostics for a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LspWorkspaceSnapshot {
    /// Canonical root directory of the workspace.
    pub workspace_root: String,
    /// Status snapshots for all tracked language servers in the workspace.
    pub servers: Vec<LspServerStatusEvent>,
    /// Non-empty diagnostic lists keyed by workspace-relative file path.
    pub diagnostics_by_path: BTreeMap<String, Vec<LspDiagnostic>>,
}

/// Read-only formatted text buffer summarizing workspace LSP servers and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LspDiagnosticsBuffer {
    /// Display title for the read-only buffer tab.
    pub title: String,
    /// Optional language filter applied when rendering the buffer.
    pub language_id: Option<String>,
    /// Formatted Markdown/plain-text diagnostics report.
    pub content: String,
}

/// Asynchronous event emitted by `LspService` when server status or file diagnostics change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LspEvent {
    /// A language server changed status or updated its aggregate diagnostic counts.
    Status(LspServerStatusEvent),
    /// A language server published updated diagnostics for a file.
    Diagnostics(LspFileDiagnosticsEvent),
}

impl LspDiagnosticsBuffer {
    /// Renders a formatted diagnostics buffer for `workspace_root`, optionally filtered by `language_filter`.
    pub fn render(
        workspace_root: &Path,
        language_filter: Option<&str>,
        servers: &[LspServerStatusEvent],
        diagnostics_by_path: &BTreeMap<String, (String, Vec<LspDiagnostic>)>,
    ) -> Self {
        render_diagnostics_buffer(
            workspace_root,
            language_filter,
            servers,
            diagnostics_by_path,
        )
    }
}

// Private ---------------------------------------------------------------------

/// Converts a flat UTF-16 code unit offset in `text` into a 0-based `(line, character)` LSP position.
pub(crate) fn offset_to_position(text: &str, utf16_offset: u32) -> (u32, u32) {
    let mut current_utf16: u32 = 0;
    let mut line: u32 = 0;
    let mut character: u32 = 0;

    for ch in text.chars() {
        if current_utf16 >= utf16_offset {
            break;
        }
        let units = ch.len_utf16() as u32;
        if ch == '\n' {
            line += 1;
            character = 0;
        } else if ch != '\r' {
            character += units;
        }
        current_utf16 += units;
    }

    (line, character)
}

/// Converts a 0-based `(target_line, target_character)` LSP position in `text` into a flat UTF-16 offset.
pub(crate) fn position_to_offset(text: &str, target_line: u32, target_character: u32) -> u32 {
    let mut current_utf16: u32 = 0;
    let mut line: u32 = 0;
    let mut character: u32 = 0;

    for ch in text.chars() {
        if line == target_line && character >= target_character {
            return current_utf16;
        }
        if line > target_line {
            return current_utf16;
        }
        let units = ch.len_utf16() as u32;
        if ch == '\n' {
            if line == target_line {
                return current_utf16;
            }
            line += 1;
            character = 0;
        } else if ch != '\r' {
            character += units;
        }
        current_utf16 += units;
    }

    current_utf16
}

/// Converts an absolute filesystem path into a `file://` URI string.
pub(crate) fn path_to_file_uri(path: &Path) -> Option<String> {
    Url::from_file_path(path).ok().map(|url| url.to_string())
}

/// Parses a `file://` URI string back into an absolute filesystem path.
pub(crate) fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let parsed = Url::parse(uri).ok()?;
    if parsed.scheme() != "file" {
        return None;
    }
    parsed.to_file_path().ok()
}

/// Renders a plain-text/Markdown buffer listing active servers and file diagnostics.
pub(crate) fn render_diagnostics_buffer(
    workspace_root: &Path,
    language_filter: Option<&str>,
    servers: &[LspServerStatusEvent],
    diagnostics_by_path: &BTreeMap<String, (String, Vec<LspDiagnostic>)>,
) -> LspDiagnosticsBuffer {
    let title = match language_filter {
        Some(lang) => format!("LSP Diagnostics ({lang})"),
        None => "LSP Diagnostics".to_string(),
    };

    let mut lines = Vec::new();
    lines.push(format!("# {title}"));
    lines.push(format!("Workspace: {}", workspace_root.display()));
    lines.push(String::new());

    lines.push("## Servers".to_string());
    let filtered_servers: Vec<&LspServerStatusEvent> = servers
        .iter()
        .filter(|srv| language_filter.is_none_or(|lang| srv.language_id == lang))
        .collect();

    if filtered_servers.is_empty() {
        lines.push("No active language servers.".to_string());
    } else {
        for srv in filtered_servers {
            let status_label = match srv.status {
                LspServerStatus::Starting => "starting",
                LspServerStatus::Ready => "ready",
                LspServerStatus::Unavailable => "unavailable",
                LspServerStatus::Error => "error",
            };
            let detail = srv
                .message
                .as_deref()
                .map(|msg| format!(" - {msg}"))
                .unwrap_or_default();
            lines.push(format!(
                "- {} [{}] (cmd: `{}`, root: `{}`, {} error(s), {} warning(s)){detail}",
                srv.language_id,
                status_label,
                srv.command,
                srv.project_root,
                srv.error_count,
                srv.warning_count
            ));
        }
    }

    lines.push(String::new());
    lines.push("## Diagnostics".to_string());

    let mut matching_entries = 0usize;
    for (path, (lang_id, diags)) in diagnostics_by_path {
        if language_filter.is_some_and(|filter| filter != lang_id) {
            continue;
        }
        if diags.is_empty() {
            continue;
        }
        let rel_path = Path::new(path)
            .strip_prefix(workspace_root)
            .map(|rel| rel.display().to_string())
            .unwrap_or_else(|_| {
                if Path::new(path).ends_with(".lgtm/config.json") {
                    "~/.lgtm/config.json".to_string()
                } else {
                    path.clone()
                }
            });
        for diag in diags {
            matching_entries += 1;
            let severity_label = match diag.severity {
                LspDiagnosticSeverity::Error => "ERROR",
                LspDiagnosticSeverity::Warning => "WARN",
                LspDiagnosticSeverity::Info => "INFO",
                LspDiagnosticSeverity::Hint => "HINT",
            };
            let code_suffix = diag
                .code
                .as_deref()
                .map(|code| format!(" [{code}]"))
                .unwrap_or_default();
            let source_prefix = diag
                .source
                .as_deref()
                .map(|src| format!("({src}) "))
                .unwrap_or_default();
            lines.push(format!(
                "{}:{}:{} [{}]{} {}{}",
                rel_path,
                diag.line + 1,
                diag.character + 1,
                severity_label,
                code_suffix,
                source_prefix,
                diag.message
            ));
        }
    }

    if matching_entries == 0 {
        lines.push("No diagnostics reported.".to_string());
    }

    lines.push(String::new());
    LspDiagnosticsBuffer {
        title,
        language_id: language_filter.map(str::to_string),
        content: lines.join("\n"),
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_utf8_and_utf16_source_text {
        use super::*;

        mod when_converting_between_utf16_offsets_and_lsp_positions {
            use super::*;

            #[test]
            fn should_round_trip_ascii_and_surrogate_pair_coordinates() {
                // Arrange
                let text =
                    "fn main() {\n    let rocket = \"🚀\";\n    println!(\"{}\", rocket);\n}\n";
                // "🚀" is 2 UTF-16 code units on line 1.
                let line1_start = position_to_offset(text, 1, 0);
                let after_rocket = position_to_offset(text, 1, 20);

                // Act
                let (resolved_line, resolved_char) = offset_to_position(text, after_rocket);
                let round_trip = position_to_offset(text, resolved_line, resolved_char);

                // Assert
                assert_eq!(line1_start, 12);
                assert_eq!((resolved_line, resolved_char), (1, 20));
                assert_eq!(round_trip, after_rocket);
            }
        }
    }

    mod given_an_absolute_filesystem_path {
        use super::*;

        mod when_converting_to_and_from_file_uri {
            use super::*;

            #[test]
            fn should_preserve_canonical_path() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let file_path = temp.path().join("lib.rs");
                std::fs::write(&file_path, "pub fn demo() {}\n").unwrap();
                let canonical = std::fs::canonicalize(&file_path).unwrap();

                // Act
                let uri = path_to_file_uri(&canonical).unwrap();
                let round_trip = file_uri_to_path(&uri).unwrap();

                // Assert
                assert!(uri.starts_with("file://"));
                assert_eq!(round_trip, canonical);
            }
        }
    }

    mod given_workspace_servers_and_diagnostics {
        use super::*;

        mod when_rendering_diagnostics_buffer {
            use super::*;

            #[test]
            fn should_format_servers_and_file_diagnostics_as_plain_text() {
                // Arrange
                let workspace_root = Path::new("/tmp/workspace");
                let servers = vec![LspServerStatusEvent {
                    workspace_root: "/tmp/workspace".to_string(),
                    language_id: "rust".to_string(),
                    project_root: "/tmp/workspace".to_string(),
                    status: LspServerStatus::Ready,
                    command: "rust-analyzer".to_string(),
                    message: None,
                    error_count: 1,
                    warning_count: 0,
                }];
                let mut diags = BTreeMap::new();
                diags.insert(
                    "/tmp/workspace/src/lib.rs".to_string(),
                    (
                        "rust".to_string(),
                        vec![LspDiagnostic {
                            from: 10,
                            to: 16,
                            line: 1,
                            character: 4,
                            end_line: 1,
                            end_character: 10,
                            severity: LspDiagnosticSeverity::Error,
                            message: "cannot find value `broken` in this scope".to_string(),
                            source: Some("rustc".to_string()),
                            code: Some("E0425".to_string()),
                        }],
                    ),
                );

                // Act
                let buffer =
                    render_diagnostics_buffer(workspace_root, Some("rust"), &servers, &diags);

                // Assert
                assert_eq!(buffer.title, "LSP Diagnostics (rust)");
                assert!(buffer.content.contains("rust [ready]"));
                assert!(
                    buffer.content.contains(
                        "src/lib.rs:2:5 [ERROR] [E0425] (rustc) cannot find value `broken` in this scope"
                    )
                );
            }
        }
    }
}
