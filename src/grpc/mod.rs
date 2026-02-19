use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use tracing::info;
use uuid::Uuid;

use crate::domain;
use crate::engine::AllocationEngine;

pub mod proto {
    tonic::include_proto!("allocation");
}

use proto::allocation_service_server::AllocationService;
use proto::*;

pub struct AllocationGrpcService {
    engine: Arc<Mutex<AllocationEngine>>,
}

impl AllocationGrpcService {
    pub fn new(engine: Arc<Mutex<AllocationEngine>>) -> Self {
        Self { engine }
    }
}

fn ts_to_dt(ts: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now)
}

fn proto_flight(pf: &Flight) -> domain::Flight {
    domain::Flight {
        flight_id: pf.flight_id.clone(),
        airline: pf.airline.clone(),
        origin: pf.origin.clone(),
        destination: pf.destination.clone(),
        aircraft_type: pf.aircraft_type.clone(),
        scheduled_arrival: ts_to_dt(pf.scheduled_arrival_utc),
        scheduled_departure: ts_to_dt(pf.scheduled_departure_utc),
        status: match FlightStatus::try_from(pf.status) {
            Ok(FlightStatus::Delayed) => domain::FlightStatus::Delayed,
            Ok(FlightStatus::Cancelled) => domain::FlightStatus::Cancelled,
            _ => domain::FlightStatus::Scheduled,
        },
    }
}

fn to_proto_assignment(a: &domain::GateAssignment) -> GateAssignment {
    GateAssignment {
        assignment_id: a.assignment_id.to_string(),
        flight: Some(Flight {
            flight_id: a.flight.flight_id.clone(),
            airline: a.flight.airline.clone(),
            origin: a.flight.origin.clone(),
            destination: a.flight.destination.clone(),
            aircraft_type: a.flight.aircraft_type.clone(),
            scheduled_arrival_utc: a.flight.scheduled_arrival.timestamp(),
            scheduled_departure_utc: a.flight.scheduled_departure.timestamp(),
            status: match a.flight.status {
                domain::FlightStatus::Delayed => FlightStatus::Delayed as i32,
                domain::FlightStatus::Cancelled => FlightStatus::Cancelled as i32,
                _ => FlightStatus::Scheduled as i32,
            },
        }),
        gate: Some(Gate {
            gate_id: a.gate.gate_id.clone(),
            terminal: a.gate.terminal.clone(),
            size: match a.gate.size {
                domain::AircraftSize::Small => GateSize::Small as i32,
                domain::AircraftSize::Medium => GateSize::Medium as i32,
                domain::AircraftSize::Large => GateSize::Large as i32,
            },
            is_available: a.gate.is_available,
        }),
        assigned_from_utc: a.assigned_from.timestamp(),
        assigned_until_utc: a.assigned_until.timestamp(),
    }
}

#[tonic::async_trait]
impl AllocationService for AllocationGrpcService {
    async fn allocate_gate(
        &self,
        req: Request<AllocateGateRequest>,
    ) -> Result<Response<AllocateGateResponse>, Status> {
        let r = req.into_inner();
        let f = r
            .flight
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("Flight required"))?;
        let mut eng = self.engine.lock().await;
        let res = eng.allocate_gate(&proto_flight(f), &r.airport_iata, &r.preferred_gates);
        Ok(Response::new(AllocateGateResponse {
            success: res.success,
            assignment: res.assignment.map(|a| to_proto_assignment(&a)),
            message: res.message,
        }))
    }

    async fn report_disruption(
        &self,
        req: Request<proto::DisruptionEvent>,
    ) -> Result<Response<DisruptionResponse>, Status> {
        let r = req.into_inner();
        let af = r
            .affected_flight
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("Flight required"))?;
        let event = domain::DisruptionEvent {
            event_id: Uuid::new_v4(),
            disruption_type: match DisruptionType::try_from(r.r#type) {
                Ok(DisruptionType::Cancellation) => domain::DisruptionType::Cancellation,
                Ok(DisruptionType::Diversion) => domain::DisruptionType::Diversion,
                Ok(DisruptionType::GateUnavailable) => domain::DisruptionType::GateUnavailable,
                _ => domain::DisruptionType::Delay,
            },
            affected_flight_id: af.flight_id.clone(),
            description: r.description,
            reported_at: Utc::now(),
            delay_minutes: r.delay_minutes,
        };
        let mut eng = self.engine.lock().await;
        let res = eng.handle_disruption(event);
        Ok(Response::new(DisruptionResponse {
            acknowledged: res.acknowledged,
            reassignments: res.reassignments.iter().map(to_proto_assignment).collect(),
            summary: res.summary,
        }))
    }

    async fn get_gate_assignments(
        &self,
        req: Request<GateAssignmentsRequest>,
    ) -> Result<Response<GateAssignmentsResponse>, Status> {
        let r = req.into_inner();
        let tf = if r.terminal.is_empty() {
            None
        } else {
            Some(r.terminal.as_str())
        };
        let eng = self.engine.lock().await;
        Ok(Response::new(GateAssignmentsResponse {
            assignments: eng
                .get_assignments(tf)
                .iter()
                .map(|a| to_proto_assignment(a))
                .collect(),
        }))
    }

    type StreamDisruptionsStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::DisruptionEvent, Status>>;

    async fn stream_disruptions(
        &self,
        _req: Request<StreamDisruptionsRequest>,
    ) -> Result<Response<Self::StreamDisruptionsStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(16);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}

pub async fn start_grpc_server(
    engine: Arc<Mutex<AllocationEngine>>,
    addr: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(%addr, "Starting gRPC server");
    tonic::transport::Server::builder()
        .add_service(
            proto::allocation_service_server::AllocationServiceServer::new(
                AllocationGrpcService::new(engine),
            ),
        )
        .serve(addr)
        .await?;
    Ok(())
}
