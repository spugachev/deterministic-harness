//! Seat-reservation service for a single event with a fixed seat capacity.
//!
//! In-memory store guarded by a `Mutex`, exposed over an axum HTTP API:
//!   POST /holds            { "seats": N }            -> grant or reject a hold
//!   POST /holds/:id/confirm                          -> confirm a hold
//!   POST /holds/:id/release                          -> release a hold (idempotent)
//!   GET  /availability                               -> seats currently available
//!
//! Holds expire after a fixed TTL; expiry is evaluated lazily against the
//! current time whenever the store is touched.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Total seats for the event.
const CAPACITY: u32 = 100;
/// How long a hold lives before it expires and frees its seats.
const HOLD_TTL: Duration = Duration::from_secs(120);

/// A pending (unconfirmed) hold on some seats.
struct Hold {
    seats: u32,
    expires_at: Instant,
}

/// The whole reservation state behind one lock.
#[derive(Default)]
struct Store {
    confirmed: u32,
    holds: HashMap<Uuid, Hold>,
}

impl Store {
    /// Drop any holds whose TTL has elapsed. Called before every read/write so
    /// expired seats are always treated as available.
    fn sweep_expired(&mut self, now: Instant) {
        self.holds.retain(|_, h| h.expires_at > now);
    }

    /// Seats neither confirmed nor currently held.
    fn available(&self) -> u32 {
        let held: u32 = self.holds.values().map(|h| h.seats).sum();
        CAPACITY - self.confirmed - held
    }
}

type AppState = Arc<Mutex<Store>>;

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

/// POST /holds — grant a hold for N seats if enough are available.
async fn create_hold(
    State(store): State<AppState>,
    Json(req): Json<HoldRequest>,
) -> Result<(StatusCode, Json<HoldResponse>), (StatusCode, Json<ErrorResponse>)> {
    if req.seats == 0 {
        return Err(err(StatusCode::BAD_REQUEST, "seats must be greater than 0"));
    }

    let now = Instant::now();
    let mut store = store.lock().unwrap();
    store.sweep_expired(now);

    if req.seats > store.available() {
        return Err(err(
            StatusCode::CONFLICT,
            "insufficient availability for requested seats",
        ));
    }

    let hold_id = Uuid::new_v4();
    store.holds.insert(
        hold_id,
        Hold {
            seats: req.seats,
            expires_at: now + HOLD_TTL,
        },
    );

    Ok((
        StatusCode::CREATED,
        Json(HoldResponse {
            hold_id,
            seats: req.seats,
            ttl_seconds: HOLD_TTL.as_secs(),
        }),
    ))
}

/// POST /holds/:id/confirm — turn a live hold into a permanent booking.
async fn confirm_hold(
    State(store): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let now = Instant::now();
    let mut store = store.lock().unwrap();
    store.sweep_expired(now);

    match store.holds.remove(&id) {
        Some(hold) => {
            store.confirmed += hold.seats;
            Ok(StatusCode::OK)
        }
        None => Err(err(
            StatusCode::CONFLICT,
            "hold is unknown, expired, or already confirmed",
        )),
    }
}

/// POST /holds/:id/release — free an unconfirmed hold. Idempotent: releasing an
/// unknown/expired hold is a no-op success.
async fn release_hold(State(store): State<AppState>, Path(id): Path<Uuid>) -> StatusCode {
    let now = Instant::now();
    let mut store = store.lock().unwrap();
    store.sweep_expired(now);
    store.holds.remove(&id);
    StatusCode::NO_CONTENT
}

/// GET /availability — how many seats are currently free.
async fn availability(State(store): State<AppState>) -> Json<AvailabilityResponse> {
    let now = Instant::now();
    let mut store = store.lock().unwrap();
    store.sweep_expired(now);
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
    let state: AppState = Arc::new(Mutex::new(Store::default()));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    println!(
        "seats service listening on {}",
        listener.local_addr().unwrap()
    );
    axum::serve(listener, app(state)).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Happy-path smoke test over the real HTTP API: hold 2 seats, confirm
    /// them, and verify availability drops from CAPACITY to CAPACITY - 2.
    #[tokio::test]
    async fn hold_confirm_flow_books_seats() {
        let state: AppState = Arc::new(Mutex::new(Store::default()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app(state)).await.unwrap();
        });

        let client = reqwest::Client::new();
        let base = format!("http://{addr}");

        // Start with the full house available.
        let avail: AvailabilityResponse = client
            .get(format!("{base}/availability"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(avail.available, CAPACITY);

        // Hold 2 seats.
        let resp = client
            .post(format!("{base}/holds"))
            .json(&serde_json::json!({ "seats": 2 }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let hold: HoldResponse = resp.json().await.unwrap();
        assert_eq!(hold.seats, 2);

        // Confirm the hold.
        let resp = client
            .post(format!("{base}/holds/{}/confirm", hold.hold_id))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Two seats are now permanently booked.
        let avail: AvailabilityResponse = client
            .get(format!("{base}/availability"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(avail.available, CAPACITY - 2);
    }
}
