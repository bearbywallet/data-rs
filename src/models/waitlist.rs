use crate::config::waitlist::{
    WAITLIST_EMAIL_COOLDOWN_SECS, WAITLIST_KEY, WAITLIST_POW_DIFFICULTY,
    WAITLIST_RATE_LIMIT_MAX, WAITLIST_RATE_LIMIT_WINDOW_SECS,
};
use log::info;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sled::Db;
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Subscriber {
    pub id: u64,
    pub ts: u64,
    pub ip: String,
    pub unsubscribed: bool,
    pub last_email: u64,
}

#[derive(Debug, PartialEq)]
pub enum JoinStatus {
    Created,
    Resubscribed,
    Already,
    Cooldown,
}

pub struct Waitlist {
    db: Db,
    secret: String,
    rate: Mutex<HashMap<IpAddr, (Instant, usize)>>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

pub fn valid_email(email: &str) -> bool {
    let parts: Vec<&str> = email.split('@').collect();

    parts.len() == 2
        && (3..=254).contains(&email.len())
        && !parts[0].is_empty()
        && parts[1].contains('.')
        && !email.contains(' ')
}

pub fn pow_leading_zero_bits(preimage: &str) -> u32 {
    let digest = Sha256::digest(preimage.as_bytes());
    let mut bits = 0;

    for byte in digest {
        if byte == 0 {
            bits += 8;
        } else {
            bits += byte.leading_zeros();
            break;
        }
    }

    bits
}

impl Waitlist {
    pub fn new(db_path: &str) -> Self {
        let secret =
            std::env::var("WAITLIST_SECRET").expect("ENV var WAITLIST_SECRET is required");
        let db = sled::open(format!("{}/{}", db_path, WAITLIST_KEY))
            .expect("Cannot waitlist open database.");

        info!("WAITLIST: loaded, {} subscribers", db.len());

        Waitlist {
            db,
            secret,
            rate: Mutex::new(HashMap::new()),
        }
    }

    /// PoW: client had to find nonce with sha256(email + nonce) >= difficulty zero bits.
    pub fn verify_pow(&self, email: &str, nonce: &str) -> bool {
        !nonce.is_empty()
            && nonce.len() <= 64
            && pow_leading_zero_bits(&format!("{}{}", email, nonce)) >= WAITLIST_POW_DIFFICULTY
    }

    pub fn rate_limit(&self, ip: IpAddr) -> Result<(), Error> {
        let mut rate = self.rate.lock().unwrap();
        let now = Instant::now();
        let entry = rate.entry(ip).or_insert((now, 0));

        if now.duration_since(entry.0) > Duration::from_secs(WAITLIST_RATE_LIMIT_WINDOW_SECS) {
            *entry = (now, 0);
        }

        entry.1 += 1;

        if entry.1 > WAITLIST_RATE_LIMIT_MAX {
            return Err(Error::new(ErrorKind::Other, "rate limit exceeded"));
        }

        Ok(())
    }

    pub fn join(&self, email: &str, ip: String) -> JoinStatus {
        let ts = now();

        match self.db.get(email) {
            Ok(Some(bytes)) => {
                let mut sub: Subscriber = serde_json::from_slice(&bytes).unwrap_or(Subscriber {
                    id: 0,
                    ts,
                    ip,
                    unsubscribed: false,
                    last_email: 0,
                });

                if sub.unsubscribed {
                    sub.unsubscribed = false;
                    sub.last_email = ts;
                    let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
                    return JoinStatus::Resubscribed;
                }

                if ts.saturating_sub(sub.last_email) < WAITLIST_EMAIL_COOLDOWN_SECS {
                    return JoinStatus::Cooldown;
                }

                sub.last_email = ts;
                let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
                JoinStatus::Already
            }
            _ => {
                let sub = Subscriber {
                    id: (self.db.len() + 1) as u64,
                    ts,
                    ip,
                    unsubscribed: false,
                    last_email: ts,
                };
                let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
                JoinStatus::Created
            }
        }
    }

    pub fn unsubscribe(&self, email: &str) {
        if let Ok(Some(bytes)) = self.db.get(email) {
            if let Ok(mut sub) = serde_json::from_slice::<Subscriber>(&bytes) {
                sub.unsubscribed = true;
                let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
            }
        }
    }

    /// Only the mailbox owner knows this token: it is delivered inside the welcome email.
    pub fn token(&self, email: &str) -> String {
        hex::encode(Sha256::digest(
            format!("unsub|{}|{}", self.secret, email).as_bytes(),
        ))[..32]
            .to_string()
    }

    pub fn stats(&self) -> serde_json::Value {
        let (mut total, mut unsubscribed, mut last7d) = (0u64, 0u64, 0u64);
        let week_ago = now() - 7 * 24 * 3600;

        for entry in self.db.iter().flatten() {
            if let Ok(sub) = serde_json::from_slice::<Subscriber>(&entry.1) {
                total += 1;
                if sub.unsubscribed {
                    unsubscribed += 1;
                }
                if sub.ts >= week_ago {
                    last7d += 1;
                }
            }
        }

        json!({ "total": total, "unsubscribed": unsubscribed, "last7d": last7d })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sha256_vector() {
        assert_eq!(pow_leading_zero_bits("abc"), 0);
    }

    #[test]
    fn pow_findable() {
        let mut nonce = 0u64;
        loop {
            if pow_leading_zero_bits(&format!("user@example.com{}", nonce)) >= 8 {
                break;
            }
            nonce += 1;
        }
        assert!(pow_leading_zero_bits(&format!("user@example.com{}", nonce)) >= 8);
        assert!(pow_leading_zero_bits(&format!("user@example.com{}", nonce)) < 16);
    }

    #[test]
    fn email_validation() {
        assert!(valid_email("user@example.com"));
        assert!(valid_email("  USER@Example.COM "));
        assert!(!valid_email("user@localhost"));
        assert!(!valid_email("not-an-email"));
        assert!(!valid_email("a @b.c"));
        assert!(!valid_email("@b.c"));
    }
}
