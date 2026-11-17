use crate::events::CheckEvidence;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for state transition hysteresis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTrackerConfig {
    /// Number of consecutive failures before emitting a down event (default: 3)
    pub consecutive_failures_threshold: u32,
    /// Number of consecutive successes before emitting an up event (default: 1)
    pub consecutive_successes_threshold: u32,
}

impl Default for StateTrackerConfig {
    fn default() -> Self {
        Self {
            consecutive_failures_threshold: 3,
            consecutive_successes_threshold: 1,
        }
    }
}

/// Tracks state for a single entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityState {
    pub entity_id: String,
    pub entity_type: String,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_success: Option<bool>,
    /// The last confirmed state (up/down) for which an event was emitted
    /// None = unknown (first check), Some(true) = up, Some(false) = down
    pub confirmed_state: Option<bool>,
}

impl EntityState {
    pub fn new(entity_id: String, entity_type: String) -> Self {
        Self {
            entity_id,
            entity_type,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_success: None,
            confirmed_state: None,
        }
    }

    pub fn from_db(
        entity_id: String,
        entity_type: String,
        last_success: Option<bool>,
        consecutive_failures: u32,
        consecutive_successes: u32,
    ) -> Self {
        // Determine confirmed_state from loaded data:
        // - If consecutive_failures >= threshold -> down (confirmed_state = Some(false))
        // - If consecutive_successes >= threshold -> up (confirmed_state = Some(true))
        // - Otherwise unknown (None)
        // Note: threshold values not known here, so we infer from counters
        let confirmed_state = if consecutive_failures >= 3 {
            Some(false) // was down
        } else if consecutive_successes >= 1 {
            Some(true) // was up
        } else {
            None
        };
        Self {
            entity_id,
            entity_type,
            consecutive_failures,
            consecutive_successes,
            last_success,
            confirmed_state,
        }
    }
}

/// Result of processing a check result through the state tracker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    pub entity_id: String,
    pub entity_type: String,
    pub previous_state: Option<bool>,
    pub current_state: bool,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub should_emit_event: bool,
    pub event_type: Option<EventType>,
    pub check_evidence: CheckEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    CheckFailed,    // down event
    CheckCompleted, // up event
}

/// Pure state tracker - no I/O, no time, fully testable
pub struct StateTracker {
    config: StateTrackerConfig,
    states: HashMap<String, EntityState>,
}

impl StateTracker {
    pub fn new(config: StateTrackerConfig) -> Self {
        Self {
            config,
            states: HashMap::new(),
        }
    }

    /// Get or create state for an entity
    fn get_state(&mut self, entity_id: &str, entity_type: &str) -> &mut EntityState {
        let key = format!("{}::{}", entity_type, entity_id);
        self.states
            .entry(key)
            .or_insert_with(|| EntityState::new(entity_id.to_string(), entity_type.to_string()))
    }

    /// Load state from database (for startup recovery)
    pub fn load_state(&mut self, state: EntityState) {
        let key = format!("{}::{}", state.entity_type, state.entity_id);
        self.states.insert(key, state);
    }

    /// Process a check result and return any state transition
    pub fn process_check(
        &mut self,
        check_evidence: CheckEvidence,
        entity_id: &str,
        entity_type: &str,
    ) -> StateTransition {
        // Copy config values to avoid borrow conflicts
        let failures_threshold = self.config.consecutive_failures_threshold;
        let successes_threshold = self.config.consecutive_successes_threshold;

        let state = self.get_state(entity_id, entity_type);
        let previous_state = state.last_success;
        let confirmed_state = state.confirmed_state;

        // Update consecutive counters
        if check_evidence.success {
            state.consecutive_successes += 1;
            state.consecutive_failures = 0;
        } else {
            state.consecutive_failures += 1;
            state.consecutive_successes = 0;
        }
        state.last_success = Some(check_evidence.success);

        // Determine if hysteresis threshold crossed
        // Down event: check fails, consecutive_failures >= threshold, and not already confirmed down
        // Up event: check succeeds, consecutive_successes >= threshold, and WAS confirmed down (recovery)
        let emit_down_event = !check_evidence.success
            && state.consecutive_failures >= failures_threshold
            && confirmed_state != Some(false);
        let emit_up_event = check_evidence.success
            && state.consecutive_successes >= successes_threshold
            && confirmed_state == Some(false); // Only recover from confirmed down state
        let should_emit_event = emit_down_event || emit_up_event;

        // Update confirmed_state if event emitted
        if emit_down_event {
            state.confirmed_state = Some(false);
        } else if emit_up_event {
            state.confirmed_state = Some(true);
        }

        let event_type = if emit_down_event {
            Some(EventType::CheckFailed)
        } else if emit_up_event {
            Some(EventType::CheckCompleted)
        } else {
            None
        };

        StateTransition {
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            previous_state,
            current_state: check_evidence.success,
            consecutive_failures: state.consecutive_failures,
            consecutive_successes: state.consecutive_successes,
            should_emit_event,
            event_type,
            check_evidence,
        }
    }

    /// Get current state for an entity (for testing/inspection)
    pub fn get_entity_state(&self, entity_id: &str, entity_type: &str) -> Option<&EntityState> {
        let key = format!("{}::{}", entity_type, entity_id);
        self.states.get(&key)
    }

    /// Get all tracked states
    pub fn all_states(&self) -> &HashMap<String, EntityState> {
        &self.states
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::CheckEvidence;

    fn make_check_evidence(success: bool, protocol: &str) -> CheckEvidence {
        CheckEvidence {
            target: "test-target".to_string(),
            protocol: protocol.to_string(),
            success,
            latency_ms: if success { Some(10) } else { None },
            error: if success {
                None
            } else {
                Some("timeout".to_string())
            },
        }
    }

    #[test]
    fn test_down_only_after_n_consecutive_failures() {
        let config = StateTrackerConfig {
            consecutive_failures_threshold: 3,
            consecutive_successes_threshold: 1,
        };
        let mut tracker = StateTracker::new(config);

        // 1st failure - no event
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_failures, 1);

        // 2nd failure - no event
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_failures, 2);

        // 3rd failure - DOWN event emitted
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(t.should_emit_event);
        assert_eq!(t.event_type, Some(EventType::CheckFailed));
        assert_eq!(t.consecutive_failures, 3);

        // 4th failure - no new event (already down)
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_failures, 4);
    }

    #[test]
    fn test_up_after_m_consecutive_successes() {
        let config = StateTrackerConfig {
            consecutive_failures_threshold: 3,
            consecutive_successes_threshold: 2, // require 2 successes
        };
        let mut tracker = StateTracker::new(config);

        // First go down
        for _ in 0..3 {
            tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        }

        // 1st success - no event (threshold is 2)
        let t = tracker.process_check(make_check_evidence(true, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_successes, 1);

        // 2nd success - UP event emitted
        let t = tracker.process_check(make_check_evidence(true, "icmp"), "test-1", "device");
        assert!(t.should_emit_event);
        assert_eq!(t.event_type, Some(EventType::CheckCompleted));
        assert_eq!(t.consecutive_successes, 2);

        // 3rd success - no new event
        let t = tracker.process_check(make_check_evidence(true, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_successes, 3);
    }

    #[test]
    fn test_isolated_failure_no_event() {
        let config = StateTrackerConfig {
            consecutive_failures_threshold: 3,
            consecutive_successes_threshold: 1,
        };
        let mut tracker = StateTracker::new(config);

        // Success - no event (first check, no previous down state)
        let t = tracker.process_check(make_check_evidence(true, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);

        // Single failure - no event (threshold is 3)
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_failures, 1);

        // Success again - no UP event because failure threshold was never reached (no down state)
        let t = tracker.process_check(make_check_evidence(true, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_failures, 0);
        assert_eq!(t.consecutive_successes, 1);

        // Now reach down threshold
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(t.should_emit_event); // DOWN event after 3 failures
        assert_eq!(t.event_type, Some(EventType::CheckFailed));

        // Recovery after down state
        let t = tracker.process_check(make_check_evidence(true, "icmp"), "test-1", "device");
        assert!(t.should_emit_event); // UP event after 1 success (recovery)
        assert_eq!(t.event_type, Some(EventType::CheckCompleted));
    }

    #[test]
    fn test_startup_with_last_state_down_no_false_transition() {
        let config = StateTrackerConfig {
            consecutive_failures_threshold: 3,
            consecutive_successes_threshold: 1,
        };
        let mut tracker = StateTracker::new(config);

        // Load state as if we were already down (last check failed, 3 consecutive failures)
        tracker.load_state(EntityState::from_db(
            "test-1".to_string(),
            "device".to_string(),
            Some(false), // last was down
            3,           // already at threshold
            0,
        ));

        // Another failure - should NOT emit new down event (already down)
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_failures, 4);

        // Success - should emit UP event after 1 success
        let t = tracker.process_check(make_check_evidence(true, "icmp"), "test-1", "device");
        assert!(t.should_emit_event);
        assert_eq!(t.event_type, Some(EventType::CheckCompleted));
    }

    #[test]
    fn test_startup_with_last_state_up_no_false_transition() {
        let config = StateTrackerConfig {
            consecutive_failures_threshold: 3,
            consecutive_successes_threshold: 1,
        };
        let mut tracker = StateTracker::new(config);

        // Load state as if we were already up
        tracker.load_state(EntityState::from_db(
            "test-1".to_string(),
            "device".to_string(),
            Some(true), // last was up
            0,
            5, // well above threshold
        ));

        // Success - should NOT emit new up event (already up)
        let t = tracker.process_check(make_check_evidence(true, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_successes, 6);

        // Failure - no event until threshold
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        assert_eq!(t.consecutive_failures, 1);
    }

    #[test]
    fn test_different_entities_independent() {
        let config = StateTrackerConfig {
            consecutive_failures_threshold: 2,
            consecutive_successes_threshold: 1,
        };
        let mut tracker = StateTracker::new(config);

        // Entity 1: 2 failures -> down
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "entity-1", "device");
        assert!(!t.should_emit_event);
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "entity-1", "device");
        assert!(t.should_emit_event);

        // Entity 2: still up, 1 failure -> no event
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "entity-2", "device");
        assert!(!t.should_emit_event);
    }

    #[test]
    fn test_different_entity_types_independent() {
        let config = StateTrackerConfig {
            consecutive_failures_threshold: 2,
            consecutive_successes_threshold: 1,
        };
        let mut tracker = StateTracker::new(config);

        // Same ID, different types should be independent
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(!t.should_emit_event);
        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "service");
        assert!(!t.should_emit_event);

        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "device");
        assert!(t.should_emit_event);

        let t = tracker.process_check(make_check_evidence(false, "icmp"), "test-1", "service");
        assert!(t.should_emit_event);
    }
}
