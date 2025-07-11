use bytes::Bytes;
use http_body_util::Full;
use hyper::{
    header::{self, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN},
    http::HeaderValue,
    Request, Response,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
enum StakingPoolType {
    LIQUID,
    NORMAL,
}

#[derive(Debug, Serialize)]
struct EvmPool<'a> {
    address: &'a str,
    token_address: &'a str,
    name: &'a str,
    pool_type: StakingPoolType,
    token_decimals: u8,
    token_symbol: &'a str,
}

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

const MAINNET_POOLS: [EvmPool; 11] = [
    EvmPool {
        address: "0x1f0e86Bc299Cc66df2e5512a7786C3F528C0b5b6",
        token_address: "0x8a2afD8Fe79F8C694210eB71f4d726Fc8cAFdB31",
        name: "Amazing Pool (Avely)",
        pool_type: StakingPoolType::LIQUID,
        token_decimals: 18,
        token_symbol: "aZIL",
    },
    EvmPool {
        address: "0x1311059DD836D7000Dc673eA4Cc834fe04e9933C",
        token_address: "0x8E3073b22F670d3A09C66D0Abb863f9E358402d2",
        name: "Encapsulate",
        pool_type: StakingPoolType::LIQUID,
        token_decimals: 18,
        token_symbol: "encapZIL",
    },
    EvmPool {
        address: "0x18925cE668b2bBC26dfE6F630F5C285D46b937AE",
        token_address: "0x0000000000000000000000000000000000000000",
        name: "CEX.IO",
        pool_type: StakingPoolType::NORMAL,
        token_decimals: 18,
        token_symbol: "ZIL",
    },
    EvmPool {
        address: "0x8776F1135b3583DbaE79C8f7268a7e0d4C16462c",
        token_address: "0x0000000000000000000000000000000000000000",
        name: "DTEAM",
        pool_type: StakingPoolType::NORMAL,
        token_decimals: 18,
        token_symbol: "ZIL",
    },
    EvmPool {
        address: "0x63CE81C023Bb9F8A6FFA08fcF48ba885C21FcFBC",
        token_address: "0x0000000000000000000000000000000000000000",
        name: "Luganodes",
        pool_type: StakingPoolType::NORMAL,
        token_decimals: 18,
        token_symbol: "ZIL",
    },
    EvmPool {
        address: "0x715F94264057df97e772ebDFE2c94A356244F142",
        token_address: "0x0000000000000000000000000000000000000000",
        name: "Stakefish",
        pool_type: StakingPoolType::NORMAL,
        token_decimals: 18,
        token_symbol: "ZIL",
    },
    EvmPool {
        address: "0xBD6ca237f30A86eea8CF9bF869677F3a0496a990",
        token_address: "0x3B78f66651E2eCAbf13977817848F82927a17DcF",
        name: "Lithium Digital",
        pool_type: StakingPoolType::LIQUID,
        token_decimals: 18,
        token_symbol: "litZil",
    },
    EvmPool {
        address: "0xCDb0B23Db1439b28689844FD093C478d73C0786A",
        token_address: "0x0000000000000000000000000000000000000000",
        name: "2ZilMoon (Make Zilliqa Great Again)",
        pool_type: StakingPoolType::NORMAL,
        token_decimals: 18,
        token_symbol: "ZIL",
    },
    EvmPool {
        address: "0x068C599686d2511AD709B8b4C578549A65D19491",
        token_address: "0x0000000000000000000000000000000000000000",
        name: "AlphaZil (former Ezil)",
        pool_type: StakingPoolType::NORMAL,
        token_decimals: 18,
        token_symbol: "ZIL",
    },
    EvmPool {
        address: "0xF35E17333Bd4AD7b11e18f750AFbccE14e4101b7",
        token_address: "0x0000000000000000000000000000000000000000",
        name: "Moonlet",
        pool_type: StakingPoolType::NORMAL,
        token_decimals: 18,
        token_symbol: "ZIL",
    },
    EvmPool {
        address: "0x691682FCa60Fa6B702a0a69F60d045c08f404220",
        token_address: "0xc85b0db68467dede96A7087F4d4C47731555cA7A",
        name: "PlunderSwap Staked ZIL",
        pool_type: StakingPoolType::NORMAL,
        token_decimals: 18,
        token_symbol: "pZIL",
    },
];

pub async fn handle_get_pools(
    _req: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let json = serde_json::to_string(&MAINNET_POOLS).unwrap_or_else(|e| {
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

const MAINNET_POOLS_V2: [EvmPoolV2; 12] = [
    EvmPoolV2 {
        address: "0x1f0e86Bc299Cc66df2e5512a7786C3F528C0b5b6",
        name: "ZilPay Pool (Avely)",
        token: Some(Token {
            name: "Amazing Pool Liquid Staking",
            symbol: "aZIL",
            decimals: 18,
            address: "0x8a2afD8Fe79F8C694210eB71f4d726Fc8cAFdB31",
        }),
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0xCDb0B23Db1439b28689844FD093C478d73C0786A",
        name: "2ZilMoon (Make Zilliqa Great Again)",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0x068C599686d2511AD709B8b4C578549A65D19491",
        name: "AlphaZil (former Ezil)",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0x1311059DD836D7000Dc673eA4Cc834fe04e9933C",
        name: "Encapsulate",
        token: Some(Token {
            name: "Encapsulate Zilliqa",
            symbol: "encapZIL",
            decimals: 18,
            address: "0x8E3073b22F670d3A09C66D0Abb863f9E358402d2",
        }),
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0x18925cE668b2bBC26dfE6F630F5C285D46b937AE",
        name: "CEX.IO",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0x8776F1135b3583DbaE79C8f7268a7e0d4C16462c",
        name: "DTEAM",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0x63CE81C023Bb9F8A6FFA08fcF48ba885C21FcFBC",
        name: "Luganodes",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0x715F94264057df97e772ebDFE2c94A356244F142",
        name: "Stakefish",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0xBD6ca237f30A86eea8CF9bF869677F3a0496a990",
        name: "Lithium Digital",
        token: Some(Token {
            name: "litZil",
            symbol: "litZil",
            decimals: 18,
            address: "0x3B78f66651E2eCAbf13977817848F82927a17DcF",
        }),
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0xF35E17333Bd4AD7b11e18f750AFbccE14e4101b7",
        name: "Moonlet",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: true,
    },
    EvmPoolV2 {
        address: "0x691682FCa60Fa6B702a0a69F60d045c08f404220",
        name: "PlunderSwap",
        token: Some(Token {
            name: "PlunderSwap Staked ZIL",
            symbol: "pZIL",
            decimals: 18,
            address: "0xc85b0db68467dede96A7087F4d4C47731555cA7A",
        }),
        hide: false,
        uptime: 80,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0xBB2Cb8B573Ec1ec4f77953128df7F1d08D9c34DF",
        name: "TorchWallet.io",
        token: Some(Token {
            name: "Torch Liquid ZIL",
            symbol: "tZIL",
            decimals: 18,
            address: "0x9e4E0F7A06E50DA13c78cF8C83E907f792DE54fd",
        }),
        hide: false,
        uptime: 99,
        can_stake: false,
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
