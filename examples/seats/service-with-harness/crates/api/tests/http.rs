//! HTTP-level integration tests for the seat-reservation API.
//!
//! These drive the real axum [`Router`] over a fixed [`FixedClock`] (so TTL
//! expiry is deterministic) using `tower::ServiceExt::oneshot`, and assert the
//! status-code contract the REQs specify (201/409/422/204/200).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test-only"
)]

use std::sync::Arc;

use api::app::{router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt as _;
use seats_core::ports::Clock;
use serde_json::{json, Value};
use tower::ServiceExt as _;

/// A test clock whose time can be advanced between requests.
#[derive(Clone)]
struct TestClock(Arc<std::sync::atomic::AtomicI64>);

impl TestClock {
    fn new(now: i64) -> Self {
        Self(Arc::new(std::sync::atomic::AtomicI64::new(now)))
    }
    fn set(&self, now: i64) {
        self.0.store(now, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_unix(&self) -> i64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

fn app_with(capacity: u32, ttl: i64, clock: TestClock) -> Router {
    router(Arc::new(AppState::new(capacity, ttl, clock)))
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

fn post_json(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn hold_confirm_availability_flow() {
    let clock = TestClock::new(1_000);
    let app = app_with(10, 60, clock.clone());

    // Hold 3 → 201 with an id.
    let (status, body) = send(&app, post_json("/holds", &json!({"seats": 3}))).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = body["id"].as_u64().unwrap();
    assert_eq!(body["seats"], 3);

    // Availability → 7.
    let (status, body) = send(
        &app,
        Request::builder()
            .uri("/availability")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], 7);
    assert_eq!(body["held"], 3);

    // Confirm → 204, then confirmed=3.
    let (status, _) = send(&app, post_json(&format!("/holds/{id}/confirm"), &json!({}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = send(
        &app,
        Request::builder()
            .uri("/availability")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(body["confirmed"], 3);
    assert_eq!(body["available"], 7);
}

#[tokio::test]
async fn over_capacity_hold_conflicts() {
    let app = app_with(5, 60, TestClock::new(0));
    let (status, _) = send(&app, post_json("/holds", &json!({"seats": 4}))).await;
    assert_eq!(status, StatusCode::CREATED);
    // Only 1 seat left — a hold for 2 is a 409 conflict (no overbooking).
    let (status, _) = send(&app, post_json("/holds", &json!({"seats": 2}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn zero_seat_hold_is_unprocessable() {
    let app = app_with(5, 60, TestClock::new(0));
    let (status, _) = send(&app, post_json("/holds", &json!({"seats": 0}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn confirm_expired_hold_conflicts() {
    let clock = TestClock::new(1_000);
    let app = app_with(10, 60, clock.clone());
    let (_, body) = send(&app, post_json("/holds", &json!({"seats": 4}))).await;
    let id = body["id"].as_u64().unwrap();
    // Advance past the TTL; the hold has expired.
    clock.set(1_100);
    let (status, _) = send(&app, post_json(&format!("/holds/{id}/confirm"), &json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn release_is_idempotent_over_http() {
    let app = app_with(10, 60, TestClock::new(0));
    let (_, body) = send(&app, post_json("/holds", &json!({"seats": 4}))).await;
    let id = body["id"].as_u64().unwrap();
    let del = |id: u64| {
        Request::builder()
            .method("DELETE")
            .uri(format!("/holds/{id}"))
            .body(Body::empty())
            .unwrap()
    };
    // First release → 204; availability back to full.
    let (status, _) = send(&app, del(id)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // Second release of the same id → still 204 (no-op).
    let (status, _) = send(&app, del(id)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = send(
        &app,
        Request::builder()
            .uri("/availability")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(body["available"], 10);
}
