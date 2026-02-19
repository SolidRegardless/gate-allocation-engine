use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AircraftSize {
    Small,
    Medium,
    Large,
}

impl fmt::Display for AircraftSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Small => write!(f, "Small"),
            Self::Medium => write!(f, "Medium"),
            Self::Large => write!(f, "Large"),
        }
    }
}

/// Wide-body aircraft types that require a Large gate.
const LARGE_AIRCRAFT_TYPES: &[&str] = &["A350", "A380", "B777", "B787", "B747", "A330", "A340"];
/// Regional jets and turboprops that fit a Small gate.
const SMALL_AIRCRAFT_TYPES: &[&str] = &["E190", "E195", "ATR72", "ATR42", "CRJ900", "CRJ700"];

/// Map an ICAO/common aircraft designator to a gate-size category.
/// The default returns `Medium`, which covers the most common unrecognised
/// narrowbodies (e.g. B737 family variants not explicitly listed above).
/// To add a new aircraft type, append its designator to the appropriate const above.
pub fn classify_aircraft(aircraft_type: &str) -> AircraftSize {
    let t = aircraft_type.to_uppercase();
    let t = t.as_str();
    if LARGE_AIRCRAFT_TYPES.contains(&t) {
        AircraftSize::Large
    } else if SMALL_AIRCRAFT_TYPES.contains(&t) {
        AircraftSize::Small
    } else {
        AircraftSize::Medium
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlightStatus {
    Scheduled,
    Boarding,
    Departed,
    EnRoute,
    Arrived,
    Delayed,
    Cancelled,
    Diverted,
}

impl fmt::Display for FlightStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flight {
    pub flight_id: String,
    pub airline: String,
    pub origin: String,
    pub destination: String,
    pub aircraft_type: String,
    pub scheduled_arrival: DateTime<Utc>,
    pub scheduled_departure: DateTime<Utc>,
    pub status: FlightStatus,
}

impl Flight {
    pub fn aircraft_size(&self) -> AircraftSize {
        classify_aircraft(&self.aircraft_type)
    }
}

impl fmt::Display for Flight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}) {} → {} [{}] arr {} dep {}",
            self.flight_id,
            self.aircraft_type,
            self.origin,
            self.destination,
            self.status,
            self.scheduled_arrival.format("%H:%M"),
            self.scheduled_departure.format("%H:%M")
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub gate_id: String,
    pub terminal: String,
    pub size: AircraftSize,
    pub is_available: bool,
}

impl Gate {
    pub fn can_accommodate(&self, aircraft_size: AircraftSize) -> bool {
        self.size >= aircraft_size
    }
}

impl fmt::Display for Gate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = if self.is_available { "AVAIL" } else { "INUSE" };
        write!(
            f,
            "{} [{}] {} {}",
            self.gate_id, self.terminal, self.size, s
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateAssignment {
    pub assignment_id: Uuid,
    pub flight: Flight,
    pub gate: Gate,
    pub assigned_from: DateTime<Utc>,
    pub assigned_until: DateTime<Utc>,
}

impl fmt::Display for GateAssignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Gate {} <- {} ({} - {})",
            self.gate.gate_id,
            self.flight.flight_id,
            self.assigned_from.format("%H:%M"),
            self.assigned_until.format("%H:%M")
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisruptionType {
    Delay,
    Cancellation,
    Diversion,
    GateUnavailable,
    Weather,
    Mechanical,
}

impl fmt::Display for DisruptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisruptionEvent {
    pub event_id: Uuid,
    pub disruption_type: DisruptionType,
    pub affected_flight_id: String,
    pub description: String,
    pub reported_at: DateTime<Utc>,
    pub delay_minutes: i32,
}

impl fmt::Display for DisruptionEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} - {} ({})",
            self.disruption_type,
            self.affected_flight_id,
            self.description,
            self.reported_at.format("%H:%M:%S")
        )
    }
}
