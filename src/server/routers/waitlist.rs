use crate::models::waitlist::{normalize_email, valid_email, ConfirmStatus, JoinStatus, Waitlist};
use crate::utils::geo::{suggested_country, valid_country};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{header, Request, Response, StatusCode};
use serde_json::{json, Value};
use std::{collections::HashMap, net::IpAddr, sync::Arc};

use crate::components::email::Emailer;

const CORS_METHODS: &str = "GET, POST, OPTIONS";
const CORS_HEADERS: &str = "Content-Type, Authorization";

/// Public website base URL: email links must point at bearby.io (never at the
/// API host), per the privacy plan.
fn site_url() -> String {
    std::env::var("PUBLIC_SITE_URL").unwrap_or_else(|_| "https://bearby.io".to_string())
}

fn json_res(status: StatusCode, body: Value) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, CORS_METHODS)
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, CORS_HEADERS)
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap();

    Ok(response)
}

fn html_res(status: StatusCode, body: String) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Full::new(Bytes::from(body)))
        .unwrap();

    Ok(response)
}

pub async fn handle_preflight() -> Result<Response<Full<Bytes>>, hyper::Error> {
    let response = Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, CORS_METHODS)
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, CORS_HEADERS)
        .header(header::ACCESS_CONTROL_MAX_AGE, "86400")
        .body(Full::new(Bytes::new()))
        .unwrap();

    Ok(response)
}

fn incorrect_params() -> Result<Response<Full<Bytes>>, hyper::Error> {
    json_res(
        StatusCode::BAD_REQUEST,
        json!({ "code": -2, "message": "Incorrect params" }),
    )
}

/// Resolves the visitor IP: direct socket peer; behind relayd it is 127.0.0.1 —
/// then take the real client from X-Forwarded-For (relayd injects $REMOTE_ADDR).
fn visitor_ip(req: &Request<hyper::body::Incoming>, peer: &str) -> IpAddr {
    let socket_ip: IpAddr = peer.parse().unwrap_or(IpAddr::from([127, 0, 0, 1]));

    if socket_ip.is_loopback() {
        req.headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|xff| xff.split(',').next())
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(socket_ip)
    } else {
        socket_ip
    }
}

fn page_html(title: &str, text: &str, ok: bool) -> String {
    let accent = if ok { "#ac59ff" } else { "#e05c5c" };

    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title></head>
<body style="margin:0;padding:0;background:#0b0910;font-family:Arial,Helvetica,sans-serif;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="min-height:100vh;background:#0b0910;padding:48px 16px;">
<tr><td align="center">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:440px;background:#17131f;border:1px solid #2a2338;border-radius:16px;padding:40px;">
<tr><td style="color:{accent};font-size:12px;font-weight:bold;letter-spacing:2px;text-transform:uppercase;">Bearby Card</td></tr>
<tr><td style="color:#ffffff;font-size:22px;font-weight:bold;padding:14px 0 8px;">{title}</td></tr>
<tr><td style="color:#a89fb8;font-size:15px;line-height:1.6;">{text}</td></tr>
</table>
</td></tr>
</table>
</body></html>"#,
        title = title,
        text = text,
        accent = accent
    )
}

pub async fn handle_join(
    req: Request<hyper::body::Incoming>,
    waitlist: Arc<Waitlist>,
    emailer: Arc<Emailer>,
    peer: String,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let ip = visitor_ip(&req, &peer);

    if waitlist.rate_limit(ip).is_err() {
        return json_res(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "code": -6, "message": "Too many requests" }),
        );
    }

    let body_bytes = match req.collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => return incorrect_params(),
    };

    let value: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return incorrect_params(),
    };

    let email = normalize_email(
        value
            .get("email")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
    );
    let nonce = value
        .get("nonce")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let source = value
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if !valid_email(&email) {
        return incorrect_params();
    }

    if !waitlist.verify_pow(&email, nonce) {
        return json_res(
            StatusCode::BAD_REQUEST,
            json!({ "code": -3, "message": "Invalid proof of work" }),
        );
    }

    // Block Proton Mail before sending anything: known domains, or any custom
    // domain whose MX records point to Proton servers (checked via DoH).
    if crate::utils::proton::is_proton_email(&email).await {
        return json_res(
            StatusCode::BAD_REQUEST,
            json!({ "code": -4, "message": "Using Proton Mail? We're currently experiencing delivery issues with Proton Mail. Please use another email address to join the waitlist for now." }),
        );
    }

    let (status, code) = waitlist.join(&email, ip.to_string(), source);

    if !code.is_empty() {
        emailer.send_code(&email, &code).await;
    }

    // Already-confirmed duplicate: informational amber state on the website,
    // never an error and never a second email.
    if status == JoinStatus::Already {
        return json_res(
            StatusCode::OK,
            json!({ "code": -10, "message": "Already on the waitlist" }),
        );
    }

    // Country guess from the visitor IP — a default for the country select on
    // the website only; never stored as residence.
    let mut body = json!({ "code": 0, "message": "ok" });

    if let Some(country) = suggested_country(ip) {
        body["suggested_country"] = json!(country);
    }

    json_res(StatusCode::OK, body)
}

/// POST /api/v1/waitlist/complete — verifies the emailed code together with the
/// required country of residence and creates the qualified record.
pub async fn handle_complete(
    req: Request<hyper::body::Incoming>,
    waitlist: Arc<Waitlist>,
    emailer: Arc<Emailer>,
    peer: String,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let ip = visitor_ip(&req, &peer);

    if waitlist.rate_limit(ip).is_err() {
        return json_res(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "code": -6, "message": "Too many requests" }),
        );
    }

    let body_bytes = match req.collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => return incorrect_params(),
    };

    let value: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return incorrect_params(),
    };

    let email = normalize_email(
        value
            .get("email")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
    );
    let code = value
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim();
    let country = value
        .get("country")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_uppercase();

    if !valid_email(&email)
        || code.len() != 6
        || !code.chars().all(|c| c.is_ascii_digit())
        || !valid_country(&country)
    {
        return incorrect_params();
    }

    match waitlist.complete(&email, code, &country) {
        ConfirmStatus::Confirmed => {
            if let Some(token) = waitlist.unsub_token(&email) {
                let unsub = format!("{}/waitlist/unsubscribe?token={}", site_url(), token);
                let privacy = format!("{}/privacy-policy#card-waitlist", site_url());

                emailer.send_welcome(&email, &unsub, &privacy).await;
            }

            json_res(StatusCode::OK, json!({ "code": 0, "message": "qualified" }))
        }
        ConfirmStatus::AlreadyConfirmed => {
            json_res(StatusCode::OK, json!({ "code": 0, "message": "confirmed" }))
        }
        ConfirmStatus::WrongCode => json_res(
            StatusCode::FORBIDDEN,
            json!({ "code": -7, "message": "Wrong code" }),
        ),
        ConfirmStatus::Expired => json_res(
            StatusCode::GONE,
            json!({ "code": -8, "message": "Code expired" }),
        ),
        ConfirmStatus::TooManyAttempts => json_res(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "code": -9, "message": "Too many attempts" }),
        ),
        ConfirmStatus::NotFound => incorrect_params(),
    }
}

/// Legacy GET confirm kept for links/clients from before the country step;
/// new clients use POST /complete. Confirms without country (record stays
/// unqualified until the user completes the country step on the website).
pub async fn handle_confirm(
    req: Request<hyper::body::Incoming>,
    waitlist: Arc<Waitlist>,
    emailer: Arc<Emailer>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let query = req.uri().query().unwrap_or_default();
    let pairs: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let email = normalize_email(pairs.get("email").map(String::as_str).unwrap_or_default());
    let code = pairs
        .get("code")
        .map(String::as_str)
        .unwrap_or_default()
        .trim();

    if !valid_email(&email) || code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return incorrect_params();
    }

    let status = waitlist.complete(&email, code, "");

    match status {
        ConfirmStatus::Confirmed => {
            let base = site_url();
            let privacy = format!("{}/privacy-policy#card-waitlist", base);

            if let Some(token) = waitlist.unsub_token(&email) {
                let unsub = format!("{}/waitlist/unsubscribe?token={}", base, token);

                emailer.send_welcome(&email, &unsub, &privacy).await;
            }

            json_res(StatusCode::OK, json!({ "code": 0, "message": "confirmed" }))
        }
        ConfirmStatus::AlreadyConfirmed => {
            json_res(StatusCode::OK, json!({ "code": 0, "message": "confirmed" }))
        }
        ConfirmStatus::WrongCode => json_res(
            StatusCode::FORBIDDEN,
            json!({ "code": -7, "message": "Wrong code" }),
        ),
        ConfirmStatus::Expired => json_res(
            StatusCode::GONE,
            json!({ "code": -8, "message": "Code expired" }),
        ),
        ConfirmStatus::TooManyAttempts => json_res(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "code": -9, "message": "Too many attempts" }),
        ),
        ConfirmStatus::NotFound => incorrect_params(),
    }
}

/// POST /api/v1/waitlist/unsubscribe — token-only body; the website page
/// (/waitlist/unsubscribe) proxies this so no email ever appears in a URL.
pub async fn handle_unsubscribe_token(
    mut req: Request<hyper::body::Incoming>,
    waitlist: Arc<Waitlist>,
    peer: String,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let ip = visitor_ip(&req, &peer);

    if waitlist.rate_limit(ip).is_err() {
        return json_res(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "code": -6, "message": "Too many requests" }),
        );
    }

    let body_bytes = match req.body_mut().collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => return incorrect_params(),
    };

    let value: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return incorrect_params(),
    };

    let token = value
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim();

    if waitlist.unsubscribe_by_token(token) {
        json_res(
            StatusCode::OK,
            json!({ "code": 0, "message": "unsubscribed" }),
        )
    } else {
        json_res(
            StatusCode::FORBIDDEN,
            json!({ "code": -11, "message": "Invalid link" }),
        )
    }
}

pub async fn handle_unsubscribe(
    req: Request<hyper::body::Incoming>,
    waitlist: Arc<Waitlist>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let query = req.uri().query().unwrap_or_default();
    let pairs: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let email = normalize_email(pairs.get("email").map(String::as_str).unwrap_or_default());
    let token = pairs.get("token").map(String::as_str).unwrap_or_default();

    let ok = valid_email(&email) && token == waitlist.token(&email);

    if ok {
        waitlist.unsubscribe(&email);
    }

    let (title, text, status) = if ok {
        (
            "Unsubscribed",
            "You have been unsubscribed from Bearby Card emails.",
            StatusCode::OK,
        )
    } else {
        (
            "Invalid link",
            "This unsubscribe link is invalid. Please use the link from your email.",
            StatusCode::FORBIDDEN,
        )
    };

    html_res(status, page_html(title, text, ok))
}

pub async fn handle_stats(
    req: Request<hyper::body::Incoming>,
    waitlist: Arc<Waitlist>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let access_token = std::env::var("ACCESS_TOKEN").unwrap_or("666".to_string());
    let header_token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    if access_token != header_token {
        return json_res(
            StatusCode::NETWORK_AUTHENTICATION_REQUIRED,
            json!({ "code": -5, "message": "Incorrect auth token" }),
        );
    }

    json_res(
        StatusCode::OK,
        json!({ "code": 0, "data": waitlist.stats() }),
    )
}
