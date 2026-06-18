//! In-memory seat-reservation store.
//!
//! Tracks a fixed capacity of seats and the holds placed against them. All
//! mutating operations take `&self` and lock an internal `Mutex`, so the store
//! is safe to share across concurrent axum handlers behind an `Arc`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

/// A single unconfirmed hold against the inventory.
struct Hold {
    seats: u32,
    expires_at: Instant,
}

struct Inner {
    capacity: u32,
    confirmed: u32,
    holds: HashMap<Uuid, Hold>,
}

pub struct SeatStore {
    ttl: Duration,
    inner: Mutex<Inner>,
}

/// Outcome of a successful hold request.
pub struct HoldGranted {
    pub hold_id: Uuid,
    pub ttl_secs: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HoldError {
    /// Not enough seats are currently available.
    InsufficientAvailability,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfirmError {
    /// The hold id is unknown, already confirmed, or expired.
    NotHeld,
}

impl SeatStore {
    pub fn new(capacity: u32, ttl: Duration) -> Self {
        Self {
            ttl,
            inner: Mutex::new(Inner {
                capacity,
                confirmed: 0,
                holds: HashMap::new(),
            }),
        }
    }

    /// Drop holds whose TTL has elapsed. Caller must hold the lock.
    fn expire(inner: &mut Inner, now: Instant) {
        inner.holds.retain(|_, h| h.expires_at > now);
    }

    /// Seats currently committed: confirmed plus still-live holds.
    fn committed(inner: &Inner) -> u32 {
        let held: u32 = inner.holds.values().map(|h| h.seats).sum();
        inner.confirmed + held
    }

    /// Attempt to hold `seats`. On success returns a fresh hold id and the TTL.
    pub fn hold(&self, seats: u32, now: Instant) -> Result<HoldGranted, HoldError> {
        let mut inner = self.inner.lock().unwrap();
        Self::expire(&mut inner, now);

        let available = inner.capacity - Self::committed(&inner);
        if seats == 0 || seats > available {
            return Err(HoldError::InsufficientAvailability);
        }

        let hold_id = Uuid::new_v4();
        inner.holds.insert(
            hold_id,
            Hold {
                seats,
                expires_at: now + self.ttl,
            },
        );
        Ok(HoldGranted {
            hold_id,
            ttl_secs: self.ttl.as_secs(),
        })
    }

    /// Confirm a live hold, permanently booking its seats.
    pub fn confirm(&self, hold_id: Uuid, now: Instant) -> Result<(), ConfirmError> {
        let mut inner = self.inner.lock().unwrap();
        Self::expire(&mut inner, now);

        match inner.holds.remove(&hold_id) {
            Some(h) => {
                inner.confirmed += h.seats;
                Ok(())
            }
            None => Err(ConfirmError::NotHeld),
        }
    }

    /// Release an unconfirmed hold. Idempotent: releasing an unknown/expired
    /// hold is a no-op.
    pub fn release(&self, hold_id: Uuid, now: Instant) {
        let mut inner = self.inner.lock().unwrap();
        Self::expire(&mut inner, now);
        inner.holds.remove(&hold_id);
    }

    /// Seats currently available to be held.
    pub fn available(&self, now: Instant) -> u32 {
        let mut inner = self.inner.lock().unwrap();
        Self::expire(&mut inner, now);
        inner.capacity - Self::committed(&inner)
    }
}
