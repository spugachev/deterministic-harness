//! Seat-reservation service for a single event with fixed capacity.
//!
//! In-memory store guarded by a `Mutex`, exposed over an axum HTTP API.
//! Holds expire lazily (TTL checked against the current time on every access).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const CAPACITY: u32 = 100;
const HOLD_TTL: Duration = Duration::from_secs(120);

/// An unconfirmed reservation of `seats` seats that expires at `expires_at`.
#[derive(Clone, Copy)]
struct Hold {
    seats: u32,
    expires_at: Instant,
}

/// The full reservation state for the event.
struct Store {
    capacity: u32,
    /// Seats that are permanently booked.
    confirmed: u32,
    /// Live holds keyed by id.
    holds: HashMap<Uuid, Hold>,
}

impl Store {
    fn new(capacity: u32) -> Self {
        Self {
            capacity,
            confirmed: 0,
            holds: HashMap::new(),
        }
    }

    /// Drop any holds whose TTL has elapsed as of `now`.
    fn expire(&mut self, now: Instant) {
        self.holds.retain(|_, h| h.expires_at > now);
    }

    /// Seats currently spoken for: confirmed + all live holds.
    fn in_use(&self) -> u32 {
        self.confirmed + self.holds.values().map(|h| h.seats).sum::<u32>()
    }

    fn available(&self) -> u32 {
        self.capacity - self.in_use()
    }
}

type SharedStore = Arc<Mutex<Store>>;

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
struct ErrorResponse {
    error: String,
}

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
}

async fn hold(State(store): State<SharedStore>, Json(req): Json<HoldRequest>) -> impl IntoResponse {
    if req.seats == 0 {
        return err(StatusCode::BAD_REQUEST, "must request at least one seat").into_response();
    }

    let now = Instant::now();
    let mut store = store.lock().unwrap();
    store.expire(now);

    if req.seats > store.available() {
        return err(StatusCode::CONFLICT, "insufficient availability").into_response();
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

async fn confirm(State(store): State<SharedStore>, Path(hold_id): Path<Uuid>) -> impl IntoResponse {
    let now = Instant::now();
    let mut store = store.lock().unwrap();
    store.expire(now);

    match store.holds.remove(&hold_id) {
        Some(h) => {
            store.confirmed += h.seats;
            StatusCode::OK.into_response()
        }
        None => {
            err(StatusCode::CONFLICT, "unknown, expired, or already-confirmed hold").into_response()
        }
    }
}

async fn release(State(store): State<SharedStore>, Path(hold_id): Path<Uuid>) -> impl IntoResponse {
    let now = Instant::now();
    let mut store = store.lock().unwrap();
    store.expire(now);

    // Releasing an unknown/expired hold is an idempotent no-op.
    store.holds.remove(&hold_id);
    StatusCode::NO_CONTENT
}

async fn availability(State(store): State<SharedStore>) -> impl IntoResponse {
    let now = Instant::now();
    let mut store = store.lock().unwrap();
    store.expire(now);

    Json(AvailabilityResponse {
        available: store.available(),
        capacity: store.capacity,
    })
}

fn app(store: SharedStore) -> Router {
    Router::new()
        .route("/holds", post(hold))
        .route("/holds/{hold_id}/confirm", post(confirm))
        .route("/holds/{hold_id}", axum::routing::delete(release))
        .route("/availability", get(availability))
        .with_state(store)
}

#[tokio::main]
async fn main() {
    let store: SharedStore = Arc::new(Mutex::new(Store::new(CAPACITY)));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!(
        "seats service listening on {}",
        listener.local_addr().unwrap()
    );
    axum::serve(listener, app(store)).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Happy path: hold 3 seats, then confirm them; availability reflects each step.
    #[tokio::test]
    async fn hold_then_confirm_books_seats() {
        let store: SharedStore = Arc::new(Mutex::new(Store::new(CAPACITY)));
        let app = app(store);

        // Hold 3 seats.
        let resp = app
            .clone()
            .oneshot(
                Request::post("/holds")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"seats":3}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let held: HoldResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(held.seats, 3);

        // Availability dropped by 3.
        let resp = app
            .clone()
            .oneshot(Request::get("/availability").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let avail: AvailabilityResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(avail.available, CAPACITY - 3);

        // Confirm the hold.
        let resp = app
            .clone()
            .oneshot(
                Request::post(format!("/holds/{}/confirm", held.hold_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Still 3 fewer available, now permanently booked.
        let resp = app
            .oneshot(Request::get("/availability").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let avail: AvailabilityResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(avail.available, CAPACITY - 3);
    }
}
