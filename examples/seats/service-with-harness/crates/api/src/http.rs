//! The axum HTTP surface, mapping REST calls onto [`SeatService`].
//!
//! Status mapping (the externally-observable contract):
//! - `POST /holds`            → `201 Created` with `{id, expires_at}`, or
//!   `409 Conflict` when seats are unavailable / zero requested (REQ-001).
//! - `POST /holds/:id/confirm`→ `200 OK`, or `404 Not Found` if not live (REQ-002).
//! - `DELETE /holds/:id`      → `204 No Content` always (idempotent, REQ-003).
//! - `GET /availability`      → `200 OK` with `{available}` (REQ-006).

use crate::adapters::{AtomicIdGen, SystemClock};
use crate::service::SeatService;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use core::domain::seats::SeatError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The production service type: real wall-clock + atomic id generator.
pub type AppService = SeatService<SystemClock, AtomicIdGen>;

/// Shared application state handed to every handler.
pub type AppState = Arc<AppService>;

/// Request body for `POST /holds`.
#[derive(Debug, Deserialize)]
pub struct HoldRequest {
    /// Number of seats to hold.
    pub seats: u32,
}

/// Response body for a granted hold.
#[derive(Debug, Serialize)]
pub struct HoldResponse {
    /// The hold id (for confirm/release).
    pub id: u64,
    /// Unix-second instant the hold expires.
    pub expires_at: i64,
}

/// Response body for `GET /availability`.
#[derive(Debug, Serialize)]
pub struct AvailabilityResponse {
    /// Seats currently free.
    pub available: u32,
}

/// Build the router for a service of `capacity` seats wired to production adapters.
pub fn app(capacity: u32) -> Router {
    let state: AppState = Arc::new(SeatService::new(
        capacity,
        SystemClock,
        AtomicIdGen::default(),
    ));
    router(state)
}

/// Build the router around an already-constructed shared state.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/holds", post(create_hold))
        .route("/holds/:id/confirm", post(confirm_hold))
        .route("/holds/:id", delete(release_hold))
        .route("/availability", get(availability))
        .with_state(state)
}

async fn create_hold(
    State(svc): State<AppState>,
    Json(req): Json<HoldRequest>,
) -> Result<(StatusCode, Json<HoldResponse>), StatusCode> {
    match svc.hold(req.seats) {
        Ok(g) => Ok((
            StatusCode::CREATED,
            Json(HoldResponse {
                id: g.id,
                expires_at: g.expires_at,
            }),
        )),
        // Both "full" and "zero requested" are client conflicts with the current
        // state of availability → 409.
        Err(SeatError::InsufficientAvailability | SeatError::ZeroSeatsRequested) => {
            Err(StatusCode::CONFLICT)
        }
        Err(SeatError::UnknownHold) => Err(StatusCode::NOT_FOUND),
    }
}

async fn confirm_hold(State(svc): State<AppState>, Path(id): Path<u64>) -> StatusCode {
    match svc.confirm(id) {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

async fn release_hold(State(svc): State<AppState>, Path(id): Path<u64>) -> StatusCode {
    // Idempotent: whether or not a hold was freed, the client's intent is met.
    let _freed = svc.release(id);
    StatusCode::NO_CONTENT
}

async fn availability(State(svc): State<AppState>) -> Json<AvailabilityResponse> {
    Json(AvailabilityResponse {
        available: svc.available(),
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "unit tests assert on known-good values"
)]
mod tests {
    use super::*;
    use core::ports::{FixedClock, SeqGen};

    fn test_state(capacity: u32) -> Arc<SeatService<FixedClock, SeqGen>> {
        Arc::new(SeatService::new(capacity, FixedClock(0), SeqGen(0)))
    }

    #[test]
    fn hold_confirm_release_roundtrip_via_service() {
        // Exercise the handler logic at the service level (the HTTP wiring is a
        // thin shell; the DST drives it end to end over many seeds).
        let s = test_state(3);
        let g = s.hold(2).unwrap();
        assert_eq!(s.available(), 1);
        s.confirm(g.id).unwrap();
        assert_eq!(s.confirmed(), 2);
        assert!(!s.release(g.id)); // confirmed → not releasable
    }
}
