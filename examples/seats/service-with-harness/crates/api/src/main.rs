//! The `seats` service binary: serve the axum app over TCP.
//!
//! Capacity is read from the `SEATS_CAPACITY` env var (default 100). All
//! non-determinism is confined to the adapters wired in [`api::http::app`].

#![forbid(unsafe_code)]

use std::net::SocketAddr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // NB: the `core` dependency crate shadows std's `core`, so `#[tokio::main]`
    // (whose expansion references `core::future::…`) won't resolve here. Build
    // the runtime explicitly instead.
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(serve())
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let capacity = std::env::var("SEATS_CAPACITY")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(100);

    let app = api::http::app(capacity);
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("seats service listening on {addr} (capacity {capacity})");
    axum::serve(listener, app).await?;
    Ok(())
}
