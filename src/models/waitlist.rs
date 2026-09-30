use crate::config::waitlist::{
    WAITLIST_CODE_MAX_ATTEMPTS, WAITLIST_CODE_TTL_SECS, WAITLIST_EMAIL_COOLDOWN_SECS,
    WAITLIST_KEY, WAITLIST_POW_DIFFICULTY, WAITLIST_RATE_LIMIT_MAX, WAITLIST_RATE_LIMIT_WINDOW_SECS,
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
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub code_expires: u64,
    #[serde(default)]
    pub attempts: u32,
}

#[derive(Debug, PartialEq)]
pub enum JoinStatus {
    Created,
    Resubscribed,
    Already,
    Cooldown,
}

#[derive(Debug, PartialEq)]
pub enum ConfirmStatus {
    Confirmed,
    AlreadyConfirmed,
    WrongCode,
    Expired,
    TooManyAttempts,
    NotFound,
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

    /// Random-enough 6 digit code: secret + email + clock nanos through sha256.
    fn gen_code(&self, email: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let digest = Sha256::digest(format!("{}|{}|{}", self.secret, email, nanos).as_bytes());
        let n = u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
        ]) % 1_000_000;

        format!("{:06}", n)
    }

    /// Start/resume a subscription: (re)generate the confirmation code and email it.
    /// Returns (status, code) — code is empty when nothing was sent (cooldown / already confirmed).
    pub fn join(&self, email: &str, ip: String) -> (JoinStatus, String) {
        let ts = now();

        match self.db.get(email) {
            Ok(Some(bytes)) => {
                let mut sub: Subscriber = serde_json::from_slice(&bytes).unwrap_or(Subscriber {
                    id: 0,
                    ts,
                    ip,
                    unsubscribed: false,
                    last_email: 0,
                    confirmed: false,
                    code: String::new(),
                    code_expires: 0,
                    attempts: 0,
                });

                if sub.confirmed && !sub.unsubscribed {
                    return (JoinStatus::Already, String::new());
                }

                if ts.saturating_sub(sub.last_email) < WAITLIST_EMAIL_COOLDOWN_SECS
                    && !sub.unsubscribed
                {
                    return (JoinStatus::Cooldown, String::new());
                }

                let code = self.gen_code(email);
                sub.unsubscribed = false;
                sub.confirmed = false;
                sub.last_email = ts;
                sub.code = code.clone();
                sub.code_expires = ts + WAITLIST_CODE_TTL_SECS;
                sub.attempts = 0;

                let status = if sub.id == 0 {
                    sub.id = (self.db.len() + 1) as u64;
                    JoinStatus::Created
                } else {
                    JoinStatus::Resubscribed
                };

                let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
                (status, code)
            }
            _ => {
                let code = self.gen_code(email);
                let sub = Subscriber {
                    id: (self.db.len() + 1) as u64,
                    ts,
                    ip,
                    unsubscribed: false,
                    last_email: ts,
                    confirmed: false,
                    code: code.clone(),
                    code_expires: ts + WAITLIST_CODE_TTL_SECS,
                    attempts: 0,
                };
                let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
                (JoinStatus::Created, code)
            }
        }
    }

    /// Verify the emailed code. Only the mailbox owner can pass.
    pub fn confirm(&self, email: &str, code: &str) -> ConfirmStatus {
        let bytes = match self.db.get(email) {
            Ok(Some(b)) => b,
            _ => return ConfirmStatus::NotFound,
        };
        let mut sub: Subscriber = match serde_json::from_slice(&bytes) {
            Ok(s) => s,
            Err(_) => return ConfirmStatus::NotFound,
        };

        if sub.confirmed {
            return ConfirmStatus::AlreadyConfirmed;
        }

        let ts = now();

        if ts > sub.code_expires {
            return ConfirmStatus::Expired;
        }

        if sub.attempts >= WAITLIST_CODE_MAX_ATTEMPTS {
            return ConfirmStatus::TooManyAttempts;
        }

        sub.attempts += 1;

        if sub.code != code {
            let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
            return ConfirmStatus::WrongCode;
        }

        sub.confirmed = true;
        sub.code.clear();
        let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
        ConfirmStatus::Confirmed
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
        let (mut total, mut confirmed, mut unsubscribed, mut last7d) = (0u64, 0u64, 0u64, 0u64);
        let week_ago = now() - 7 * 24 * 3600;

        for entry in self.db.iter().flatten() {
            if let Ok(sub) = serde_json::from_slice::<Subscriber>(&entry.1) {
                total += 1;
                if sub.confirmed {
                    confirmed += 1;
                }
                if sub.unsubscribed {
                    unsubscribed += 1;
                }
                if sub.ts >= week_ago {
                    last7d += 1;
                }
            }
        }

        json!({
            "total": total,
            "confirmed": confirmed,
            "unsubscribed": unsubscribed,
            "last7d": last7d
        })
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
        assert!(valid_email(&normalize_email("  USER@Example.COM ")));
        assert!(!valid_email("user@localhost"));
        assert!(!valid_email("not-an-email"));
        assert!(!valid_email("a @b.c"));
        assert!(!valid_email("@b.c"));
    }

    #[test]
    fn join_confirm_flow() {
        std::env::set_var("WAITLIST_SECRET", "unit-test-secret");
        let wl = Waitlist::new("/tmp/wl-unit-flow");

        let email = "unit-flow@test.io";
        let _ = wl.db.remove(email);

        let (status, code) = wl.join(email.into(), "1.2.3.4".into());
        assert_eq!(status, JoinStatus::Created);
        assert_eq!(code.len(), 6);

        assert_eq!(wl.confirm(email, "000000"), ConfirmStatus::WrongCode);
        assert_eq!(wl.confirm(email, "123456"), ConfirmStatus::WrongCode);
        assert_eq!(wl.confirm(email, &code), ConfirmStatus::Confirmed);
        assert_eq!(wl.confirm(email, &code), ConfirmStatus::AlreadyConfirmed);
    }

    #[test]
    fn confirm_attempt_limit() {
        std::env::set_var("WAITLIST_SECRET", "unit-test-secret");
        let wl = Waitlist::new("/tmp/wl-unit-attempts");

        let email = "unit-attempts@test.io";
        let _ = wl.db.remove(email);

        let (_, code) = wl.join(email.into(), "1.2.3.4".into());

        for _ in 0..5 {
            assert_eq!(wl.confirm(email, "111111"), ConfirmStatus::WrongCode);
        }

        assert_eq!(wl.confirm(email, "111111"), ConfirmStatus::TooManyAttempts);
        assert_eq!(wl.confirm(email, &code), ConfirmStatus::TooManyAttempts);
    }
}
