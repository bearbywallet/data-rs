use crate::config::waitlist::{
    WAITLIST_CODE_MAX_ATTEMPTS, WAITLIST_CODE_TTL_SECS, WAITLIST_EMAIL_COOLDOWN_SECS, WAITLIST_KEY,
    WAITLIST_NOTICE_VERSION, WAITLIST_POW_DIFFICULTY, WAITLIST_RATE_LIMIT_MAX,
    WAITLIST_RATE_LIMIT_WINDOW_SECS, WAITLIST_SOURCE_MAX_LEN,
};
use crate::utils::geo::suggested_country;
use log::info;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sled::{Db, Tree};
use std::{
    collections::{HashMap, HashSet},
    io::{Error, ErrorKind},
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Subscriber {
    pub id: u64,
    pub ts: u64,
    /// Signup cycle the record is currently on; 1-based, incremented only on
    /// resubscribe after an explicit unsubscribe (retries are idempotent).
    #[serde(default)]
    pub cycle: u32,
    /// Original signup time; never reset so first-contact history survives.
    #[serde(default)]
    pub created_ts: u64,
    /// Start time of the CURRENT cycle (== created_ts for cycle 1).
    #[serde(default)]
    pub cycle_ts: u64,
    /// Cycle whose verification-email send was already counted
    /// (one counted send per cycle; in-cycle resends are not re-counted).
    #[serde(default)]
    pub sent_cycle: u32,
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
    /// Self-reported country of residence (ISO 3166-1 alpha-2), required to qualify.
    #[serde(default)]
    pub country: String,
    /// utm_source captured at signup; empty means direct_or_unattributed.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub confirmed_at: u64,
    #[serde(default)]
    pub country_at: u64,
    #[serde(default)]
    pub consent_ts: u64,
    #[serde(default)]
    pub notice_version: String,
    /// Purpose-specific opaque token for the bearby.io unsubscribe link.
    #[serde(default)]
    pub unsub_token: String,
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
    /// Counters live in a separate tree so email keys and db.len() stay clean.
    meta: Tree,
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
        let secret = std::env::var("WAITLIST_SECRET").expect("ENV var WAITLIST_SECRET is required");
        let db = sled::open(format!("{}/{}", db_path, WAITLIST_KEY))
            .expect("Cannot waitlist open database.");
        let meta = db
            .open_tree(b"WAITLIST_META")
            .expect("Cannot open waitlist meta tree");

        info!("WAITLIST: loaded, {} subscribers", db.len());

        Waitlist {
            db,
            meta,
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

    /// Opaque unsubscribe token, generated once per subscriber and stored.
    fn gen_unsub_token(&self, email: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let digest =
            Sha256::digest(format!("unsub|{}|{}|{}", self.secret, email, nanos).as_bytes());

        hex::encode(digest)[..32].to_string()
    }

    /// Start/resume a subscription: (re)generate the confirmation code and email it.
    /// Returns (status, code) — code is empty when nothing was sent (cooldown / already confirmed).
    pub fn join(&self, email: &str, ip: String, source: &str) -> (JoinStatus, String) {
        let ts = now();

        match self.db.get(email) {
            Ok(Some(bytes)) => {
                let mut sub: Subscriber = serde_json::from_slice(&bytes).unwrap_or(Subscriber {
                    id: 0,
                    ts,
                    cycle: 0,
                    created_ts: 0,
                    cycle_ts: 0,
                    sent_cycle: 0,
                    ip,
                    unsubscribed: false,
                    last_email: 0,
                    confirmed: false,
                    code: String::new(),
                    code_expires: 0,
                    attempts: 0,
                    country: String::new(),
                    source: String::new(),
                    confirmed_at: 0,
                    country_at: 0,
                    consent_ts: ts,
                    notice_version: WAITLIST_NOTICE_VERSION.into(),
                    unsub_token: String::new(),
                });

                // Confirmed subscriber: informational duplicate. Keep the record
                // as is (no second email, no confirmation reset), count the
                // duplicate attempt for the readout.
                if sub.confirmed && !sub.unsubscribed {
                    self.bump_duplicates();
                    return (JoinStatus::Already, String::new());
                }

                if ts.saturating_sub(sub.last_email) < WAITLIST_EMAIL_COOLDOWN_SECS
                    && !sub.unsubscribed
                {
                    return (JoinStatus::Cooldown, String::new());
                }

                // Legacy records predate the cycle fields: backfill as cycle 1.
                if sub.cycle == 0 {
                    sub.cycle = 1;
                    if sub.created_ts == 0 {
                        sub.created_ts = sub.ts;
                    }
                    if sub.cycle_ts == 0 {
                        sub.cycle_ts = sub.ts;
                    }
                }

                let code = self.gen_code(email);
                // A new cycle is only opened after an explicit unsubscribe;
                // retries/resends stay inside the current cycle.
                let new_cycle = sub.unsubscribed;
                sub.unsubscribed = false;
                sub.confirmed = false;
                // The previous cycle's confirmation belongs to that cycle; a
                // new cycle must qualify again (code + country together).
                sub.confirmed_at = 0;
                sub.country.clear();
                sub.country_at = 0;
                sub.last_email = ts;
                sub.code = code.clone();
                sub.code_expires = ts + WAITLIST_CODE_TTL_SECS;
                sub.attempts = 0;
                if new_cycle {
                    // Per-cycle attribution: every new cycle records the
                    // source it was (re)joined with. Inside one cycle the
                    // first source still wins.
                    sub.source = sanitize_source(source);
                    sub.cycle = sub.cycle.saturating_add(1);
                    sub.cycle_ts = ts;
                    self.bump_cycles_added();
                } else if sub.source.is_empty() {
                    sub.source = sanitize_source(source);
                }
                if sub.sent_cycle == 0 {
                    sub.sent_cycle = sub.cycle; // legacy records: already emailed
                }
                sub.consent_ts = ts;
                sub.notice_version = WAITLIST_NOTICE_VERSION.into();

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
                    cycle: 1,
                    created_ts: ts,
                    cycle_ts: ts,
                    sent_cycle: 0,
                    ip,
                    unsubscribed: false,
                    last_email: ts,
                    confirmed: false,
                    code: code.clone(),
                    code_expires: ts + WAITLIST_CODE_TTL_SECS,
                    attempts: 0,
                    country: String::new(),
                    source: sanitize_source(source),
                    confirmed_at: 0,
                    country_at: 0,
                    consent_ts: ts,
                    notice_version: WAITLIST_NOTICE_VERSION.into(),
                    unsub_token: self.gen_unsub_token(email),
                };
                let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
                (JoinStatus::Created, code)
            }
        }
    }

    /// Verify the emailed code AND the required country of residence. Only a
    /// valid code plus country creates the qualified record (and triggers the
    /// welcome email in the router).
    pub fn complete(&self, email: &str, code: &str, country: &str) -> ConfirmStatus {
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
        sub.confirmed_at = ts;
        sub.country = country.to_uppercase();
        sub.country_at = ts;
        sub.code.clear();

        if sub.unsub_token.is_empty() {
            sub.unsub_token = self.gen_unsub_token(email);
        }

        let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
        ConfirmStatus::Confirmed
    }

    /// The stored purpose-specific unsubscribe token; legacy records get one
    /// lazily so every welcome email carries a working link.
    pub fn unsub_token(&self, email: &str) -> Option<String> {
        let bytes = self.db.get(email).ok()??;
        let mut sub: Subscriber = serde_json::from_slice(&bytes).ok()?;

        if sub.unsub_token.is_empty() {
            sub.unsub_token = self.gen_unsub_token(email);
            let _ = self.db.insert(email, serde_json::to_vec(&sub).unwrap());
        }

        Some(sub.unsub_token)
    }

    pub fn unsubscribe_by_token(&self, token: &str) -> bool {
        if token.len() != 32 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }

        for entry in self.db.iter().flatten() {
            if let Ok(mut sub) = serde_json::from_slice::<Subscriber>(&entry.1) {
                if sub.unsub_token == token {
                    sub.unsubscribed = true;
                    let _ = self.db.insert(
                        String::from_utf8_lossy(&entry.0).as_ref(),
                        serde_json::to_vec(&sub).unwrap(),
                    );
                    return true;
                }
            }
        }

        false
    }

    fn bump_meta(&self, key: &[u8]) {
        let _ = self.meta.fetch_and_update(key, |old| {
            let n = old
                .and_then(|b| <[u8; 8]>::try_from(&b[..]).ok())
                .map(u64::from_be_bytes)
                .unwrap_or(0);
            Some((n + 1).to_be_bytes().to_vec())
        });
    }

    fn meta_counter(&self, key: &[u8]) -> u64 {
        self.meta
            .get(key)
            .ok()
            .flatten()
            .and_then(|b| <[u8; 8]>::try_from(&b[..]).ok())
            .map(u64::from_be_bytes)
            .unwrap_or(0)
    }

    fn bump_duplicates(&self) {
        self.bump_meta(b"duplicates");
    }

    fn duplicates(&self) -> u64 {
        self.meta_counter(b"duplicates")
    }

    /// Resubscribe events: each opens a new cycle for an existing record.
    fn bump_cycles_added(&self) {
        self.bump_meta(b"cycles_added");
    }

    fn cycles_added(&self) -> u64 {
        self.meta_counter(b"cycles_added")
    }

    /// Total signup cycles started = records + resubscribes. Retries inside
    /// a cycle never bump it; legacy data needs no migration.
    pub fn cycles_total(&self) -> u64 {
        (self.db.len() as u64) + self.cycles_added()
    }

    /// Successful verification-email sends (bumped by the join handler after
    /// Emailer::send_code reports SMTP success, once per cycle).
    pub fn bump_emails_sent(&self) {
        self.bump_meta(b"emails_sent");
    }

    fn emails_sent(&self) -> u64 {
        self.meta_counter(b"emails_sent")
    }

    /// Mark the current cycle's send as counted. Returns true when this
    /// cycle had no counted send yet (i.e. the counter should grow).
    pub fn mark_email_sent(&self, email: &str) -> bool {
        let Ok(Some(bytes)) = self.db.get(email) else {
            return false;
        };
        let Ok(mut sub) = serde_json::from_slice::<Subscriber>(&bytes) else {
            return false;
        };
        if sub.sent_cycle == sub.cycle {
            return false;
        }
        sub.sent_cycle = sub.cycle;
        if self.db.insert(email, serde_json::to_vec(&sub).unwrap()).is_err() {
            return false;
        }
        self.bump_emails_sent();
        true
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

    /// Everything the waitlist can report: totals, time windows, breakdowns by
    /// country/source/notice-version/email-domain, an IP-estimated geo split
    /// and a 30-day daily event series. Consumed by GET /api/v1/waitlist/stats.
    pub fn stats(&self) -> serde_json::Value {
        let now_ts = now();
        let (cut_24h, cut_7d, cut_30d) = (
            now_ts - 24 * 3600,
            now_ts - 7 * 24 * 3600,
            now_ts - 30 * 24 * 3600,
        );

        // Address states are mutually exclusive latest-action buckets:
        // unsubscribed > qualified > pending; everything else is incomplete.
        let (mut total, mut qualified, mut unsubscribed, mut pending) =
            (0u64, 0u64, 0u64, 0u64);
        let mut qualified_last7d = 0u64;
        let (mut last24h, mut last7d, mut last30d) = (0u64, 0u64, 0u64);
        let (mut code_attempts, mut first_ts, mut last_ts) = (0u64, u64::MAX, 0u64);
        let mut unique_ips: HashSet<String> = HashSet::new();

        // country: (qualified, unsubscribed); only self-reported outcomes.
        let mut by_country: HashMap<String, (u64, u64)> = HashMap::new();
        // source: (cycles_started, qualified)
        let mut by_source: HashMap<String, (u64, u64)> = HashMap::new();
        let mut by_notice_version: HashMap<String, u64> = HashMap::new();
        let mut by_domain: HashMap<String, u64> = HashMap::new();
        // Estimated signup country resolved from the stored IP (db-ip lookup);
        // this is NOT the self-reported residence.
        let mut estimated_by_country: HashMap<String, u64> = HashMap::new();
        // UTC day -> (cycles_started, qualifications)
        let mut daily: HashMap<String, (u64, u64)> = HashMap::new();

        for entry in self.db.iter().flatten() {
            if let Ok(sub) = serde_json::from_slice::<Subscriber>(&entry.1) {
                total += 1;

                // Mutually exclusive latest-action buckets: an unsubscribe is
                // final for the current cycle, a completed code+country makes
                // the address qualified, a live code keeps it pending.
                let is_unsubscribed = sub.unsubscribed;
                let is_qualified =
                    !is_unsubscribed && sub.confirmed && !sub.country.is_empty();
                let is_pending =
                    !is_unsubscribed && !sub.confirmed && sub.code_expires > now_ts;

                if is_unsubscribed {
                    unsubscribed += 1;
                }

                if is_qualified {
                    qualified += 1;
                }

                if is_pending {
                    pending += 1;
                }

                // Rolling windows count cycles by their start; legacy records
                // without cycle_ts fall back to their signup ts.
                let start_ts = if sub.cycle_ts > 0 { sub.cycle_ts } else { sub.ts };

                if start_ts >= cut_24h {
                    last24h += 1;
                }

                if start_ts >= cut_7d {
                    last7d += 1;
                }

                if start_ts >= cut_30d {
                    last30d += 1;
                }

                if sub.country_at >= cut_7d {
                    qualified_last7d += 1;
                }

                code_attempts += u64::from(sub.attempts);
                unique_ips.insert(sub.ip.clone());
                // first/last cycle = earliest-ever / most recent cycle start;
                // a resubscribe moves last_cycle_started_at forward.
                first_ts = first_ts.min(if sub.created_ts > 0 { sub.created_ts } else { sub.ts });
                last_ts = last_ts.max(start_ts);

                let country_key = if sub.country.is_empty() {
                    "unknown".to_string()
                } else {
                    sub.country.clone()
                };
                // Only self-reported outcomes land in by_country: a stalled
                // address may never have reported a country at all, and an
                // all-zero bucket for it would be noise.
                if is_qualified || is_unsubscribed {
                    let country_entry = by_country.entry(country_key).or_insert((0, 0));

                    if is_qualified {
                        country_entry.0 += 1;
                    }

                    if is_unsubscribed {
                        country_entry.1 += 1;
                    }
                }

                let source_key = if sub.source.is_empty() {
                    "direct_or_unattributed".to_string()
                } else {
                    sub.source.clone()
                };
                // Per-cycle sources are not stored, so every started cycle of
                // an address is attributed to its current source.
                let source_entry = by_source.entry(source_key).or_insert((0, 0));
                source_entry.0 += sub.cycle.max(1) as u64;

                if is_qualified {
                    source_entry.1 += 1;
                }

                *by_notice_version
                    .entry(if sub.notice_version.is_empty() {
                        "unknown".to_string()
                    } else {
                        sub.notice_version.clone()
                    })
                    .or_insert(0) += 1;

                let email = String::from_utf8_lossy(&entry.0);
                let domain = email.rsplit('@').next().unwrap_or("unknown");
                *by_domain.entry(domain.to_string()).or_insert(0) += 1;

                let guessed = sub
                    .ip
                    .parse::<IpAddr>()
                    .ok()
                    .and_then(suggested_country)
                    .unwrap_or_else(|| "unknown".to_string());
                *estimated_by_country.entry(guessed).or_insert(0) += 1;

                // Daily event counts within the 30-day window.
                if start_ts >= cut_30d {
                    daily.entry(date_key(start_ts)).or_insert((0, 0)).0 += 1;
                }

                if sub.country_at >= cut_30d {
                    daily
                        .entry(date_key(sub.country_at))
                        .or_insert((0, 0))
                        .1 += 1;
                }
            }
        }

        // Every day of the window is present so charts have no gaps.
        let mut daily_series = serde_json::Map::new();
        for offset in (0..30).rev() {
            let key = date_key(now_ts - offset * 86_400);
            let (cycles_day, qualified_day) = daily.get(&key).copied().unwrap_or((0, 0));
            daily_series.insert(
                key,
                json!({
                    "cycles_started": cycles_day,
                    "qualified": qualified_day
                }),
            );
        }

        // `incomplete` = started a cycle but stalled: live-code pendings are
        // counted separately, so this is expired-code or legacy code-only.
        let incomplete = total - qualified - pending - unsubscribed;

        json!({
            "generated_at": now_ts,
            "addresses": {
                "total": total,
                "qualified": qualified,
                "pending": pending,
                "incomplete": incomplete,
                "unsubscribed": unsubscribed
            },
            "qualified": qualified,
            "qualified_last7d": qualified_last7d,
            "cycles_started_total": self.cycles_total(),
            "cycles_started_last24h": last24h,
            "cycles_started_last7d": last7d,
            "cycles_started_last30d": last30d,
            "first_cycle_started_at": if total > 0 { first_ts } else { 0 },
            "last_cycle_started_at": last_ts,
            "code_attempts": code_attempts,
            "duplicates": self.duplicates(),
            "unique_ips": unique_ips.len(),
            "verification_email_sent": self.emails_sent(),
            "by_country": by_country.into_iter().map(|(k, v)| (k, json!({
                "qualified": v.0,
                "unsubscribed": v.1
            }))).collect::<serde_json::Map<String, serde_json::Value>>(),
            "by_domain": by_domain,
            "by_notice_version": by_notice_version,
            "by_source": by_source.into_iter().map(|(k, v)| (k, json!({
                "cycles_started": v.0,
                "qualified": v.1
            }))).collect::<serde_json::Map<String, serde_json::Value>>(),
            "daily": daily_series,
            "estimated_by_country": estimated_by_country,
            "definitions": definitions()
        })
    }
}

/// Unix seconds -> "YYYY-MM-DD" (UTC) for the daily stats buckets. Civil-days
/// algorithm (Howard Hinnant); avoids pulling in a date crate.
fn date_key(ts: u64) -> String {
    let z = (ts / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Self-describing metric semantics shipped with every stats payload so
/// consumers never have to guess grain, dedup rules, or time boundaries.
fn definitions() -> serde_json::Value {
    json!({
        "grain": "One record per distinct normalized email address. A signup CYCLE is one address's pending->qualified (or unsubscribed) attempt: retries inside a cycle are idempotent, unsubscribe + resubscribe opens a new cycle on the same address.",
        "addresses": "Mutually exclusive latest-action buckets over distinct addresses; addresses.total == addresses.qualified + addresses.pending + addresses.incomplete + addresses.unsubscribed.",
        "addresses.total": "Distinct email addresses ever signed up. Does NOT grow on retries or resubscriptions.",
        "addresses.qualified": "Addresses whose latest cycle completed with a valid code AND self-reported country together, and that have not unsubscribed since.",
        "addresses.pending": "Addresses whose current code is still unverified and unexpired; they may still qualify.",
        "addresses.incomplete": "Addresses that started a cycle but stalled: code expired unconfirmed, or legacy records confirmed without a country.",
        "addresses.unsubscribed": "Addresses whose latest action was unsubscribe. History is kept; resubscribing opens a new cycle.",
        "cycles_started_total": "Distinct signup cycles started: new addresses + resubscribes after unsubscribe. Retries inside a cycle never increment it.",
        "verification_email_sent": "Cycles whose verification email was accepted by the SMTP relay (counted once per cycle; suppressed in-cycle resends are not counted). This is SENT, not delivered or received.",
        "qualified": "Top-level alias of addresses.qualified: the sole cycle-completion metric (code and country accepted together; no separate code-only stage).",
        "unsubscribed": "Addresses whose latest action was unsubscribe. History is kept; resubscribing opens a new cycle.",
        "duplicates": "Join attempts for already-qualified, active addresses (lifetime counter).",
        "code_attempts": "Wrong code entries recorded during complete() (lifetime sum of per-address attempt counters).",
        "unique_ips": "Distinct signup IPs over all records (lifetime, no window).",
        "cycles_started_last24h": "Cycles started in the rolling 24h before generated_at (anchored on cycle start; a resubscribe moves its cycle forward).",
        "cycles_started_last7d": "Cycles started in the rolling 7 days before generated_at.",
        "cycles_started_last30d": "Cycles started in the rolling 30 days before generated_at.",
        "qualified_last7d": "Addresses qualified (code + country) in the rolling 7 days before generated_at (country_at).",
        "first_cycle_started_at": "Earliest cycle-start timestamp over all addresses (Unix seconds, UTC).",
        "last_cycle_started_at": "Most recent cycle-start timestamp (Unix seconds, UTC); a resubscribe moves it forward.",
        "generated_at": "When this snapshot was built (Unix seconds, UTC).",
        "by_source": "Per lowercase utm_source of the CURRENT cycle: cycles_started counts every started cycle attributed to it, qualified counts current qualified addresses. Empty/invalid source -> direct_or_unattributed. Values are filtered to [a-z0-9_-] and length-capped.",
        "by_country": "Self-reported country of residence, limited to qualified and unsubscribed addresses; pending/incomplete addresses may lack a self-reported country.",
        "by_domain": "Distinct addresses per email domain.",
        "by_notice_version": "Addresses per consent notice version shown at signup.",
        "estimated_by_country": "Addresses with a GeoIP-guessed country from the signup IP (heuristic, not self-reported).",
        "daily": "UTC calendar days for the last 30 days: cycles_started = cycles started that day; qualified = code+country accepted that day (country_at). Buckets follow current record timestamps, so a resubscribed address re-enters the funnel; rolling-window fields may differ from calendar-day sums by design.",
        "nesting": "cycles_started_total >= verification_email_sent >= qualified (funnel stages, not additive totals). addresses.* buckets are mutually exclusive and sum to addresses.total."
    })
}

/// utm_source: lowercase `[a-z0-9_-]`, capped length; anything else is dropped
/// (the signup itself is never rejected because of a bad source).
fn sanitize_source(source: &str) -> String {
    let clean: String = source
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-')
        .take(WAITLIST_SOURCE_MAX_LEN)
        .collect();

    clean
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

        let (status, code) = wl.join(email.into(), "1.2.3.4".into(), "x");
        assert_eq!(status, JoinStatus::Created);
        assert_eq!(code.len(), 6);

        assert_eq!(wl.complete(email, "000000", "US"), ConfirmStatus::WrongCode);
        assert_eq!(wl.complete(email, "123456", "US"), ConfirmStatus::WrongCode);
        assert_eq!(wl.complete(email, &code, "us"), ConfirmStatus::Confirmed);
        assert_eq!(
            wl.complete(email, &code, "US"),
            ConfirmStatus::AlreadyConfirmed
        );

        // Qualification fields are recorded with the completion.
        let sub: Subscriber = serde_json::from_slice(&wl.db.get(email).unwrap().unwrap()).unwrap();
        assert_eq!(sub.country, "US");
        assert!(sub.confirmed_at > 0);
        assert!(sub.country_at > 0);
        assert_eq!(sub.source, "x");
        assert_eq!(sub.unsub_token.len(), 32);
    }

    #[test]
    fn duplicate_join_keeps_confirmation() {
        std::env::set_var("WAITLIST_SECRET", "unit-test-secret");
        let wl = Waitlist::new("/tmp/wl-unit-duplicate");

        let email = "unit-duplicate@test.io";
        let _ = wl.db.remove(email);
        // The meta tree persists between test runs.
        let _ = wl.meta.remove(b"duplicates");

        let (_, code) = wl.join(email.into(), "1.2.3.4".into(), "");
        assert_eq!(wl.complete(email, &code, "DE"), ConfirmStatus::Confirmed);

        // Repeat submission: no new code, record untouched.
        let (status, code) = wl.join(email.into(), "5.6.7.8".into(), "linkedin");
        assert_eq!(status, JoinStatus::Already);
        assert!(code.is_empty());

        let sub: Subscriber = serde_json::from_slice(&wl.db.get(email).unwrap().unwrap()).unwrap();
        assert!(sub.confirmed);
        assert_eq!(sub.country, "DE");
        assert_eq!(sub.source, "");

        let stats = wl.stats();
        assert_eq!(stats["duplicates"], 1);
        assert_eq!(stats["addresses"]["qualified"], 1);
        assert_eq!(stats["qualified"], 1);
    }

    #[test]
    fn first_source_wins() {
        std::env::set_var("WAITLIST_SECRET", "unit-test-secret");
        let wl = Waitlist::new("/tmp/wl-unit-source");

        let email = "unit-source@test.io";
        let _ = wl.db.remove(email);

        wl.join(email.into(), "1.2.3.4".into(), "X");
        let sub: Subscriber = serde_json::from_slice(&wl.db.get(email).unwrap().unwrap()).unwrap();
        assert_eq!(sub.source, "x");
    }

    #[test]
    fn unsubscribe_by_stored_token() {
        std::env::set_var("WAITLIST_SECRET", "unit-test-secret");
        let wl = Waitlist::new("/tmp/wl-unit-unsub");

        let email = "unit-unsub@test.io";
        let _ = wl.db.remove(email);

        let (_, code) = wl.join(email.into(), "1.2.3.4".into(), "");
        assert_eq!(wl.complete(email, &code, "SG"), ConfirmStatus::Confirmed);

        let token = wl.unsub_token(email).unwrap();
        assert!(!wl.unsubscribe_by_token("short"));
        assert!(!wl.unsubscribe_by_token(&"g".repeat(32)));
        assert!(wl.unsubscribe_by_token(&token));

        let sub: Subscriber = serde_json::from_slice(&wl.db.get(email).unwrap().unwrap()).unwrap();
        assert!(sub.unsubscribed);
    }

    #[test]
    fn resubscribe_opens_new_cycle_with_own_source() {
        std::env::set_var("WAITLIST_SECRET", "unit-test-secret");
        let wl = Waitlist::new("/tmp/wl-unit-cycle");

        let email = "unit-cycle@test.io";
        let _ = wl.db.remove(email);
        for key in ["duplicates", "cycles_added", "emails_sent"] {
            let _ = wl.meta.remove(key.as_bytes());
        }

        // Cycle 1: join, complete (code+country together), source "X"->"x".
        let (st, code) = wl.join(email.into(), "1.2.3.4".into(), "X");
        assert_eq!(st, JoinStatus::Created);
        assert_eq!(wl.cycles_total(), 1);
        assert_eq!(wl.complete(email, &code, "US"), ConfirmStatus::Confirmed);
        // One counted verification send per cycle.
        assert!(wl.mark_email_sent(email));
        assert!(!wl.mark_email_sent(email));

        // Unsubscribe, then resubscribe -> NEW cycle, new attribution.
        let token = wl.unsub_token(email).unwrap();
        assert!(wl.unsubscribe_by_token(&token));

        let (st2, code2) = wl.join(email.into(), "5.6.7.8".into(), "Tyron-Email");
        assert_eq!(st2, JoinStatus::Resubscribed);
        assert!(!code2.is_empty());
        assert_eq!(wl.cycles_total(), 2);

        let sub: Subscriber =
            serde_json::from_slice(&wl.db.get(email).unwrap().unwrap()).unwrap();
        assert_eq!(sub.cycle, 2);
        // Same-second joins make timestamps equal; the cycle number is the
        // meaningful discriminator here.
        assert!(sub.created_ts <= sub.cycle_ts && sub.cycle_ts <= sub.ts);
        assert!(!sub.confirmed); // must qualify again
        assert_eq!(sub.confirmed_at, 0);
        assert!(sub.country.is_empty());
        assert_eq!(sub.attempts, 0);
        assert_eq!(sub.source, "tyron-email"); // per-cycle attribution

        // Address-level total is unchanged; funnel counters stay nested.
        let addresses = wl.db.len() as u64;
        let stats = wl.stats();
        assert_eq!(stats["addresses"]["total"], addresses);
        assert_eq!(stats["cycles_started_total"], 2);
        assert_eq!(stats["addresses"]["qualified"], 0); // reset on resubscribe

        // New cycle's send is counted once, not twice.
        assert!(wl.mark_email_sent(email));
        assert!(!wl.mark_email_sent(email));

        // An in-cycle retry is fully idempotent: cooldown-rejected, so no
        // cycle bump, no record change, no counted send.
        let (st3, code3) = wl.join(email.into(), "5.6.7.8".into(), "tyron-email");
        assert_eq!(st3, JoinStatus::Cooldown);
        assert!(code3.is_empty());
        assert_eq!(wl.cycles_total(), 2);

        // The abandoned first cycle must not leave a phantom completion
        // in the daily buckets (aggregate vs daily reconciliation).
        let stats2 = wl.stats();
        let daily_qualified: u64 = stats2["daily"]
            .as_object()
            .unwrap()
            .values()
            .map(|d| d["qualified"].as_u64().unwrap())
            .sum();
        assert_eq!(daily_qualified, 0);
    }

    #[test]
    fn confirm_attempt_limit() {
        std::env::set_var("WAITLIST_SECRET", "unit-test-secret");
        let wl = Waitlist::new("/tmp/wl-unit-attempts");

        let email = "unit-attempts@test.io";
        let _ = wl.db.remove(email);

        let (_, code) = wl.join(email.into(), "1.2.3.4".into(), "");

        for _ in 0..5 {
            assert_eq!(wl.complete(email, "111111", "US"), ConfirmStatus::WrongCode);
        }

        assert_eq!(
            wl.complete(email, "111111", "US"),
            ConfirmStatus::TooManyAttempts
        );
        assert_eq!(
            wl.complete(email, "111111", "US"),
            ConfirmStatus::TooManyAttempts
        );
    }

    #[test]
    fn date_key_formats_utc_days() {
        assert_eq!(date_key(0), "1970-01-01");
        assert_eq!(date_key(86_399), "1970-01-01");
        assert_eq!(date_key(86_400), "1970-01-02");
        assert_eq!(date_key(1_760_000_000), "2025-10-09");
    }

    #[test]
    fn stats_full_breakdown() {
        std::env::set_var("WAITLIST_SECRET", "unit-test-secret");
        let wl = Waitlist::new("/tmp/wl-unit-stats");

        let email = "unit-stats@test.io";
        let _ = wl.db.remove(email);
        let pending_email = "unit-stats-pending@test.io";
        let _ = wl.db.remove(pending_email);
        let _ = wl.meta.remove(b"duplicates");

        let (_, code) = wl.join(email.into(), "1.2.3.4".into(), "linkedin");
        assert_eq!(wl.complete(email, &code, "US"), ConfirmStatus::Confirmed);

        // Second record: joined, never confirmed, code still alive.
        wl.join(pending_email.into(), "5.6.7.8".into(), "");

        let stats = wl.stats();

        assert_eq!(stats["addresses"]["total"], 2);
        assert_eq!(stats["addresses"]["qualified"], 1);
        assert_eq!(stats["addresses"]["pending"], 1);
        assert_eq!(stats["addresses"]["incomplete"], 0);
        assert_eq!(stats["addresses"]["unsubscribed"], 0);
        assert_eq!(stats["qualified"], 1);
        assert_eq!(stats["cycles_started_last24h"], 2);
        assert_eq!(stats["cycles_started_last7d"], 2);
        assert_eq!(stats["cycles_started_last30d"], 2);
        assert_eq!(stats["qualified_last7d"], 1);
        assert_eq!(stats["unique_ips"], 2);
        assert!(stats["first_cycle_started_at"].as_u64().unwrap() > 0);
        assert!(stats["last_cycle_started_at"].as_u64().unwrap() > 0);

        let us = &stats["by_country"]["US"];
        assert_eq!(us["qualified"], 1);
        assert_eq!(us["unsubscribed"], 0);
        assert_eq!(stats["by_country"].get("unknown"), None);

        assert_eq!(stats["by_source"]["linkedin"]["cycles_started"], 1);
        assert_eq!(stats["by_source"]["linkedin"]["qualified"], 1);
        assert_eq!(stats["by_source"]["direct_or_unattributed"]["cycles_started"], 1);

        assert_eq!(stats["by_domain"]["test.io"], 2);
        assert_eq!(stats["by_notice_version"].as_object().unwrap().len(), 1);

        // Estimated geo must account for every signup IP.
        let estimated: u64 = stats["estimated_by_country"]
            .as_object()
            .unwrap()
            .values()
            .map(|v| v.as_u64().unwrap())
            .sum();
        assert_eq!(estimated, 2);

        let daily = stats["daily"].as_object().unwrap();
        assert_eq!(daily.len(), 30);
        let today = daily.get(&date_key(now())).unwrap();
        assert_eq!(today["cycles_started"], 2);
        assert_eq!(today["qualified"], 1);
    }
}
