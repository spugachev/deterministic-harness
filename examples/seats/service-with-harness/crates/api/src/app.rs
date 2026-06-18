//! The axum HTTP layer for the seat-reservation service.
//!
//! This is pure plumbing: it locks the shared [`Reservation`], calls the IO-free
//! domain through the Clock/IdGen ports, and maps the domain's `Result`s onto
//! HTTP status codes. All the safety logic (no overbooking, lazy expiry) lives
//! in `core` and is proven there — the handlers only translate.

use std::sync::Mutex;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use seats_core::domain::reservation::{ConfirmError, HoldError, Reservation};
use seats_core::ports::{Clock, IdGen};
use serde::{Deserialize, Serialize};

use crate::adapters::{AtomicIds, SystemClock};

/// An [`IdGen`] view over a shared [`AtomicIds`], so handlers can mint ids
/// through the port without owning the generator mutably.
struct SharedIds<'a>(&'a AtomicIds);

impl IdGen for SharedIds<'_> {
    fn next_id(&mut self) -> u64 {
        self.0.next_shared()
    }
}

/// Shared application state: the single event's reservation behind a mutex, the
/// id generator, and the clock adapter. Generic over the [`Clock`] so tests can
/// inject a fixed clock and production uses [`SystemClock`].
pub struct AppState<C: Clock> {
    reservation: Mutex<Reservation>,
    ids: AtomicIds,
    clock: C,
}

impl<C: Clock> AppState<C> {
    /// Build state for an event with `capacity` seats and `ttl_secs` hold TTL.
    #[must_use]
    pub fn new(capacity: u32, ttl_secs: i64, clock: C) -> Self {
        Self {
            reservation: Mutex::new(Reservation::new(capacity, ttl_secs)),
            ids: AtomicIds::default(),
            clock,
        }
    }
}

/// Build the router with production adapters (the system clock).
pub fn production_app(capacity: u32, ttl_secs: i64) -> Router {
    router(std::sync::Arc::new(AppState::new(
        capacity,
        ttl_secs,
        SystemClock,
    )))
}

/// Build the router over any [`AppState`] (used by tests with a fixed clock).
pub fn router<C: Clock + Send + Sync + 'static>(state: std::sync::Arc<AppState<C>>) -> Router {
    Router::new()
        .route("/holds", post(hold))
        .route("/holds/{id}/confirm", post(confirm))
        .route("/holds/{id}", axum::routing::delete(release))
        .route("/availability", get(availability))
        .with_state(state)
}

#[derive(Deserialize)]
struct HoldRequest {
    seats: u32,
}

#[derive(Serialize)]
struct HoldResponse {
    id: u64,
    seats: u32,
    expires_at: i64,
}

#[derive(Serialize)]
struct AvailabilityResponse {
    available: u32,
    confirmed: u32,
    held: u32,
}

/// POST /holds — request a hold (REQ-001). 201 on success; 409 when there is
/// insufficient availability; 422 for a zero-seat request.
async fn hold<C: Clock + Send + Sync + 'static>(
    State(state): State<std::sync::Arc<AppState<C>>>,
    Json(req): Json<HoldRequest>,
) -> Result<(StatusCode, Json<HoldResponse>), StatusCode> {
    let mut r = state
        .reservation
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut ids = SharedIds(&state.ids);
    match r.hold(req.seats, &state.clock, &mut ids) {
        Ok(h) => Ok((
            StatusCode::CREATED,
            Json(HoldResponse {
                id: h.id,
                seats: h.seats,
                expires_at: h.expires_at,
            }),
        )),
        Err(HoldError::InsufficientAvailability) => Err(StatusCode::CONFLICT),
        Err(HoldError::ZeroSeats) => Err(StatusCode::UNPROCESSABLE_ENTITY),
    }
}

/// POST /holds/{id}/confirm — confirm a hold (REQ-002). 204 on success; 409 when
/// the hold is not confirmable (unknown, expired, released, already confirmed).
async fn confirm<C: Clock + Send + Sync + 'static>(
    State(state): State<std::sync::Arc<AppState<C>>>,
    Path(id): Path<u64>,
) -> StatusCode {
    let Ok(mut r) = state.reservation.lock() else {
        return StatusCode::INTERNAL_SERVER_ERROR;
    };
    match r.confirm(id, &state.clock) {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(ConfirmError::NotConfirmable) => StatusCode::CONFLICT,
    }
}

/// DELETE /holds/{id} — release a hold (REQ-003). Idempotent: always 204, even
/// for an unknown or expired hold (releasing is a safe no-op).
async fn release<C: Clock + Send + Sync + 'static>(
    State(state): State<std::sync::Arc<AppState<C>>>,
    Path(id): Path<u64>,
) -> StatusCode {
    let Ok(mut r) = state.reservation.lock() else {
        return StatusCode::INTERNAL_SERVER_ERROR;
    };
    let _ = r.release(id, &state.clock);
    StatusCode::NO_CONTENT
}

/// GET /availability — report current availability (REQ-006).
async fn availability<C: Clock + Send + Sync + 'static>(
    State(state): State<std::sync::Arc<AppState<C>>>,
) -> Result<Json<AvailabilityResponse>, StatusCode> {
    let r = state
        .reservation
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let now = state.clock.now_unix();
    Ok(Json(AvailabilityResponse {
        available: r.available(now),
        confirmed: r.confirmed(),
        held: r.held(now),
    }))
}
