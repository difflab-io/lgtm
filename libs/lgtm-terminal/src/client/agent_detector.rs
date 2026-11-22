//! Dynamic coding-agent detection combining OSC 7501 status with user-extensible `AgentMatcherConfig` rules and anti-flicker idle confirmation.

use std::time::{Duration, Instant};

use crate::client::herdr_core::{HcForeground, HcStatus, HcTerminal};
use crate::models::agent::{AgentMatcherConfig, DetectedAgent};
use crate::models::status::TerminalAgentStatus;

const IDLE_CONFIRMATION_THRESHOLD: u8 = 3;
const IDLE_CONFIRMATION_CAP: Duration = Duration::from_millis(700);

/// Holds transient `Working -> Idle` drops when no explicit OSC 7501 completion signal is present so momentary spinner redraws do not fire false `Done` notifications.
#[derive(Debug, Default)]
pub(crate) struct PendingIdleConfirmation {
    started_at: Option<Instant>,
    confirmations: u8,
}

impl PendingIdleConfirmation {
    // Public ------------------------------------------------------------------

    pub(crate) fn clear(&mut self) {
        self.started_at = None;
        self.confirmations = 0;
    }

    pub(crate) fn should_hold_working_to_idle(
        &mut self,
        previous: TerminalAgentStatus,
        next: TerminalAgentStatus,
        explicit_protocol_signal: bool,
        now: Instant,
    ) -> bool {
        if explicit_protocol_signal
            || previous != TerminalAgentStatus::Working
            || next != TerminalAgentStatus::Idle
        {
            self.clear();
            return false;
        }

        let Some(started_at) = self.started_at else {
            self.started_at = Some(now);
            self.confirmations = 0;
            return true;
        };

        if now.duration_since(started_at) >= IDLE_CONFIRMATION_CAP {
            self.clear();
            return false;
        }

        self.confirmations = self.confirmations.saturating_add(1);
        if self.confirmations >= IDLE_CONFIRMATION_THRESHOLD {
            self.clear();
            return false;
        }

        true
    }
}

/// Evaluates `herdr-core` terminal snapshots against OSC 7501 records and configured fallback matchers.
#[derive(Debug, Clone)]
pub(crate) struct AgentDetector {
    matchers: Vec<AgentMatcherConfig>,
}

impl AgentDetector {
    /// Creates an [`AgentDetector`] using the supplied data-driven `matchers`.
    pub(crate) fn new(matchers: Vec<AgentMatcherConfig>) -> Self {
        Self { matchers }
    }

    // Public ------------------------------------------------------------------

    /// Resolves the detected agent identity and raw protocol/fallback status from a `herdr-core` terminal snapshot.
    /// Returns `(Option<DetectedAgent>, TerminalAgentStatus, explicit_protocol_signal)`.
    pub(crate) fn evaluate(
        &self,
        terminal: &HcTerminal,
        prior_unseen_done: bool,
    ) -> (Option<DetectedAgent>, TerminalAgentStatus, bool) {
        if let Some(osc) = &terminal.status {
            return Self::from_osc_status(osc, terminal, prior_unseen_done);
        }

        if let Some(matcher) = self.match_fallback(terminal.foreground.as_ref(), &terminal.title) {
            let status = Self::infer_fallback_status(&terminal.title);
            let agent = DetectedAgent {
                app_id: matcher.app_id.clone(),
                label: matcher.label.clone(),
                message: None,
                status,
                unseen_done: prior_unseen_done && status == TerminalAgentStatus::Done,
            };
            return (Some(agent), status, false);
        }

        (None, TerminalAgentStatus::Idle, false)
    }

    // Private -----------------------------------------------------------------

    fn from_osc_status(
        osc: &HcStatus,
        terminal: &HcTerminal,
        prior_unseen_done: bool,
    ) -> (Option<DetectedAgent>, TerminalAgentStatus, bool) {
        let status = TerminalAgentStatus::from_osc7501_state(&osc.state);
        let app_id = osc
            .app
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                terminal
                    .foreground
                    .as_ref()
                    .map(|fg| fg.name.clone())
                    .filter(|name| !name.is_empty())
            });

        let Some(app_id) = app_id else {
            return (None, status, true);
        };

        let label = osc
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Self::title_case_app(&app_id));

        let message = osc
            .msg
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);

        let unseen_done = status == TerminalAgentStatus::Done || prior_unseen_done;

        let agent = DetectedAgent {
            app_id,
            label,
            message,
            status,
            unseen_done,
        };
        (Some(agent), status, true)
    }

    fn match_fallback(
        &self,
        foreground: Option<&HcForeground>,
        title: &str,
    ) -> Option<&AgentMatcherConfig> {
        let fg_name = foreground
            .map(|fg| fg.name.to_ascii_lowercase())
            .unwrap_or_default();
        let fg_argv = foreground
            .map(|fg| fg.argv.join(" ").to_ascii_lowercase())
            .unwrap_or_default();
        let title_lower = title.to_ascii_lowercase();

        self.matchers.iter().find(|matcher| {
            let cmd_hit = matcher.command_patterns.iter().any(|pat| {
                let needle = pat.to_ascii_lowercase();
                !needle.is_empty() && (fg_name.contains(&needle) || fg_argv.contains(&needle))
            });
            let title_hit = matcher.title_patterns.iter().any(|pat| {
                let needle = pat.to_ascii_lowercase();
                !needle.is_empty() && title_lower.contains(&needle)
            });
            cmd_hit || title_hit
        })
    }

    fn infer_fallback_status(title: &str) -> TerminalAgentStatus {
        let trimmed = title.trim();
        if trimmed.contains("Action Required") || trimmed.contains("Permission") {
            return TerminalAgentStatus::Blocked;
        }
        if trimmed.chars().any(|c| {
            ('\u{2800}'..='\u{28FF}').contains(&c) || ('\u{25D0}'..='\u{25D3}').contains(&c)
        }) {
            return TerminalAgentStatus::Working;
        }
        TerminalAgentStatus::Idle
    }

    fn title_case_app(app_id: &str) -> String {
        let mut chars = app_id.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }
}

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_pending_idle_confirmation {
        use super::*;

        mod when_non_protocol_working_briefly_drops_to_idle {
            use super::*;

            #[test]
            fn should_hold_until_confirmation_threshold_is_reached() {
                // Arrange
                let mut guard = PendingIdleConfirmation::default();
                let start = Instant::now();

                // Act & Assert
                assert!(guard.should_hold_working_to_idle(
                    TerminalAgentStatus::Working,
                    TerminalAgentStatus::Idle,
                    false,
                    start,
                ));
                assert!(guard.should_hold_working_to_idle(
                    TerminalAgentStatus::Working,
                    TerminalAgentStatus::Idle,
                    false,
                    start + Duration::from_millis(100),
                ));
                assert!(guard.should_hold_working_to_idle(
                    TerminalAgentStatus::Working,
                    TerminalAgentStatus::Idle,
                    false,
                    start + Duration::from_millis(200),
                ));
                assert!(!guard.should_hold_working_to_idle(
                    TerminalAgentStatus::Working,
                    TerminalAgentStatus::Idle,
                    false,
                    start + Duration::from_millis(300),
                ));
            }
        }
    }
}
