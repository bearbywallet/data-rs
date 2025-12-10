use crate::models::meta::Meta;
use crate::models::meta::Token;
use bytes::Bytes;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::header::ACCESS_CONTROL_ALLOW_METHODS;
use hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN;
use hyper::http::HeaderValue;
use hyper::{header, Request, Response, StatusCode};
use serde_json::Value;
use serde_json::{self, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::dex::ListedTokens;

pub async fn hanlde_get_evm_tokens() -> Result<Response<Full<Bytes>>, hyper::Error> {
    let tokens_res = json!([
        {
            "address": "0x8a2afD8Fe79F8C694210eB71f4d726Fc8cAFdB31",
            "name": "Amazing Pool (Avely)",
            "symbol": "aZIL",
            "decimals": 18,
            "poolName": "Amazing Pool - Avely and ZilPay",
            "poolAddress": "0x1f0e86Bc299Cc66df2e5512a7786C3F528C0b5b6"
        },
        {
            "address": "0x17D5af5658A24bd964984b36d28e879a8626adC3",
            "name": "Zilliqa-bridged ETH token",
            "symbol": "zETH",
            "decimals": 18
        },
        {
            "address": "0xea87bC6CcaE73bae35693639e22eF30667760F61",
            "name": "Zilliqa-bridged BNB Coin",
            "symbol": "zBNB",
            "decimals": 18
        },
        {
            "address": "0x2274005778063684fbB1BfA96a2b725dC37D75f9",
            "name": "Zilliqa-bridged USDT token",
            "symbol": "zUSDT",
            "decimals": 6
        },
        {
            "address": "0x2938fF251Aecc1dfa768D7d0276eB6d073690317",
            "name": "Zilliqa-bridged WBTC token",
            "symbol": "zWBTC",
            "decimals": 8
        },
        {
            "address": "0x4345472A0c6164F35808CDb7e7eCCd3d326CC50b",
            "name": "Zilliqa-bridged MATIC Token",
            "symbol": "zMATIC",
            "decimals": 18
        },
        {
            "address": "0x8DEAdC20f7218994c86b59eE1D5c7979fFcAa893",
            "name": "Zilliqa-bridged OPUL token",
            "symbol": "zOPUL",
            "decimals": 18
        },
        {
            "address": "0xD819257C964A78A493DF93D5643E9490b54C5af2",
            "name": "Zilliqa-bridged BRKL token",
            "symbol": "zBRKL",
            "decimals": 18
        },
        {
            "address": "0x9121A67cA79B6778eAb477c5F76dF6de7C79cC4b",
            "name": "Zilliqa-bridged TRAXX token",
            "symbol": "zTRAXX",
            "decimals": 18
        },
        {
            "address": "0x097C26F8A93009fd9d98561384b5014D64ae17C2",
            "name": "StZIL",
            "symbol": "stZIL",
            "decimals": 12
        },
        {
            "address": "0x8E3073b22F670d3A09C66D0Abb863f9E358402d2",
            "name": "Encapsulate Zilliqa",
            "symbol": "encapZIL",
            "decimals": 18,
            "poolName": "Encapsulate",
            "poolAddress": "0x1311059DD836D7000Dc673eA4Cc834fe04e9933C"
        },
        {
            "address": "0x3B78f66651E2eCAbf13977817848F82927a17DcF",
            "name": "litZil",
            "symbol": "litZil",
            "decimals": 18,
            "poolName": "Lithium Digital",
            "poolAddress": "0xBD6ca237f30A86eea8CF9bF869677F3a0496a990"
        },
        {
            "address": "0x737EBf814D2C14fb21E00Fd2990AFc364C2AF506",
            "name": "StakeShark",
            "symbol": "shZIL",
            "decimals": 18,
            "poolName": "StakeShark",
            "poolAddress": "0xF7F4049e7472fC32805Aae5bcCE909419a34D254"
        },
        {
            "address": "0x63B991C17010C21250a0eA58C6697F696a48cdf3",
            "name": "The Winners Circle",
            "symbol": "HRSE",
            "decimals": 18
        },
        {
            "address": "0x241c677D9969419800402521ae87C411897A029f",
            "name": "WEB3WAR Token",
            "symbol": "FPS",
            "decimals": 12
        },
        {
            "address": "0x598FbD8B68a8B7e75b8B7182c750164f348907Bc",
            "name": "XSGD",
            "symbol": "XSGD",
            "decimals": 6
        },
        {
            "address": "0xCcF3Ea256d42Aeef0EE0e39Bfc94bAa9Fa14b0Ba",
            "name": "XCAD Network Token",
            "symbol": "XCAD",
            "decimals": 18
        },
        {
            "address": "0xc6F3dede529Af9D98a11C5B32DbF03Bf34272ED5",
            "name": "The GARY Token",
            "symbol": "GARY",
            "decimals": 4
        },
        {
            "address": "0x7D2fF48c6b59229d448473D267a714d29F078D3E",
            "name": "ZilStream",
            "symbol": "STREAM",
            "decimals": 8
        },
        {
            "address": "0xE9D47623bb2B3C497668B34fcf61E101a7ea4058",
            "name": "Lunr",
            "symbol": "Lunr",
            "decimals": 4
        },
        {
            "address": "0x03A79429acc808e4261a68b0117aCD43Cb0FdBfa",
            "name": "Governance ZIL",
            "symbol": "gZIL",
            "decimals": 15
        },
        {
            "address": "0x01035e423c40a9ad4F6be2E6cC014EB5617c8Bd6",
            "name": "Zoge Coin",
            "symbol": "ZOGE",
            "decimals": 18
        },
        {
            "address": "0x9C3fE3f471d8380297e4fB222eFb313Ee94DFa0f",
            "name": "ZilPepe",
            "symbol": "ZILPEPE",
            "decimals": 18
        },
        {
            "address": "0x20Dd5D5B5d4C72676514A0eA1052d0200003d69D",
            "name": "Stellar Void",
            "symbol": "VOID",
            "decimals": 12
        },
        {
            "address": "0xbfDe2156aF75a29d36614bC1F8005DD816Bd9200",
            "name": "Zhib Coin",
            "symbol": "ZHIB",
            "decimals": 4
        },
        {
            "address": "0xa0A5795e7eccc43Ba92d2A0b7804696F8B9e1a05",
            "name": "dXCAD Token",
            "symbol": "dXCAD",
            "decimals": 18
        },
        {
            "address": "0xc85b0db68467dede96A7087F4d4C47731555cA7A",
            "name": "PlunderSwap Staked ZIL",
            "symbol": "pZIL",
            "decimals": 18,
            "poolName": "PlunderSwap",
            "poolAddress": "0x691682FCa60Fa6B702a0a69F60d045c08f404220"
        },
        {
            "address": "0x1202078D298Ff0358A95b6fbf48Ec166dB414660",
            "name": "Proof Of Receipt Token",
            "symbol": "PORT",
            "decimals": 4
        },
        {
            "address": "0xc99ECB82a27B45592eA02ACe9e3C42050f3c00C0",
            "name": "UNIFEES",
            "symbol": "FEES",
            "decimals": 4
        },
        {
            "address": "0xe64cA52EF34FdD7e20C0c7fb2E392cc9b4F6D049",
            "name": "Kalijo",
            "symbol": "SEED",
            "decimals": 18
        }
    ]);

    let res_json = serde_json::to_string(&tokens_res).unwrap();
    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(res_json)))
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

pub async fn handle_get_tokens(
    req: Request<hyper::body::Incoming>,
    meta: Arc<RwLock<Meta>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut params_map = HashMap::new();
    let query_params = req.uri().query().unwrap_or("");
    let parsed_params = url::form_urlencoded::parse(query_params.as_bytes());

    for (key, value) in parsed_params {
        params_map.insert(key.into_owned(), value.into_owned());
    }

    let limit: usize = params_map
        .get("limit")
        .unwrap_or(&"200".to_string())
        .parse()
        .unwrap_or(200);
    let token_type: u8 = params_map
        .get("type")
        .unwrap_or(&"1".to_string())
        .parse()
        .unwrap_or(1);
    let offset: usize = params_map
        .get("offset")
        .unwrap_or(&"0".to_string())
        .parse()
        .unwrap_or(0);

    for token in meta.read().await.list.iter() {
        if token.token_type == token_type && token.status == 1 {
            tokens.push(token.clone());
        }
    }

    let tokens = tokens
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<Token>>();
    let tokens_res = ListedTokens {
        count: tokens.len(),
        list: tokens,
    };

    let res_json = serde_json::to_string(&tokens_res).unwrap();
    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(res_json)))
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

pub async fn handle_get_token(
    req: Request<hyper::body::Incoming>,
    meta: Arc<RwLock<Meta>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let params = req.uri().path().split("/").collect::<Vec<&str>>();
    let symbol = params.last().unwrap_or(&"").to_lowercase();

    if let Some(token) = meta
        .read()
        .await
        .list
        .iter()
        .find(|t| t.symbol.to_lowercase() == symbol && t.status == 1)
    {
        let res_json = serde_json::to_string(&token).unwrap();
        let response = Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(res_json)))
            .unwrap();

        Ok(response)
    } else {
        let res = json!({
            "code": -1,
            "message": format!("No token {}", symbol)
        });
        let not_found = serde_json::to_string(&res).unwrap();
        let response = Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from(not_found)))
            .unwrap();

        Ok(response)
    }
}

pub async fn handle_update_token(
    req: Request<hyper::body::Incoming>,
    meta: Arc<RwLock<Meta>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let access_token = std::env::var("ACCESS_TOKEN").unwrap_or("666".to_string());
    let header_token = match req.headers().get("Authorization") {
        Some(value) => value.to_str().unwrap_or(""),
        None => "",
    };
    let response = Response::builder().header(header::CONTENT_TYPE, "application/json");

    if access_token != header_token {
        let res = json!({
            "code": -5,
            "message": format!("Incorrect atuh token {header_token}")
        });
        let res_json = serde_json::to_string(&res).unwrap();
        let response = response
            .status(StatusCode::NETWORK_AUTHENTICATION_REQUIRED)
            .body(Full::new(Bytes::from(res_json)))
            .unwrap();

        return Ok(response);
    }

    let params = req.uri().path().split("/").collect::<Vec<&str>>();
    let base16 = params.last().unwrap_or(&"").to_lowercase();
    let body_bytes = req.collect().await?.to_bytes();
    let value: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => {
            let res = json!({
                "code": -2,
                "message": "Incorrect params"
            });
            let res_json = serde_json::to_string(&res).unwrap();
            let response = response
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from(res_json)))
                .unwrap();

            return Ok(response);
        }
    };
    let map = match value.as_object() {
        Some(v) => v,
        None => {
            let res = json!({
                "code": -2,
                "message": "Incorrect params"
            });
            let res_json = serde_json::to_string(&res).unwrap();
            let response = response
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from(res_json)))
                .unwrap();

            return Ok(response);
        }
    };
    let status = map.get("status");
    let score = map.get("score");
    let listed = map.get("listed");
    let symbol = map.get("symbol");
    let mut token_meta = meta.write().await;
    let token_index = match token_meta
        .list
        .iter()
        .position(|t| t.base16.to_lowercase() == base16)
    {
        Some(index) => index,
        None => {
            let res = json!({
                "code": -1,
                "message": format!("No token {}", base16)
            });
            let not_found = serde_json::to_string(&res).unwrap();
            let response = response
                .status(StatusCode::NOT_FOUND)
                .body(Full::new(Bytes::from(not_found)))
                .unwrap();

            return Ok(response);
        }
    };

    if let Some(status) = status {
        let new_status = status.as_u64().unwrap_or(0);
        let new_status: u8 = if new_status > 1 { 1 } else { new_status as u8 };

        token_meta.list[token_index].status = new_status;
    }
    if let Some(symbol) = symbol {
        let new_symbol = symbol.as_str().unwrap_or("").to_string();

        token_meta.list[token_index].symbol = new_symbol;
    }
    if let Some(score) = score {
        let new_score: u8 = score.as_u64().unwrap_or(0) as u8;

        token_meta.list[token_index].scope = new_score;
    }
    if let Some(listed) = listed {
        let new_listed = listed.as_bool().unwrap_or(false);

        token_meta.list[token_index].listed = new_listed;
    }

    match token_meta.write_db() {
        Ok(_) => (),
        Err(_) => {
            let res = json!({
                "code": -4,
                "message": "Cannot write database"
            });
            let wr_db = serde_json::to_string(&res).unwrap();
            let response = response
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from(wr_db)))
                .unwrap();

            return Ok(response);
        }
    };

    let res = json!({ "message": format!("updated token {}", base16) });
    let ok = serde_json::to_string(&res).unwrap();
    let response = response
        .status(StatusCode::OK)
        .body(Full::new(Bytes::from(ok)))
        .unwrap();

    return Ok(response);
}
