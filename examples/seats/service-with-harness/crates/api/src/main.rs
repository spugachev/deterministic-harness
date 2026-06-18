//! Binary entry point: serve the seat-reservation API.
//!
//! Capacity and TTL come from the environment (`SEATS_CAPACITY`,
//! `SEATS_TTL_SECS`) with sensible defaults. This is the only place a runtime
//! is spun up; the domain and its proofs are entirely runtime-free.

use api::app::production_app;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let capacity = std::env::var("SEATS_CAPACITY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_u32);
    let ttl_secs = std::env::var("SEATS_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120_i64);
    let addr = std::env::var("SEATS_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());

    let app = production_app(capacity, ttl_secs);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("seats listening on {addr} (capacity={capacity}, ttl={ttl_secs}s)");
    axum::serve(listener, app).await?;
    Ok(())
}
