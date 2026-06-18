//! The HTTP surface (axum) over the [`SharedLedger`].
//!
//! Thin adapter: each handler parses/validates input, calls the shared ledger,
//! and maps the typed domain result to an HTTP status + JSON. No domain rule
//! lives here — the core owns all of them.

use axum::extract::{Path, State as AxumState};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use core::domain::ledger::{TransferError, TransferOutcome};
use core::domain::lifecycle::{Event, State};
use core::domain::parse::parse_transfer_str;

use crate::state::SharedLedger;

/// Build the router wired to a shared ledger.
pub fn router(ledger: SharedLedger) -> Router {
    Router::new()
        .route("/transfer", post(post_transfer))
        .route("/accounts/:id", get(get_account))
        .route("/accounts/:id/lifecycle", post(post_lifecycle))
        .with_state(ledger)
}

/// A raw transfer request: a single protocol line, parsed by the core.
#[derive(Deserialize)]
struct TransferReq {
    /// The untrusted `TRANSFER <from> <to> <amount> <key>` line.
    line: String,
}

/// The JSON outcome of a transfer.
#[derive(Serialize)]
struct TransferResp {
    /// `"applied"` or `"duplicate"`.
    outcome: &'static str,
}

async fn post_transfer(
    AxumState(ledger): AxumState<SharedLedger>,
    Json(req): Json<TransferReq>,
) -> (StatusCode, Json<TransferResp>) {
    let Ok(cmd) = parse_transfer_str(&req.line) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(TransferResp {
                outcome: "parse_error",
            }),
        );
    };
    match ledger.transfer(cmd.from, cmd.to, cmd.amount_cents, &cmd.key) {
        Ok(TransferOutcome::Applied) => (StatusCode::OK, Json(TransferResp { outcome: "applied" })),
        Ok(TransferOutcome::Duplicate) => (
            StatusCode::OK,
            Json(TransferResp {
                outcome: "duplicate",
            }),
        ),
        Err(e) => (
            transfer_status(e),
            Json(TransferResp {
                outcome: reject_reason(e),
            }),
        ),
    }
}

/// Map a typed transfer rejection to an HTTP status.
fn transfer_status(e: TransferError) -> StatusCode {
    match e {
        TransferError::NoSuchAccount => StatusCode::NOT_FOUND,
        TransferError::SelfTransfer | TransferError::InvalidAmount => StatusCode::BAD_REQUEST,
        TransferError::SourceNotOpen | TransferError::DestNotOpen => StatusCode::CONFLICT,
    }
}

/// A stable string reason for a rejection (for the JSON body).
fn reject_reason(e: TransferError) -> &'static str {
    match e {
        TransferError::SelfTransfer => "self_transfer",
        TransferError::NoSuchAccount => "no_such_account",
        TransferError::SourceNotOpen => "source_not_open",
        TransferError::DestNotOpen => "dest_not_open",
        TransferError::InvalidAmount => "invalid_amount",
    }
}

/// An account query response.
#[derive(Serialize)]
struct AccountResp {
    /// Balance in integer cents.
    balance: u64,
    /// Lifecycle state name.
    state: &'static str,
}

async fn get_account(
    AxumState(ledger): AxumState<SharedLedger>,
    Path(id): Path<u64>,
) -> Result<Json<AccountResp>, StatusCode> {
    let (balance, state) = ledger.query(id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(AccountResp {
        balance,
        state: state_name(state),
    }))
}

/// A lifecycle command request.
#[derive(Deserialize)]
struct LifecycleReq {
    /// One of `"freeze"`, `"unfreeze"`, `"close"`.
    event: String,
}

async fn post_lifecycle(
    AxumState(ledger): AxumState<SharedLedger>,
    Path(id): Path<u64>,
    Json(req): Json<LifecycleReq>,
) -> Result<Json<AccountResp>, StatusCode> {
    let event = match req.event.as_str() {
        "freeze" => Event::Freeze,
        "unfreeze" => Event::Unfreeze,
        "close" => Event::Close,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let state = ledger
        .apply_lifecycle(id, event)
        .map_err(|_| StatusCode::CONFLICT)?;
    let balance = ledger.query(id).map_or(0, |(b, _)| b);
    Ok(Json(AccountResp {
        balance,
        state: state_name(state),
    }))
}

/// Stable lifecycle-state name for JSON.
fn state_name(s: State) -> &'static str {
    match s {
        State::Open => "open",
        State::Frozen => "frozen",
        State::Closed => "closed",
    }
}

#[cfg(test)]
mod tests {
    use super::{reject_reason, state_name, transfer_status};
    use axum::http::StatusCode;
    use core::domain::ledger::TransferError;
    use core::domain::lifecycle::State;

    #[test]
    fn statuses_map_each_rejection() {
        assert_eq!(
            transfer_status(TransferError::NoSuchAccount),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            transfer_status(TransferError::SelfTransfer),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            transfer_status(TransferError::InvalidAmount),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            transfer_status(TransferError::SourceNotOpen),
            StatusCode::CONFLICT
        );
        assert_eq!(
            transfer_status(TransferError::DestNotOpen),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn reasons_and_names_are_stable() {
        assert_eq!(reject_reason(TransferError::SelfTransfer), "self_transfer");
        assert_eq!(reject_reason(TransferError::DestNotOpen), "dest_not_open");
        assert_eq!(state_name(State::Open), "open");
        assert_eq!(state_name(State::Frozen), "frozen");
        assert_eq!(state_name(State::Closed), "closed");
    }
}
