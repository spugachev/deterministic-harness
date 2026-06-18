//! Binary entrypoint: serve the ledger HTTP API on `0.0.0.0:8080`.
//!
//! Thin shell — all behaviour is in [`api::http`] / [`api::state`] (and the
//! verified `core`). Seeds a couple of demo accounts so the service is useful on
//! first boot.

use api::http::router;
use api::state::SharedLedger;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // NB: the `core` dependency shadows std's `core`, so `#[tokio::main]` (whose
    // expansion references `core::future::…`) won't resolve here — build the
    // runtime explicitly instead.
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let ledger = SharedLedger::new();
        ledger.open_account(1, 1_000);
        ledger.open_account(2, 0);

        let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
        println!("ledger listening on {}", listener.local_addr()?);
        axum::serve(listener, router(ledger)).await?;
        Ok(())
    })
}
