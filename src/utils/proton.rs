use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;

/// Well-known Proton Mail domains.
const PROTON_DOMAINS: [&str; 4] = ["proton.me", "protonmail.com", "pm.me", "protonmail.ch"];

/// MX hosts for custom domains hosted on Proton end with one of these.
const PROTON_MX_SUFFIXES: [&str; 5] = [
    "proton.me",
    "protonmail.ch",
    "protonmail.com",
    "protonmail.net",
    "proton.ch",
];

/// DNS-over-HTTPS JSON resolvers (RFC 8484 is binary; these accept `application/dns-json`).
const DOH_URLS: [&str; 2] = [
    "https://dns.google/resolve",
    "https://cloudflare-dns.com/dns-query",
];

fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Cannot build DNS-over-HTTPS client")
    })
}

/// Exact match against the well-known Proton domains.
pub fn is_proton_domain(domain: &str) -> bool {
    PROTON_DOMAINS.contains(&domain.trim().to_lowercase().as_str())
}

/// `data` is one DoH MX record, e.g. `"10 mail.protonmail.ch."`.
fn mx_is_proton(data: &str) -> bool {
    let Some(host) = data.split_whitespace().last() else {
        return false;
    };

    let host = host.trim_end_matches('.').to_lowercase();

    PROTON_MX_SUFFIXES
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

/// True when the email is served by Proton Mail: either the domain is a
/// well-known Proton domain, or its MX records point to Proton servers
/// (a custom domain can be hosted on Proton, so the domain alone is not
/// enough). DNS/network failures fail open — returns false.
pub async fn is_proton_email(email: &str) -> bool {
    let Some(domain) = email.rsplit('@').next().map(str::trim) else {
        return false;
    };

    if is_proton_domain(domain) {
        return true;
    }

    let domain = domain.to_lowercase();

    for url in DOH_URLS {
        let response = match client()
            .get(url)
            .query(&[("name", domain.as_str()), ("type", "MX")])
            .header("accept", "application/dns-json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => continue,
        };

        let json: serde_json::Value = match response.json().await {
            Ok(j) => j,
            Err(_) => continue,
        };

        if let Some(records) = json.get("Answer").and_then(|a| a.as_array()) {
            if records
                .iter()
                .filter_map(|r| r.get("data").and_then(|d| d.as_str()))
                .any(mx_is_proton)
            {
                return true;
            }
        }

        // Got a valid resolver answer (even empty): settled.
        return false;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_proton_domains() {
        for d in PROTON_DOMAINS {
            assert!(is_proton_domain(d));
            assert!(is_proton_domain(&d.to_uppercase()));
        }

        assert!(!is_proton_domain("gmail.com"));
        assert!(!is_proton_domain("notproton.me"));
        assert!(!is_proton_domain("spam.me"));
    }

    #[test]
    fn mx_records() {
        assert!(mx_is_proton("10 mail.protonmail.ch."));
        assert!(mx_is_proton("20 mail.proton.me."));
        assert!(mx_is_proton("10 mx.custom.protonmail.com."));

        assert!(!mx_is_proton("10 aspmx.l.google.com."));
        // No partial suffix matches ("spam.me" is not "pm.me").
        assert!(!mx_is_proton("5 mail.spam.me."));
    }
}
