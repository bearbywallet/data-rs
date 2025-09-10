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

const MAINNET_POOLS_V2: [EvmPoolV2; 21] = [
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
    EvmPoolV2 {
        address: "0x068c599686d2511ad709b8b4c578549a65d19491",
        name: "AlphaZil (former Ezil)",
        token: None,
        hide: false,
        uptime: 100,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x1311059dd836d7000dc673ea4cc834fe04e9933c",
        name: "Encapsulate",
        token: Some(Token {
            name: "Encapsulate Zilliqa",
            symbol: "encapZIL",
            decimals: 18,
            address: "0x8e3073b22f670d3a09c66d0abb863f9e358402d2",
        }),
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x8776f1135b3583dbae79c8f7268a7e0d4c16462c",
        name: "DTEAM",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x63ce81c023bb9f8a6ffa08fcf48ba885c21fcfbc",
        name: "Luganodes",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x715f94264057df97e772ebdfe2c94a356244f142",
        name: "Stakefish",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0xbd6ca237f30a86eea8cf9bf869677f3a0496a990",
        name: "Lithium Digital",
        token: Some(Token {
            name: "litZil",
            symbol: "litZil",
            decimals: 18,
            address: "0x3b78f66651e2ecabf13977817848f82927a17dcf",
        }),
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0xf35e17333bd4ad7b11e18f750afbcce14e4101b7",
        name: "Moonlet",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x691682fca60fa6b702a0a69f60d045c08f404220",
        name: "PlunderSwap",
        token: Some(Token {
            name: "PlunderSwap Staked ZIL",
            symbol: "pZIL",
            decimals: 18,
            address: "0xc85b0db68467dede96a7087f4d4c47731555ca7a",
        }),
        hide: false,
        uptime: 100,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0xbb2cb8b573ec1ec4f77953128df7f1d08d9c34df",
        name: "TorchWallet.io",
        token: Some(Token {
            name: "Torch Liquid ZIL",
            symbol: "tZIL",
            decimals: 18,
            address: "0x9e4e0f7a06e50da13c78cf8c83e907f792de54fd",
        }),
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x87297b0b63a0b93d3f7cafa9e0f4c849e92642eb",
        name: "BlackNodes",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0xe5e8158883a37449ae07fe70b69e658766b317fc",
        name: "Shardpool",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x7e3a0aebbf8ec2f12a8a885cd663ee4a490f923f",
        name: "Zillet Staking Pool",
        token: None,
        hide: false,
        uptime: 77,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0xf7f4049e7472fc32805aae5bcce909419a34d254",
        name: "StakeShark",
        token: Some(Token {
            name: "StakeShark Staked ZIL",
            symbol: "shZIL",
            decimals: 18,
            address: "0x737ebf814d2c14fb21e00fd2990afc364c2af506",
        }),
        hide: false,
        uptime: 98,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0xd12340c2d5a26e7f5c469b57ee81ee82c8cb7686",
        name: "Citadel.one",
        token: None,
        hide: false,
        uptime: 89,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x18925ce668b2bbc26dfe6f630f5c285d46b937ae",
        name: "CEX.IO",
        token: None,
        hide: false,
        uptime: 78,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0xe67e119dcdc1168ec8089f4647702a72a0fcbc7f",
        name: "PathrockNetwork",
        token: None,
        hide: false,
        uptime: 99,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x26322705fcbf5d3065707c408b6594912daa3488",
        name: "Cryptech-Hacken",
        token: None,
        hide: false,
        uptime: 90,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0x60571e6c6d55109e6705d17956201a0cf39f1198",
        name: "RockX",
        token: None,
        hide: false,
        uptime: 70,
        can_stake: false,
    },
    EvmPoolV2 {
        address: "0xba669cc6b49218624e84920dc8136a05411b1ec8",
        name: "Stakin",
        token: None,
        hide: false,
        uptime: 89,
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
