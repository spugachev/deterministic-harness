//! Seat-reservation service for a single event with fixed capacity.
//!
//! All mutable state lives behind a single `Mutex`, so every operation
//! (hold / confirm / release) is atomic. That is what guarantees the capacity
//! invariant — confirmed + currently-held seats can never exceed capacity, even
//! under concurrent requests racing for the last seats.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Total seats for the event.
const CAPACITY: u32 = 100;
/// How long a hold survives before it lazily expires.
const HOLD_TTL: Duration = Duration::from_secs(120);

/// A single unconfirmed reservation.
#[derive(Clone)]
struct Hold {
    seats: u32,
    expires_at: Instant,
}

/// Event state. Guarded by a `Mutex` in `AppState`.
struct Store {
    confirmed: u32,
    holds: HashMap<Uuid, Hold>,
}

impl Store {
    fn new() -> Self {
        Self {
            confirmed: 0,
            holds: HashMap::new(),
        }
    }

    /// Drop any holds whose TTL has elapsed (lazy expiry). The freed seats are
    /// simply no longer counted by `held`.
    fn purge_expired(&mut self, now: Instant) {
        self.holds.retain(|_, h| h.expires_at > now);
    }

    /// Seats currently held by *active* (non-expired) holds.
    fn held(&self) -> u32 {
        self.holds.values().map(|h| h.seats).sum()
    }

    /// Seats neither confirmed nor actively held.
    fn available(&self) -> u32 {
        CAPACITY - self.confirmed - self.held()
    }
}

#[derive(Clone)]
struct AppState {
    store: Arc<Mutex<Store>>,
}

#[derive(Deserialize)]
struct HoldRequest {
    seats: u32,
}

#[derive(Serialize, Deserialize)]
struct HoldResponse {
    hold_id: Uuid,
    seats: u32,
    ttl_seconds: u64,
}

#[derive(Serialize, Deserialize)]
struct AvailabilityResponse {
    available: u32,
    capacity: u32,
}

#[derive(Serialize)]
struct MessageResponse {
    message: String,
}

fn message(msg: &str) -> Json<MessageResponse> {
    Json(MessageResponse {
        message: msg.to_string(),
    })
}

/// POST /holds — request to hold N seats.
async fn create_hold(
    State(state): State<AppState>,
    Json(req): Json<HoldRequest>,
) -> impl IntoResponse {
    if req.seats == 0 {
        return (StatusCode::BAD_REQUEST, message("seats must be > 0")).into_response();
    }

    let now = Instant::now();
    let mut store = state.store.lock().unwrap();
    store.purge_expired(now);

    if req.seats > store.available() {
        return (StatusCode::CONFLICT, message("insufficient availability")).into_response();
    }

    let hold_id = Uuid::new_v4();
    store.holds.insert(
        hold_id,
        Hold {
            seats: req.seats,
            expires_at: now + HOLD_TTL,
        },
    );

    (
        StatusCode::CREATED,
        Json(HoldResponse {
            hold_id,
            seats: req.seats,
            ttl_seconds: HOLD_TTL.as_secs(),
        }),
    )
        .into_response()
}

/// POST /holds/:id/confirm — confirm a hold before it expires.
async fn confirm_hold(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    let now = Instant::now();
    let mut store = state.store.lock().unwrap();
    store.purge_expired(now);

    match store.holds.remove(&id) {
        Some(hold) => {
            store.confirmed += hold.seats;
            (StatusCode::OK, message("confirmed")).into_response()
        }
        None => (
            StatusCode::CONFLICT,
            message("unknown, expired, or already-confirmed hold"),
        )
            .into_response(),
    }
}

/// POST /holds/:id/release — release an unconfirmed hold (idempotent).
async fn release_hold(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> impl IntoResponse {
    let now = Instant::now();
    let mut store = state.store.lock().unwrap();
    store.purge_expired(now);

    // Releasing an unknown/expired hold is a no-op.
    store.holds.remove(&id);
    (StatusCode::OK, message("released")).into_response()
}

/// GET /availability — how many seats are currently available.
async fn availability(State(state): State<AppState>) -> impl IntoResponse {
    let now = Instant::now();
    let mut store = state.store.lock().unwrap();
    store.purge_expired(now);

    Json(AvailabilityResponse {
        available: store.available(),
        capacity: CAPACITY,
    })
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/holds", post(create_hold))
        .route("/holds/:id/confirm", post(confirm_hold))
        .route("/holds/:id/release", post(release_hold))
        .route("/availability", get(availability))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState {
        store: Arc::new(Mutex::new(Store::new())),
    };

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("bind 0.0.0.0:3000");
    println!("seat-reservation service listening on http://0.0.0.0:3000");
    axum::serve(listener, app(state)).await.expect("serve");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // for `oneshot`

    /// Happy path: hold seats, confirm them, and check availability drops by
    /// the confirmed amount.
    #[tokio::test]
    async fn hold_then_confirm_happy_path() {
        let state = AppState {
            store: Arc::new(Mutex::new(Store::new())),
        };
        let router = app(state);

        // Hold 3 seats.
        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/holds")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"seats":3}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let hold: HoldResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(hold.seats, 3);

        // Confirm it.
        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/holds/{}/confirm", hold.hold_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Availability should now be capacity - 3.
        let resp = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/availability")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let avail: AvailabilityResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(avail.available, CAPACITY - 3);
        assert_eq!(avail.capacity, CAPACITY);
    }
}

