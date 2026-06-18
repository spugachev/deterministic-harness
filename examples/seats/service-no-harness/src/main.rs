//! Seat-reservation HTTP service (axum + in-memory store).

mod store;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use store::SeatStore;

const CAPACITY: u32 = 100;
const HOLD_TTL: Duration = Duration::from_secs(120);

type SharedStore = Arc<SeatStore>;

#[derive(Deserialize)]
struct HoldRequest {
    seats: u32,
}

#[derive(Serialize, Deserialize)]
struct HoldResponse {
    hold_id: Uuid,
    ttl_secs: u64,
}

#[derive(Serialize)]
struct AvailabilityResponse {
    available: u32,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

async fn hold(
    State(store): State<SharedStore>,
    Json(req): Json<HoldRequest>,
) -> Result<Json<HoldResponse>, (StatusCode, Json<ErrorResponse>)> {
    match store.hold(req.seats, Instant::now()) {
        Ok(granted) => Ok(Json(HoldResponse {
            hold_id: granted.hold_id,
            ttl_secs: granted.ttl_secs,
        })),
        Err(_) => Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "insufficient availability".into(),
            }),
        )),
    }
}

async fn confirm(
    State(store): State<SharedStore>,
    Path(hold_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    match store.confirm(hold_id, Instant::now()) {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "hold is not active".into(),
            }),
        )),
    }
}

async fn release(State(store): State<SharedStore>, Path(hold_id): Path<Uuid>) -> StatusCode {
    store.release(hold_id, Instant::now());
    StatusCode::NO_CONTENT
}

async fn availability(State(store): State<SharedStore>) -> Json<AvailabilityResponse> {
    Json(AvailabilityResponse {
        available: store.available(Instant::now()),
    })
}

fn app(store: SharedStore) -> Router {
    Router::new()
        .route("/holds", post(hold))
        .route("/holds/:id/confirm", post(confirm))
        .route("/holds/:id", axum::routing::delete(release))
        .route("/availability", get(availability))
        .with_state(store)
}

#[tokio::main]
async fn main() {
    let store: SharedStore = Arc::new(SeatStore::new(CAPACITY, HOLD_TTL));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("bind 0.0.0.0:3000");
    println!("seats service listening on http://0.0.0.0:3000 (capacity {CAPACITY})");
    axum::serve(listener, app(store)).await.expect("serve");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// Happy path: hold some seats, then confirm them.
    #[tokio::test]
    async fn hold_then_confirm() {
        let store = Arc::new(SeatStore::new(CAPACITY, HOLD_TTL));
        let app = app(store);

        // Hold 3 seats.
        let resp = app
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
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let granted: HoldResponse = serde_json::from_slice(&body).unwrap();

        // Confirm the hold.
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/holds/{}/confirm", granted.hold_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }
}
