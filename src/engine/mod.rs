use chrono::{DateTime, Duration, Utc};
use std::fmt;
use tracing::{info, warn};
use uuid::Uuid;

use crate::domain::*;

const TURNAROUND_BUFFER_MINUTES: i64 = 15;
const PENALTY_OVERSIZED_GATE: f64 = 10.0;
const PENALTY_PREFERRED_MISS: f64 = 5.0;
const REWARD_PREFERRED_GATE: f64 = -3.0;

pub struct AllocationEngine {
    pub gates: Vec<Gate>,
    pub assignments: Vec<GateAssignment>,
    pub disruptions: Vec<DisruptionEvent>,
}

#[derive(Debug)]
pub struct AllocationResult {
    pub success: bool,
    pub assignment: Option<GateAssignment>,
    pub message: String,
}

#[derive(Debug)]
pub struct DisruptionResult {
    pub acknowledged: bool,
    pub reassignments: Vec<GateAssignment>,
    pub summary: String,
}

impl AllocationEngine {
    pub fn new() -> Self {
        Self {
            gates: Vec::new(),
            assignments: Vec::new(),
            disruptions: Vec::new(),
        }
    }

    pub fn add_gate(&mut self, gate: Gate) {
        info!(gate_id = %gate.gate_id, terminal = %gate.terminal, "Gate registered");
        self.gates.push(gate);
    }

    /// Allocate the best available gate for a flight using constraint-based scoring.
    pub fn allocate_gate(
        &mut self,
        flight: &Flight,
        airport: &str,
        preferred: &[String],
    ) -> AllocationResult {
        info!(flight = %flight.flight_id, aircraft = %flight.aircraft_type, airport, "Attempting allocation");

        let size = flight.aircraft_size();
        let need_from = flight.scheduled_arrival;
        let need_until = flight.scheduled_departure + Duration::minutes(TURNAROUND_BUFFER_MINUTES);

        let mut candidates: Vec<(&Gate, f64)> = self
            .gates
            .iter()
            .filter(|g| g.is_available && g.can_accommodate(size))
            .filter(|g| !self.has_conflict(&g.gate_id, need_from, need_until))
            .map(|g| (g, self.score_gate(g, size, preferred)))
            .collect();

        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        match candidates.first() {
            Some((gate, score)) => {
                let assignment = GateAssignment {
                    assignment_id: Uuid::new_v4(),
                    flight: flight.clone(),
                    gate: (*gate).clone(),
                    assigned_from: need_from,
                    assigned_until: need_until,
                };
                info!(flight = %flight.flight_id, gate = %gate.gate_id, score, "Allocated");
                self.assignments.push(assignment.clone());
                AllocationResult {
                    success: true,
                    assignment: Some(assignment),
                    message: format!(
                        "Allocated {} -> {} (score: {:.1})",
                        flight.flight_id, gate.gate_id, score
                    ),
                }
            }
            None => {
                warn!(flight = %flight.flight_id, "No available gates");
                AllocationResult {
                    success: false,
                    assignment: None,
                    message: format!(
                        "No compatible gate for {} ({}) at {}",
                        flight.flight_id, flight.aircraft_type, airport
                    ),
                }
            }
        }
    }

    fn has_conflict(&self, gate_id: &str, from: DateTime<Utc>, until: DateTime<Utc>) -> bool {
        self.assignments.iter().any(|a| {
            a.gate.gate_id == gate_id && a.assigned_from < until && a.assigned_until > from
        })
    }

    fn score_gate(&self, gate: &Gate, aircraft_size: AircraftSize, preferred: &[String]) -> f64 {
        let mut score = 0.0;
        let size_diff = (gate.size as i32) - (aircraft_size as i32);
        if size_diff > 0 {
            score += PENALTY_OVERSIZED_GATE * size_diff as f64;
        }
        if !preferred.is_empty() {
            if preferred.contains(&gate.gate_id) {
                score += REWARD_PREFERRED_GATE;
            } else {
                score += PENALTY_PREFERRED_MISS;
            }
        }
        score
    }

    /// Handle a disruption event with automatic re-allocation.
    pub fn handle_disruption(&mut self, event: DisruptionEvent) -> DisruptionResult {
        info!(event = %event.event_id, kind = %event.disruption_type, flight = %event.affected_flight_id, "Disruption");
        self.disruptions.push(event.clone());
        let mut reassignments = Vec::new();

        match event.disruption_type {
            DisruptionType::Delay => {
                let delay = Duration::minutes(event.delay_minutes as i64);
                let indices: Vec<usize> = self
                    .assignments
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.flight.flight_id == event.affected_flight_id)
                    .map(|(i, _)| i)
                    .collect();

                for &idx in &indices {
                    let new_from = self.assignments[idx].assigned_from + delay;
                    let new_until = self.assignments[idx].assigned_until + delay;
                    let gate_id = self.assignments[idx].gate.gate_id.clone();
                    let flight_clone = self.assignments[idx].flight.clone();

                    let conflict = self.assignments.iter().enumerate().any(|(i, other)| {
                        i != idx
                            && other.gate.gate_id == gate_id
                            && other.assigned_from < new_until
                            && other.assigned_until > new_from
                    });

                    if conflict {
                        info!(flight = %flight_clone.flight_id, gate = %gate_id, "Delay conflict - re-allocating");
                        let mut shifted = flight_clone;
                        shifted.scheduled_arrival = shifted.scheduled_arrival + delay;
                        shifted.scheduled_departure = shifted.scheduled_departure + delay;
                        shifted.status = FlightStatus::Delayed;
                        if let Some(new_a) = self
                            .allocate_gate(&shifted, &shifted.destination, &[gate_id])
                            .assignment
                        {
                            reassignments.push(new_a);
                        }
                    } else {
                        let a = &mut self.assignments[idx];
                        a.assigned_from = new_from;
                        a.assigned_until = new_until;
                        a.flight.status = FlightStatus::Delayed;
                        info!(flight = %a.flight.flight_id, gate = %a.gate.gate_id, "Window shifted");
                        reassignments.push(a.clone());
                    }
                }
                DisruptionResult {
                    acknowledged: true,
                    reassignments,
                    summary: format!(
                        "{} delayed {}min - {} assignment(s) adjusted",
                        event.affected_flight_id,
                        event.delay_minutes,
                        indices.len()
                    ),
                }
            }
            DisruptionType::Cancellation => {
                let before = self.assignments.len();
                self.assignments
                    .retain(|a| a.flight.flight_id != event.affected_flight_id);
                let freed = before - self.assignments.len();
                info!(flight = %event.affected_flight_id, freed, "Cancelled - gates freed");
                DisruptionResult {
                    acknowledged: true,
                    reassignments: Vec::new(),
                    summary: format!(
                        "{} cancelled - {} gate(s) freed",
                        event.affected_flight_id, freed
                    ),
                }
            }
            DisruptionType::GateUnavailable => {
                let gate_id = event.description.clone();
                let affected: Vec<Flight> = self
                    .assignments
                    .iter()
                    .filter(|a| a.gate.gate_id == gate_id)
                    .map(|a| a.flight.clone())
                    .collect();

                if let Some(g) = self.gates.iter_mut().find(|g| g.gate_id == gate_id) {
                    g.is_available = false;
                }
                self.assignments.retain(|a| a.gate.gate_id != gate_id);

                for flight in &affected {
                    if let Some(a) = self
                        .allocate_gate(flight, &flight.destination, &[])
                        .assignment
                    {
                        reassignments.push(a);
                    } else {
                        warn!(flight = %flight.flight_id, "Re-allocation failed after gate loss");
                    }
                }
                DisruptionResult {
                    acknowledged: true,
                    reassignments,
                    summary: format!(
                        "Gate {} unavailable - {} flight(s) re-allocated",
                        gate_id,
                        affected.len()
                    ),
                }
            }
            _ => {
                if event.disruption_type == DisruptionType::Diversion {
                    self.assignments
                        .retain(|a| a.flight.flight_id != event.affected_flight_id);
                }
                DisruptionResult {
                    acknowledged: true,
                    reassignments: Vec::new(),
                    summary: format!(
                        "{} event for {}",
                        event.disruption_type, event.affected_flight_id
                    ),
                }
            }
        }
    }

    pub fn get_assignments(&self, terminal: Option<&str>) -> Vec<&GateAssignment> {
        self.assignments
            .iter()
            .filter(|a| terminal.map(|t| a.gate.terminal == t).unwrap_or(true))
            .collect()
    }

    pub fn stats(&self) -> EngineStats {
        EngineStats {
            total_gates: self.gates.len(),
            available_gates: self.gates.iter().filter(|g| g.is_available).count(),
            occupied_gates: self.assignments.len(),
            total_disruptions: self.disruptions.len(),
        }
    }
}

#[derive(Debug)]
pub struct EngineStats {
    pub total_gates: usize,
    pub available_gates: usize,
    pub occupied_gates: usize,
    pub total_disruptions: usize,
}

impl fmt::Display for EngineStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Gates: {}/{} available | Assignments: {} | Disruptions: {}",
            self.available_gates, self.total_gates, self.occupied_gates, self.total_disruptions
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn gate(id: &str, term: &str, size: AircraftSize) -> Gate {
        Gate {
            gate_id: id.into(),
            terminal: term.into(),
            size,
            is_available: true,
        }
    }
    fn flight(id: &str, ac: &str, arr_h: u32, dep_h: u32) -> Flight {
        Flight {
            flight_id: id.into(),
            airline: "Test".into(),
            origin: "LHR".into(),
            destination: "JFK".into(),
            aircraft_type: ac.into(),
            scheduled_arrival: Utc.with_ymd_and_hms(2026, 3, 1, arr_h, 0, 0).unwrap(),
            scheduled_departure: Utc.with_ymd_and_hms(2026, 3, 1, dep_h, 0, 0).unwrap(),
            status: FlightStatus::Scheduled,
        }
    }

    #[test]
    fn allocates_right_sized_gate() {
        let mut e = AllocationEngine::new();
        e.add_gate(gate("A1", "T5", AircraftSize::Medium));
        e.add_gate(gate("A2", "T5", AircraftSize::Large));
        assert_eq!(
            e.allocate_gate(&flight("F1", "A320", 10, 12), "LHR", &[])
                .assignment
                .unwrap()
                .gate
                .gate_id,
            "A1"
        );
    }

    #[test]
    fn rejects_undersized_gate() {
        let mut e = AllocationEngine::new();
        e.add_gate(gate("A1", "T5", AircraftSize::Small));
        assert!(
            !e.allocate_gate(&flight("F1", "A320", 10, 12), "LHR", &[])
                .success
        );
    }

    #[test]
    fn detects_time_conflicts() {
        let mut e = AllocationEngine::new();
        e.add_gate(gate("A1", "T5", AircraftSize::Medium));
        assert!(
            e.allocate_gate(&flight("F1", "A320", 10, 12), "LHR", &[])
                .success
        );
        assert!(
            !e.allocate_gate(&flight("F2", "A320", 11, 13), "LHR", &[])
                .success
        );
    }

    #[test]
    fn cancellation_frees_gate() {
        let mut e = AllocationEngine::new();
        e.add_gate(gate("A1", "T5", AircraftSize::Medium));
        e.allocate_gate(&flight("F1", "A320", 10, 12), "LHR", &[]);
        e.handle_disruption(DisruptionEvent {
            event_id: Uuid::new_v4(),
            disruption_type: DisruptionType::Cancellation,
            affected_flight_id: "F1".into(),
            description: "Cancelled".into(),
            reported_at: Utc::now(),
            delay_minutes: 0,
        });
        assert!(
            e.allocate_gate(&flight("F2", "A320", 11, 13), "LHR", &[])
                .success
        );
    }

    #[test]
    fn prefers_requested_gate() {
        let mut e = AllocationEngine::new();
        e.add_gate(gate("A1", "T5", AircraftSize::Medium));
        e.add_gate(gate("B1", "T5", AircraftSize::Medium));
        let r = e.allocate_gate(&flight("F1", "A320", 10, 12), "LHR", &["B1".into()]);
        assert_eq!(r.assignment.unwrap().gate.gate_id, "B1");
    }
}
