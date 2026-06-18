//! The `seats-server` binary: bind the axum router over the production state.
//!
//! Capacity is read from `SEATS_CAPACITY` (default 100); the bind address from
//! `SEATS_ADDR` (default `0.0.0.0:8080`).

#![forbid(unsafe_code)]

use api::http::{production_state, router};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // NB: `core` is an in-workspace dependency crate, which shadows std's
    // `core` in the extern prelude — so `#[tokio::main]` (its expansion
    // references `::core::future::…`) would mis-resolve. Build the runtime
    // explicitly instead.
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let capacity: u32 = std::env::var("SEATS_CAPACITY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        let addr = std::env::var("SEATS_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());

        let app = router(production_state(capacity));
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        println!("seats-server listening on {addr} with capacity {capacity}");
        axum::serve(listener, app).await?;
        Ok(())
    })
}
