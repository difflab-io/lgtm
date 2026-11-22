//! Parser for OSC 7501 (Program Status Protocol) records emitted by coding agents and CLIs.

use crate::models::status::TerminalAgentStatus;

/// Parsed OSC 7501 status record (`\x1b]7501;state=...:app=...:msg=...:title=...\x1b\\`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Osc7501Record {
    /// Dynamic application identifier (e.g., `"claude"`, `"codex"`, `"my-agent"`).
    pub app: Option<String>,
    /// Mapped lifecycle status (`Idle`, `Working`, `Blocked`, or `Done`).
    pub status: TerminalAgentStatus,
    /// Optional human-readable status message (`msg=`).
    pub msg: Option<String>,
    /// Optional display title (`title=`).
    pub title: Option<String>,
    /// Whether this record explicitly clears the prior status (`state=clear`).
    pub clear: bool,
}

impl Osc7501Record {
    // Public ------------------------------------------------------------------

    /// Parses the body of an OSC 7501 payload (`"state=working:app=claude:msg=Running tests"`).
    pub fn parse_body(body: &str) -> Option<Self> {
        let mut app = None;
        let mut msg = None;
        let mut title = None;
        let mut state_raw: Option<&str> = None;

        for part in body.split([':', ';']) {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            let trimmed = value.trim();
            match key.trim() {
                "state" => state_raw = Some(trimmed),
                "app" if !trimmed.is_empty() => app = Some(trimmed.to_string()),
                "msg" if !trimmed.is_empty() => msg = Some(trimmed.to_string()),
                "title" if !trimmed.is_empty() => title = Some(trimmed.to_string()),
                _ => {}
            }
        }

        let state = state_raw?;
        let clear = state.eq_ignore_ascii_case("clear");
        let status = if clear {
            TerminalAgentStatus::Idle
        } else {
            TerminalAgentStatus::from_osc7501_state(state)
        };

        Some(Self {
            app,
            status,
            msg,
            title,
            clear,
        })
    }

    /// Extracts all OSC 7501 records from a raw terminal byte slice.
    pub fn extract_all(bytes: &[u8]) -> Vec<Self> {
        let text = String::from_utf8_lossy(bytes);
        let mut records = Vec::new();
        let mut rest = text.as_ref();

        while let Some(start) = rest.find("\x1b]7501;") {
            let after = &rest[start + 7..];
            let end_match = after
                .find("\x1b\\")
                .map(|idx| (idx, 2usize))
                .or_else(|| after.find('\x07').map(|idx| (idx, 1usize)));
            let Some((end, term_len)) = end_match else {
                break;
            };
            if let Some(record) = Self::parse_body(&after[..end]) {
                records.push(record);
            }
            rest = &after[end + term_len..];
        }

        records
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_osc_7501_sequences {
        use super::*;

        mod when_parsing_working_and_blocked_records {
            use super::*;

            #[test]
            fn should_extract_dynamic_app_id_status_and_message() {
                // Arrange
                let stream = b"hello\x1b]7501;state=working:app=my-custom-agent:msg=Indexing\x1b\\mid\x1b]7501;state=blocked:app=my-custom-agent:msg=Approve?\x07tail";

                // Act
                let records = Osc7501Record::extract_all(stream);

                // Assert
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].app.as_deref(), Some("my-custom-agent"));
                assert_eq!(records[0].status, TerminalAgentStatus::Working);
                assert_eq!(records[0].msg.as_deref(), Some("Indexing"));
                assert_eq!(records[1].status, TerminalAgentStatus::Blocked);
                assert_eq!(records[1].msg.as_deref(), Some("Approve?"));
            }
        }
    }
}
