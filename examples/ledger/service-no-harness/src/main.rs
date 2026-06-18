//! Transfer ledger service: in-memory, integer-cent accounts, axum HTTP front.
//!
//! Move-fast build: parse a single-line TRANSFER protocol, apply transfers with
//! a conservation invariant, idempotency keys, and an Open->Frozen->Closed
//! account lifecycle. State lives behind a Mutex so concurrent transfers are
//! serialized and can never double-spend or drive a balance negative.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Domain
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum LifecycleState {
    Open,
    Frozen,
    Closed,
}

#[derive(Clone, Debug)]
struct Account {
    balance_cents: u64,
    state: LifecycleState,
}

impl Account {
    fn accepts_transfers(&self) -> bool {
        matches!(self.state, LifecycleState::Open)
    }
}

/// A parsed, validated transfer command.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TransferCommand {
    from: u64,
    to: u64,
    amount_cents: u64,
    idempotency_key: String,
}

/// Typed parse error — the parser must never panic on any input.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ParseError {
    Empty,
    BadVerb,
    WrongFieldCount,
    NotANumber(&'static str),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty command"),
            ParseError::BadVerb => write!(f, "unknown verb (expected TRANSFER)"),
            ParseError::WrongFieldCount => write!(f, "wrong field count"),
            ParseError::NotANumber(field) => {
                write!(f, "field `{field}` is not an unsigned integer")
            }
        }
    }
}

/// Typed transfer-rejection error — no state change occurs.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TransferError {
    ZeroAmount,
    SelfTransfer,
    InsufficientFunds,
    AccountNotFound(u64),
    AccountNotOpen(u64),
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferError::ZeroAmount => write!(f, "amount must be greater than zero"),
            TransferError::SelfTransfer => write!(f, "from and to must differ"),
            TransferError::InsufficientFunds => write!(f, "insufficient funds"),
            TransferError::AccountNotFound(id) => write!(f, "account {id} not found"),
            TransferError::AccountNotOpen(id) => write!(f, "account {id} is frozen or closed"),
        }
    }
}

/// Parse the untrusted single-line protocol. Never panics.
///
/// `TRANSFER <from_id> <to_id> <amount_cents> <idempotency_key>`
fn parse_transfer(input: &str) -> Result<TransferCommand, ParseError> {
    let line = input.trim();
    if line.is_empty() {
        return Err(ParseError::Empty);
    }
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields[0] != "TRANSFER" {
        return Err(ParseError::BadVerb);
    }
    if fields.len() != 5 {
        return Err(ParseError::WrongFieldCount);
    }
    let from = fields[1]
        .parse::<u64>()
        .map_err(|_| ParseError::NotANumber("from_id"))?;
    let to = fields[2]
        .parse::<u64>()
        .map_err(|_| ParseError::NotANumber("to_id"))?;
    let amount_cents = fields[3]
        .parse::<u64>()
        .map_err(|_| ParseError::NotANumber("amount_cents"))?;
    let idempotency_key = fields[4].to_string();
    Ok(TransferCommand {
        from,
        to,
        amount_cents,
        idempotency_key,
    })
}

/// The outcome recorded against an idempotency key, so re-submission replays it.
#[derive(Clone, Debug, Serialize)]
struct TransferReceipt {
    from: u64,
    to: u64,
    amount_cents: u64,
    from_balance_cents: u64,
    to_balance_cents: u64,
}

#[derive(Default)]
struct Ledger {
    accounts: HashMap<u64, Account>,
    applied: HashMap<String, TransferReceipt>,
}

impl Ledger {
    fn open_account(&mut self, id: u64, initial_cents: u64) {
        self.accounts.insert(
            id,
            Account {
                balance_cents: initial_cents,
                state: LifecycleState::Open,
            },
        );
    }

    /// Apply a transfer. Conservation holds: a success moves money, a rejection
    /// changes nothing, and a replayed key moves money at most once.
    fn apply_transfer(&mut self, cmd: &TransferCommand) -> Result<TransferReceipt, TransferError> {
        // Idempotency: a known key replays the original outcome, no state change.
        if let Some(receipt) = self.applied.get(&cmd.idempotency_key) {
            return Ok(receipt.clone());
        }

        if cmd.amount_cents == 0 {
            return Err(TransferError::ZeroAmount);
        }
        if cmd.from == cmd.to {
            return Err(TransferError::SelfTransfer);
        }

        let from = self
            .accounts
            .get(&cmd.from)
            .ok_or(TransferError::AccountNotFound(cmd.from))?;
        if !from.accepts_transfers() {
            return Err(TransferError::AccountNotOpen(cmd.from));
        }
        let from_balance = from.balance_cents;

        let to = self
            .accounts
            .get(&cmd.to)
            .ok_or(TransferError::AccountNotFound(cmd.to))?;
        if !to.accepts_transfers() {
            return Err(TransferError::AccountNotOpen(cmd.to));
        }
        let to_balance = to.balance_cents;

        if from_balance < cmd.amount_cents {
            return Err(TransferError::InsufficientFunds);
        }

        // Commit: subtraction is guarded above; addition can't overflow because
        // the total supply already fit in u64 before the move.
        let new_from = from_balance - cmd.amount_cents;
        let new_to = to_balance + cmd.amount_cents;
        self.accounts.get_mut(&cmd.from).unwrap().balance_cents = new_from;
        self.accounts.get_mut(&cmd.to).unwrap().balance_cents = new_to;

        let receipt = TransferReceipt {
            from: cmd.from,
            to: cmd.to,
            amount_cents: cmd.amount_cents,
            from_balance_cents: new_from,
            to_balance_cents: new_to,
        };
        self.applied
            .insert(cmd.idempotency_key.clone(), receipt.clone());
        Ok(receipt)
    }

    fn set_state(&mut self, id: u64, state: LifecycleState) -> Result<(), TransferError> {
        let acct = self
            .accounts
            .get_mut(&id)
            .ok_or(TransferError::AccountNotFound(id))?;
        // Closed is terminal.
        if acct.state == LifecycleState::Closed {
            return Err(TransferError::AccountNotOpen(id));
        }
        acct.state = state;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// HTTP layer
// ---------------------------------------------------------------------------

type SharedLedger = Arc<Mutex<Ledger>>;

#[derive(Serialize)]
struct AccountView {
    id: u64,
    balance_cents: u64,
    state: LifecycleState,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn err_response(code: StatusCode, msg: String) -> Response {
    (code, Json(ErrorBody { error: msg })).into_response()
}

async fn transfer(State(ledger): State<SharedLedger>, body: String) -> Response {
    let cmd = match parse_transfer(&body) {
        Ok(cmd) => cmd,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e.to_string()),
    };
    let mut guard = ledger.lock().unwrap();
    match guard.apply_transfer(&cmd) {
        Ok(receipt) => (StatusCode::OK, Json(receipt)).into_response(),
        Err(e) => err_response(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
    }
}

async fn get_account(State(ledger): State<SharedLedger>, Path(id): Path<u64>) -> Response {
    let guard = ledger.lock().unwrap();
    match guard.accounts.get(&id) {
        Some(acct) => (
            StatusCode::OK,
            Json(AccountView {
                id,
                balance_cents: acct.balance_cents,
                state: acct.state,
            }),
        )
            .into_response(),
        None => err_response(StatusCode::NOT_FOUND, format!("account {id} not found")),
    }
}

async fn freeze(State(ledger): State<SharedLedger>, Path(id): Path<u64>) -> Response {
    set_lifecycle(&ledger, id, LifecycleState::Frozen)
}

async fn close(State(ledger): State<SharedLedger>, Path(id): Path<u64>) -> Response {
    set_lifecycle(&ledger, id, LifecycleState::Closed)
}

fn set_lifecycle(ledger: &SharedLedger, id: u64, state: LifecycleState) -> Response {
    let mut guard = ledger.lock().unwrap();
    match guard.set_state(id, state) {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => err_response(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
    }
}

fn app(ledger: SharedLedger) -> Router {
    Router::new()
        .route("/transfer", post(transfer))
        .route("/accounts/:id", get(get_account))
        .route("/accounts/:id/freeze", post(freeze))
        .route("/accounts/:id/close", post(close))
        .with_state(ledger)
}

fn seed_ledger() -> SharedLedger {
    let mut ledger = Ledger::default();
    // A couple of demo accounts so the service is useful out of the box.
    ledger.open_account(1, 10_000);
    ledger.open_account(2, 0);
    Arc::new(Mutex::new(ledger))
}

#[tokio::main]
async fn main() {
    let ledger = seed_ledger();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("bind 127.0.0.1:3000");
    println!("ledger listening on http://127.0.0.1:3000");
    axum::serve(listener, app(ledger)).await.expect("serve");
}

// ---------------------------------------------------------------------------
// Tests — one happy-path smoke test, end to end over HTTP.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn happy_path_transfer_moves_money() {
        let ledger = seed_ledger();
        let router = app(ledger);

        // Move 2500 cents from account 1 -> account 2.
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/transfer")
                    .body(Body::from("TRANSFER 1 2 2500 key-abc"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let receipt: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(receipt["from_balance_cents"], 7500);
        assert_eq!(receipt["to_balance_cents"], 2500);

        // Confirm via query.
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/accounts/2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let view: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(view["balance_cents"], 2500);
        assert_eq!(view["state"], "open");
    }
}
