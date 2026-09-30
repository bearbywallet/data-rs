use crate::models::waitlist::{normalize_email, valid_email, JoinStatus, Waitlist};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{header, Request, Response, StatusCode};
use serde_json::{json, Value};
use std::{collections::HashMap, net::IpAddr, sync::Arc};

use crate::components::email::Emailer;

const CORS_METHODS: &str = "GET, POST, OPTIONS";
const CORS_HEADERS: &str = "Content-Type, Authorization";

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
    // Direct socket peer; behind relayd it is 127.0.0.1 — then take the real
    // client from X-Forwarded-For (relayd injects it via $REMOTE_ADDR).
    let socket_ip: IpAddr = peer.parse().unwrap_or(IpAddr::from([127, 0, 0, 1]));
    let ip: IpAddr = if socket_ip.is_loopback() {
        req.headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|xff| xff.split(',').next())
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(socket_ip)
    } else {
        socket_ip
    };

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

    let email = normalize_email(value.get("email").and_then(|v| v.as_str()).unwrap_or_default());
    let nonce = value
        .get("nonce")
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

    // Same "ok" for every post-pow outcome: no email enumeration.
    let status = waitlist.join(&email, ip.to_string());

    if status != JoinStatus::Cooldown {
        let base =
            std::env::var("PUBLIC_API_URL").unwrap_or_else(|_| "https://api.bearby.io".to_string());
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("email", &email)
            .append_pair("token", &waitlist.token(&email))
            .finish();

        emailer
            .send_welcome(&email, &format!("{}/api/v1/waitlist/unsubscribe?{}", base, query))
            .await;
    }

    json_res(StatusCode::OK, json!({ "code": 0, "message": "ok" }))
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

    json_res(StatusCode::OK, json!({ "code": 0, "data": waitlist.stats() }))
}
