use bytes::Bytes;
use http_body_util::Full;
use hyper::StatusCode;
use hyper::{Request, Response};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::components::email::Emailer;
use crate::models::{currencies::Currencies, dex::Dex, meta::Meta, waitlist::Waitlist};

mod dex;
mod rates;
mod stake;
mod tokens;
mod waitlist;

pub async fn route(
    req: Request<hyper::body::Incoming>,
    meta: Arc<RwLock<Meta>>,
    dex: Arc<RwLock<Dex>>,
    rates: Arc<RwLock<Currencies>>,
    waitlist: Arc<Waitlist>,
    emailer: Arc<Emailer>,
    peer: String,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&hyper::Method::OPTIONS, path) if path.starts_with("/api/v1/waitlist") => {
            waitlist::handle_preflight().await
        }
        (&hyper::Method::POST, "/api/v1/waitlist") => {
            waitlist::handle_join(req, waitlist, emailer, peer).await
        }
        (&hyper::Method::GET, "/api/v1/waitlist/confirm") => {
            waitlist::handle_confirm(req, waitlist, emailer).await
        }
        (&hyper::Method::GET, "/api/v1/waitlist/unsubscribe") => {
            waitlist::handle_unsubscribe(req, waitlist).await
        }
        (&hyper::Method::GET, "/api/v1/waitlist/stats") => {
            waitlist::handle_stats(req, waitlist).await
        }
        (&hyper::Method::GET, "/api/v1/dex") => dex::handle_get_pools(req, meta, dex, rates).await,
        (&hyper::Method::GET, "/api/v1/rates") => rates::handle_get_rates(req, rates).await,
        (&hyper::Method::GET, "/api/v2/stake/pools") => stake::handle_get_poolsv2(req).await,
        (&hyper::Method::GET, "/api/v1/tokens") => tokens::handle_get_tokens(req, meta).await,
        (&hyper::Method::GET, "/api/v1/tokens_evm") => tokens::hanlde_get_evm_tokens().await,
        (&hyper::Method::GET, path) if path.starts_with("/api/v1/token/") => {
            tokens::handle_get_token(req, meta).await
        }
        (&hyper::Method::PUT, path) if path.starts_with("/api/v1/token/") => {
            tokens::handle_update_token(req, meta).await
        }
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from("Not Found")))
            .unwrap()),
    }
}
