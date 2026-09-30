pub const WAITLIST_KEY: &str = "WAITLIST";

/// Leading zero bits required in sha256(email + nonce).
pub const WAITLIST_POW_DIFFICULTY: u32 = 20;

pub const WAITLIST_RATE_LIMIT_MAX: usize = 10;
pub const WAITLIST_RATE_LIMIT_WINDOW_SECS: u64 = 3600;
pub const WAITLIST_EMAIL_COOLDOWN_SECS: u64 = 3600;
