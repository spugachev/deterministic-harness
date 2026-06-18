//! The axum HTTP transport. Thin: it maps requests to [`AppState`] operations
//! and domain outcomes to status codes — all logic lives in the verified core.
//!
//! Status mapping (documented in the REQ rationales):
//! - hold granted → `201 Created`; rejected → `409 Conflict`
//! - confirm ok → `200 OK`; failed → `409 Conflict`
//! - release → `204 No Content` (idempotent, always)
//! - availability → `200 OK`

use crate::adapters::{AtomicIdGen, SystemClock};
use crate::state::{AppState, DEFAULT_TTL_SECS};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use core::domain::seats::HoldError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The concrete production state: system clock + atomic id generator.
pub type SharedState = Arc<AppState<SystemClock, AtomicIdGen>>;

/// Body of a hold request.
#[derive(Debug, Deserialize)]
pub struct HoldRequest {
    /// Number of seats to hold.
    pub seats: u32,
}

/// Response to a successful hold.
#[derive(Debug, Serialize)]
pub struct HoldResponse {
    /// The minted hold id.
    pub id: u64,
    /// Seats reserved.
    pub seats: u32,
    /// Expiry instant (seconds since the Unix epoch).
    pub expires_at: i64,
}

/// Response to an availability query.
#[derive(Debug, Serialize)]
pub struct AvailabilityResponse {
    /// Seats currently available.
    pub available: u32,
    /// Total venue capacity.
    pub capacity: u32,
}

/// Build the production state for an event of `capacity` seats.
#[must_use]
pub fn production_state(capacity: u32) -> SharedState {
    Arc::new(AppState::new(
        capacity,
        SystemClock,
        AtomicIdGen::default(),
        DEFAULT_TTL_SECS,
    ))
}

/// Assemble the router over a shared state.
pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/holds", post(post_hold))
        .route("/holds/:id/confirm", post(post_confirm))
        .route("/holds/:id", axum::routing::delete(delete_hold))
        .route("/availability", get(get_availability))
        .with_state(state)
}

/// Map a hold rejection to a status code. Insufficient availability and a
/// zero-seat request are both client conflicts (`409`).
const fn hold_error_status(_e: HoldError) -> StatusCode {
    StatusCode::CONFLICT
}

async fn post_hold(
    State(state): State<SharedState>,
    Json(req): Json<HoldRequest>,
) -> Result<(StatusCode, Json<HoldResponse>), StatusCode> {
    match state.hold(req.seats) {
        Ok(g) => Ok((
            StatusCode::CREATED,
            Json(HoldResponse {
                id: g.id,
                seats: g.seats,
                expires_at: g.expires_at,
            }),
        )),
        Err(e) => Err(hold_error_status(e)),
    }
}

async fn post_confirm(State(state): State<SharedState>, Path(id): Path<u64>) -> StatusCode {
    match state.confirm(id) {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::CONFLICT,
    }
}

async fn delete_hold(State(state): State<SharedState>, Path(id): Path<u64>) -> StatusCode {
    state.release(id);
    StatusCode::NO_CONTENT
}

async fn get_availability(State(state): State<SharedState>) -> Json<AvailabilityResponse> {
    Json(AvailabilityResponse {
        available: state.available(),
        capacity: state.capacity(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test-only assertions")]
    use super::*;

    #[test]
    fn hold_error_maps_to_conflict() {
        assert_eq!(
            hold_error_status(HoldError::InsufficientAvailability),
            StatusCode::CONFLICT
        );
        assert_eq!(
            hold_error_status(HoldError::ZeroSeats),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn production_state_starts_full() {
        let s = production_state(42);
        assert_eq!(s.capacity(), 42);
        assert_eq!(s.available(), 42);
    }
}
