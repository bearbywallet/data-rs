use db_ip::{include_country_code_database, CountryCode, DbIpDatabase};
use std::net::IpAddr;
use std::sync::LazyLock;

/// Country-level database embedded at build time (db-ip.com lite data, see
/// LICENSE-DBIP for the attribution requirement). Lookups are offline and
/// sub-microsecond; the build needs network access to fetch fresh data.
static COUNTRY_DB: LazyLock<DbIpDatabase<CountryCode>> =
    LazyLock::new(|| include_country_code_database!());

/// Best-effort country guess from the visitor IP. Used ONLY to preselect the
/// country-of-residence select on the website — the user can change it, and it
/// is never stored as residence. Codes outside the assigned ISO list (db-ip
/// uses "ZZ" for reserved/unknown ranges) are dropped.
pub fn suggested_country(ip: IpAddr) -> Option<String> {
    let code = COUNTRY_DB.get(&ip)?.to_string();

    if valid_country(&code) {
        Some(code)
    } else {
        None
    }
}

/// Every officially assigned ISO 3166-1 alpha-2 code.
pub const COUNTRY_CODES: &[&str] = &[
    "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AO", "AQ", "AR", "AS", "AT", "AU", "AW", "AX", "AZ",
    "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ", "BR", "BS",
    "BT", "BV", "BW", "BY", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN",
    "CO", "CR", "CU", "CV", "CW", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE",
    "EG", "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FM", "FO", "FR", "GA", "GB", "GD", "GE", "GF",
    "GG", "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT", "GU", "GW", "GY", "HK", "HM",
    "HN", "HR", "HT", "HU", "ID", "IE", "IL", "IM", "IN", "IO", "IQ", "IR", "IS", "IT", "JE", "JM",
    "JO", "JP", "KE", "KG", "KH", "KI", "KM", "KN", "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC",
    "LI", "LK", "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MH", "MK",
    "ML", "MM", "MN", "MO", "MP", "MQ", "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA",
    "NC", "NE", "NF", "NG", "NI", "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG",
    "PH", "PK", "PL", "PM", "PN", "PR", "PS", "PT", "PW", "PY", "QA", "RE", "RO", "RS", "RU", "RW",
    "SA", "SB", "SC", "SD", "SE", "SG", "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "SS",
    "ST", "SV", "SX", "SY", "SZ", "TC", "TD", "TF", "TG", "TH", "TJ", "TK", "TL", "TM", "TN", "TO",
    "TR", "TT", "TV", "TW", "TZ", "UA", "UG", "UM", "US", "UY", "UZ", "VA", "VC", "VE", "VG", "VI",
    "VN", "VU", "WF", "WS", "YE", "YT", "ZA", "ZM", "ZW",
];

pub fn valid_country(country: &str) -> bool {
    COUNTRY_CODES.contains(&country)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_country_resolves_public_ip() {
        // First range owned by the US (1.0.0.0/8 contains Cloudflare etc.).
        let ip: IpAddr = "8.8.8.8".parse().unwrap();
        assert_eq!(suggested_country(ip).as_deref(), Some("US"));
    }

    #[test]
    fn suggested_country_ignores_loopback() {
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert_eq!(suggested_country(ip), None);
    }

    #[test]
    fn country_list_validates() {
        assert!(valid_country("US"));
        assert!(valid_country("SG"));
        assert!(!valid_country("us"));
        assert!(!valid_country("XX"));
        assert!(!valid_country(""));
    }
}
