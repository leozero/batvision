//! Quick connect: a device without a keyboard (TV) shows a short code, a signed-in
//! user approves it from another device, and the waiting device receives its own token.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use super::{DeviceInfo, crypto};

pub const CODE_TTL: Duration = Duration::from_secs(5 * 60);
/// No 0/O, 1/I/L: easy to read on a TV and type on a phone.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 6;
const MAX_PENDING: usize = 500;
const MAX_PENDING_PER_IP: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Pending,
    Approved { user_id: i64 },
}

#[derive(Debug)]
struct Request {
    code: String,
    /// Who asked: one address can't fill the table and keep every TV out.
    ip: String,
    device: DeviceInfo,
    state: State,
    expires_at: Instant,
}

/// Outcome of a poll by the waiting device.
#[derive(Debug)]
pub enum Poll {
    Pending,
    Expired,
    /// Approved: the request is consumed and the caller issues the session.
    Approved {
        user_id: i64,
        device: DeviceInfo,
    },
}

#[derive(Debug, Default)]
pub struct QuickConnect {
    /// Keyed by the hash of the poll secret held by the waiting device.
    requests: Mutex<HashMap<String, Request>>,
}

pub struct Started {
    pub code: String,
    pub poll_secret: String,
}

impl QuickConnect {
    pub fn start(&self, device: DeviceInfo, ip: &str, now: Instant) -> Option<Started> {
        let mut requests = self.requests.lock().unwrap();
        requests.retain(|_, r| r.expires_at > now);
        if requests.len() >= MAX_PENDING
            || requests.values().filter(|r| r.ip == ip).count() >= MAX_PENDING_PER_IP
        {
            return None;
        }
        let code = loop {
            let code = crypto::random_code(CODE_ALPHABET, CODE_LEN);
            if !requests.values().any(|r| r.code == code) {
                break code;
            }
        };
        let poll_secret = crypto::random_token("bvq_");
        requests.insert(
            crypto::hash_token(&poll_secret),
            Request {
                code: code.clone(),
                ip: ip.to_owned(),
                device,
                state: State::Pending,
                expires_at: now + CODE_TTL,
            },
        );
        Some(Started { code, poll_secret })
    }

    /// Approves the pending request showing `code`. Returns false if no live request matches.
    pub fn approve(&self, code: &str, user_id: i64, now: Instant) -> bool {
        let code = code.trim().to_ascii_uppercase();
        let mut requests = self.requests.lock().unwrap();
        match requests
            .values_mut()
            .find(|r| r.code == code && r.expires_at > now && r.state == State::Pending)
        {
            Some(request) => {
                request.state = State::Approved { user_id };
                true
            }
            None => false,
        }
    }

    pub fn poll(&self, poll_secret: &str, now: Instant) -> Poll {
        let key = crypto::hash_token(poll_secret);
        let mut requests = self.requests.lock().unwrap();
        let Some(request) = requests.get(&key) else {
            return Poll::Expired;
        };
        if request.expires_at <= now {
            requests.remove(&key);
            return Poll::Expired;
        }
        match request.state {
            State::Pending => Poll::Pending,
            State::Approved { user_id } => {
                let request = requests.remove(&key).expect("present");
                Poll::Approved {
                    user_id,
                    device: request.device,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Platform;

    fn tv() -> DeviceInfo {
        DeviceInfo {
            name: "Salon".into(),
            platform: Platform::AndroidTv,
        }
    }

    #[test]
    fn full_flow() {
        let qc = QuickConnect::default();
        let t0 = Instant::now();
        let started = qc.start(tv(), "10.0.0.1", t0).unwrap();
        assert_eq!(started.code.len(), 6);
        assert!(matches!(qc.poll(&started.poll_secret, t0), Poll::Pending));

        assert!(
            qc.approve(&started.code.to_lowercase(), 7, t0),
            "case-insensitive"
        );
        match qc.poll(&started.poll_secret, t0) {
            Poll::Approved { user_id, device } => {
                assert_eq!(user_id, 7);
                assert_eq!(device.name, "Salon");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(
            matches!(qc.poll(&started.poll_secret, t0), Poll::Expired),
            "consumed"
        );
    }

    #[test]
    fn codes_expire() {
        let qc = QuickConnect::default();
        let t0 = Instant::now();
        let started = qc.start(tv(), "10.0.0.1", t0).unwrap();
        let later = t0 + CODE_TTL + Duration::from_secs(1);
        assert!(!qc.approve(&started.code, 1, later));
        assert!(matches!(
            qc.poll(&started.poll_secret, later),
            Poll::Expired
        ));
    }

    #[test]
    fn unknown_code_and_secret() {
        let qc = QuickConnect::default();
        let t0 = Instant::now();
        assert!(!qc.approve("ZZZZZZ", 1, t0));
        assert!(matches!(qc.poll("bvq_nope", t0), Poll::Expired));
    }

    #[test]
    fn an_approved_code_cannot_be_reapproved_by_someone_else() {
        let qc = QuickConnect::default();
        let t0 = Instant::now();
        let started = qc.start(tv(), "10.0.0.1", t0).unwrap();
        assert!(qc.approve(&started.code, 1, t0));
        assert!(!qc.approve(&started.code, 2, t0));
    }

    #[test]
    fn one_address_cannot_fill_the_table() {
        let qc = QuickConnect::default();
        let t0 = Instant::now();
        for _ in 0..MAX_PENDING_PER_IP {
            assert!(qc.start(tv(), "10.0.0.1", t0).is_some());
        }
        assert!(qc.start(tv(), "10.0.0.1", t0).is_none());
        assert!(qc.start(tv(), "10.0.0.2", t0).is_some());
    }
}
