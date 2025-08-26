use bytes::Bytes;
use http_body_util::Full;
use hyper::{
    header::{self, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN},
    http::HeaderValue,
    Request, Response,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Token<'a> {
    pub name: &'a str,
    pub symbol: &'a str,
    pub decimals: u8,
    pub address: &'a str,
}

#[derive(Debug, Serialize)]
struct EvmPoolV2<'a> {
    address: &'a str,
    token: Option<Token<'a>>,
    name: &'a str,
    hide: bool,
    uptime: u8,
    can_stake: bool,
}

const MAINNET_POOLS_V2: [EvmPoolV2; 2] = [
    EvmPoolV2 {
        address: "0x1f0e86bc299cc66df2e5512a7786c3f528c0b5b6",
        name: "ZilPay Pool (Avely)",
        token: Some(Token {
            name: "Amazing Pool Liquid Staking",
            symbol: "aZIL",
            decimals: 18,
            address: "0x8a2afd8fe79f8c694210eb71f4d726fc8cafdb31",
        }),
        hide: false,
        uptime: 100,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0xcdb0b23db1439b28689844fd093c478d73c0786a",
        name: "2ZilMoon (Make Zilliqa Great Again)",
        token: None,
        hide: false,
        uptime: 100,
        can_stake: true,
    },
];

pub async fn handle_get_poolsv2(
    _req: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let json = serde_json::to_string(&MAINNET_POOLS_V2).unwrap_or_else(|e| {
        eprintln!("Error serializing pools: {}", e);
        "[]".to_string()
    });

    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(json)))
        .unwrap();

    response
        .headers_mut()
        .insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    response.headers_mut().insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET"),
    );

    Ok(response)
}
