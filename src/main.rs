mod domain;
mod engine;
mod grpc;

use chrono::{TimeZone, Utc};
use domain::*;
use engine::AllocationEngine;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

fn seed_gates() -> Vec<Gate> {
    vec![
        Gate {
            gate_id: "T5-A1".into(),
            terminal: "T5".into(),
            size: AircraftSize::Large,
            is_available: true,
        },
        Gate {
            gate_id: "T5-A2".into(),
            terminal: "T5".into(),
            size: AircraftSize::Large,
            is_available: true,
        },
        Gate {
            gate_id: "T5-B1".into(),
            terminal: "T5".into(),
            size: AircraftSize::Medium,
            is_available: true,
        },
        Gate {
            gate_id: "T5-B2".into(),
            terminal: "T5".into(),
            size: AircraftSize::Medium,
            is_available: true,
        },
        Gate {
            gate_id: "T5-B3".into(),
            terminal: "T5".into(),
            size: AircraftSize::Medium,
            is_available: true,
        },
        Gate {
            gate_id: "T5-C1".into(),
            terminal: "T5".into(),
            size: AircraftSize::Small,
            is_available: true,
        },
        Gate {
            gate_id: "T5-C2".into(),
            terminal: "T5".into(),
            size: AircraftSize::Small,
            is_available: true,
        },
        Gate {
            gate_id: "T2-A1".into(),
            terminal: "T2".into(),
            size: AircraftSize::Large,
            is_available: true,
        },
        Gate {
            gate_id: "T2-B1".into(),
            terminal: "T2".into(),
            size: AircraftSize::Medium,
            is_available: true,
        },
        Gate {
            gate_id: "T2-B2".into(),
            terminal: "T2".into(),
            size: AircraftSize::Medium,
            is_available: true,
        },
    ]
}

fn seed_flights() -> Vec<Flight> {
    let d = |h: u32, m: u32| Utc.with_ymd_and_hms(2026, 3, 15, h, m, 0).unwrap();
    vec![
        Flight {
            flight_id: "BA-117".into(),
            airline: "British Airways".into(),
            origin: "JFK".into(),
            destination: "LHR".into(),
            aircraft_type: "B777".into(),
            scheduled_arrival: d(6, 30),
            scheduled_departure: d(9, 15),
            status: FlightStatus::Scheduled,
        },
        Flight {
            flight_id: "BA-303".into(),
            airline: "British Airways".into(),
            origin: "CDG".into(),
            destination: "LHR".into(),
            aircraft_type: "A320".into(),
            scheduled_arrival: d(7, 0),
            scheduled_departure: d(8, 45),
            status: FlightStatus::Scheduled,
        },
        Flight {
            flight_id: "BA-609".into(),
            airline: "British Airways".into(),
            origin: "EDI".into(),
            destination: "LHR".into(),
            aircraft_type: "E190".into(),
            scheduled_arrival: d(7, 15),
            scheduled_departure: d(8, 30),
            status: FlightStatus::Scheduled,
        },
        Flight {
            flight_id: "BA-215".into(),
            airline: "British Airways".into(),
            origin: "DXB".into(),
            destination: "LHR".into(),
            aircraft_type: "A350".into(),
            scheduled_arrival: d(7, 45),
            scheduled_departure: d(10, 30),
            status: FlightStatus::Scheduled,
        },
        Flight {
            flight_id: "BA-456".into(),
            airline: "British Airways".into(),
            origin: "MAD".into(),
            destination: "LHR".into(),
            aircraft_type: "A320".into(),
            scheduled_arrival: d(8, 0),
            scheduled_departure: d(10, 0),
            status: FlightStatus::Scheduled,
        },
        Flight {
            flight_id: "LH-901".into(),
            airline: "Lufthansa".into(),
            origin: "FRA".into(),
            destination: "LHR".into(),
            aircraft_type: "A320".into(),
            scheduled_arrival: d(8, 15),
            scheduled_departure: d(10, 15),
            status: FlightStatus::Scheduled,
        },
        Flight {
            flight_id: "AF-1680".into(),
            airline: "Air France".into(),
            origin: "CDG".into(),
            destination: "LHR".into(),
            aircraft_type: "A320".into(),
            scheduled_arrival: d(8, 30),
            scheduled_departure: d(10, 30),
            status: FlightStatus::Scheduled,
        },
        Flight {
            flight_id: "BA-178".into(),
            airline: "British Airways".into(),
            origin: "SIN".into(),
            destination: "LHR".into(),
            aircraft_type: "B787".into(),
            scheduled_arrival: d(9, 0),
            scheduled_departure: d(12, 0),
            status: FlightStatus::Scheduled,
        },
    ]
}

async fn run_demo() {
    println!();
    println!("=====================================================================");
    println!("  Gate Allocation Engine -- Simulation Demo");
    println!("  Aviation Gate Allocation & Disruption Optimisation");
    println!("=====================================================================");

    let mut engine = AllocationEngine::new();

    println!("\n--- Phase 1: Registering Airport Gates ---\n");
    for gate in seed_gates() {
        println!("  [+] {}", gate);
        engine.add_gate(gate);
    }

    println!("\n--- Phase 2: Morning Schedule -- Gate Allocation ---\n");
    for flight in seed_flights() {
        let preferred = match flight.airline.as_str() {
            "British Airways" => vec![
                "T5-A1".into(),
                "T5-A2".into(),
                "T5-B1".into(),
                "T5-B2".into(),
                "T5-B3".into(),
            ],
            _ => vec!["T2-A1".into(), "T2-B1".into(), "T2-B2".into()],
        };
        let r = engine.allocate_gate(&flight, "LHR", &preferred);
        if r.success {
            let a = r.assignment.as_ref().unwrap();
            println!(
                "  [OK] {} -> Gate {} ({} - {})",
                flight.flight_id,
                a.gate.gate_id,
                a.assigned_from.format("%H:%M"),
                a.assigned_until.format("%H:%M")
            );
        } else {
            println!("  [!!] {} -- {}", flight.flight_id, r.message);
        }
    }

    println!("\n  Stats: {}\n", engine.stats());

    println!("--- Phase 3: Disruption Events ---\n");

    // Delay
    println!("  [!] BA-303 from CDG delayed 45 minutes (fog)");
    let r = engine.handle_disruption(DisruptionEvent {
        event_id: Uuid::new_v4(),
        disruption_type: DisruptionType::Delay,
        affected_flight_id: "BA-303".into(),
        description: "Fog at CDG".into(),
        reported_at: Utc::now(),
        delay_minutes: 45,
    });
    println!("      -> {}", r.summary);
    for a in &r.reassignments {
        println!("      -> Reassigned: {}", a);
    }

    // Cancellation
    println!("\n  [!] LH-901 from FRA cancelled (technical fault)");
    let r = engine.handle_disruption(DisruptionEvent {
        event_id: Uuid::new_v4(),
        disruption_type: DisruptionType::Cancellation,
        affected_flight_id: "LH-901".into(),
        description: "Hydraulic fault".into(),
        reported_at: Utc::now(),
        delay_minutes: 0,
    });
    println!("      -> {}", r.summary);

    // Gate unavailable
    println!("\n  [!] Gate T5-A1 out of service (jetbridge fault)");
    let r = engine.handle_disruption(DisruptionEvent {
        event_id: Uuid::new_v4(),
        disruption_type: DisruptionType::GateUnavailable,
        affected_flight_id: String::new(),
        description: "T5-A1".into(),
        reported_at: Utc::now(),
        delay_minutes: 0,
    });
    println!("      -> {}", r.summary);
    for a in &r.reassignments {
        println!("      -> Reassigned: {}", a);
    }

    // Final state
    println!("\n--- Phase 4: Final Gate Assignments ---\n");
    for a in engine.get_assignments(None) {
        println!(
            "  [>] {} -> Gate {} [{} - {}] ({})",
            a.flight.flight_id,
            a.gate.gate_id,
            a.assigned_from.format("%H:%M"),
            a.assigned_until.format("%H:%M"),
            a.flight.status
        );
    }

    println!("\n  Stats: {}", engine.stats());
    println!("\n--- Simulation Complete ---\n");
}

async fn run_server() {
    let engine = Arc::new(Mutex::new(AllocationEngine::new()));
    {
        let mut e = engine.lock().await;
        for g in seed_gates() {
            e.add_gate(g);
        }
    }

    let addr = "[::]:50051".parse().unwrap();
    println!("\n=====================================================================");
    println!("  Gate Allocation Engine -- gRPC Server");
    println!("  Listening on {}", addr);
    println!("=====================================================================\n");

    grpc::start_grpc_server(engine, addr)
        .await
        .expect("gRPC server failed");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();

    let mode = std::env::args().nth(1).unwrap_or_else(|| "demo".into());
    match mode.as_str() {
        "demo" => run_demo().await,
        "serve" => run_server().await,
        other => {
            eprintln!("Unknown mode: '{}'. Use 'demo' or 'serve'.", other);
            std::process::exit(1);
        }
    }
}
