use std::time::Duration;

use reqwest::Client;
use serde_json::json;
use thiserror::Error;

use super::tokens::Token;

pub const URLS: [&str; 5] = [
    "https://cloudflare-eth.com",
    "https://eth.llamarpc.com",
    "https://eth.rpc.blxrbdn.com",
    "https://virginia.rpc.blxrbdn.com",
    "https://rpc.flashbots.net",
];

const WETH_ADDRESS: [u8; 20] = [
    0xC0, 0x2A, 0xAA, 0x39, 0xB2, 0x23, 0xFE, 0x8D, 0x0A, 0x0E, 0x5C, 0x4F, 0x27, 0xEA, 0xD9,
    0x08, 0x3C, 0x75, 0x6C, 0xC2,
];

const FACTORY_ADDRESS: [u8; 20] = [
    0x5C, 0x69, 0xBE, 0xE7, 0x01, 0xEF, 0x81, 0x4A, 0x2B, 0x6A, 0x3E, 0xDD, 0x4B, 0x16, 0x52,
    0xCB, 0x9C, 0xC5, 0xAA, 0x6F,
];

// Function selectors (keccak256 first 4 bytes of signature)
const SELECTOR_GET_PAIR: [u8; 4] = [0xE6, 0xA4, 0x39, 0x05]; // getPair(address,address)
const SELECTOR_GET_RESERVES: [u8; 4] = [0x09, 0x02, 0xF1, 0xAC]; // getReserves()
const SELECTOR_TOKEN0: [u8; 4] = [0x0D, 0xFE, 0x16, 0x81]; // token0()

const ADDRESS_ZERO: [u8; 20] = [0u8; 20];

#[derive(Error, Debug)]
pub enum UniswapDexError {
    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Missing environment variable: {0}")]
    EnvVar(String),
}

fn parse_address(s: &str) -> Option<[u8; 20]> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).ok()?;
    if bytes.len() == 20 {
        let mut arr = [0u8; 20];
        arr.copy_from_slice(&bytes);
        Some(arr)
    } else {
        None
    }
}

/// Left-pad a 20-byte address to 32 bytes (ABI encoding).
fn pad_address(addr: &[u8; 20]) -> [u8; 32] {
    let mut padded = [0u8; 32];
    padded[12..32].copy_from_slice(addr);
    padded
}

/// Extract a 20-byte address from a 32-byte ABI-encoded word.
fn decode_address(data: &[u8]) -> Result<[u8; 20], UniswapDexError> {
    if data.len() < 32 {
        return Err(UniswapDexError::ApiError(format!(
            "Invalid address data length: {}",
            data.len()
        )));
    }
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&data[12..32]);
    Ok(addr)
}

fn encode_get_pair(token_a: &[u8; 20], token_b: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(68); // 4 + 32 + 32
    data.extend_from_slice(&SELECTOR_GET_PAIR);
    data.extend_from_slice(&pad_address(token_a));
    data.extend_from_slice(&pad_address(token_b));
    data
}

fn encode_get_reserves() -> Vec<u8> {
    SELECTOR_GET_RESERVES.to_vec()
}

fn encode_token0() -> Vec<u8> {
    SELECTOR_TOKEN0.to_vec()
}

fn decode_get_pair_returns(data: &[u8]) -> Result<[u8; 20], UniswapDexError> {
    decode_address(data)
}

fn decode_get_reserves_returns(data: &[u8]) -> Result<(u128, u128), UniswapDexError> {
    if data.len() < 96 {
        return Err(UniswapDexError::ApiError(format!(
            "Invalid reserves data length: {}",
            data.len()
        )));
    }
    // uint112 padded to 32 bytes
    let reserve0 = u128::from_be_bytes(data[16..32].try_into().unwrap());
    let reserve1 = u128::from_be_bytes(data[48..64].try_into().unwrap());
    // data[64..96] is blockTimestampLast (uint32), ignored
    Ok((reserve0, reserve1))
}

fn decode_token0_returns(data: &[u8]) -> Result<[u8; 20], UniswapDexError> {
    decode_address(data)
}

fn create_eth_call_request(id: String, to: &[u8; 20], data: Vec<u8>) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "eth_call",
        "params": [
            {
                "to": format!("0x{}", hex::encode(to)),
                "data": format!("0x{}", hex::encode(&data))
            },
            "latest"
        ]
    })
}

async fn send_batch_request(
    client: &Client,
    urls: &[&str],
    requests: &[serde_json::Value],
) -> Result<Vec<serde_json::Value>, UniswapDexError> {
    for url in urls {
        match client.post(*url).json(requests).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let responses: Vec<serde_json::Value> =
                        response.json().await.map_err(UniswapDexError::Reqwest)?;
                    if responses.iter().all(|r| r.get("result").is_some()) {
                        return Ok(responses);
                    }
                }
            }
            Err(_e) => {}
        }
    }
    Err(UniswapDexError::ApiError(
        "All nodes failed or returned errors".to_string(),
    ))
}

pub async fn get_token_prices_in_eth(tokens: &mut [Token]) -> Result<(), UniswapDexError> {
    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;

    let mut batch_requests = Vec::with_capacity(tokens.len());
    for (i, token) in tokens.iter().enumerate() {
        let token_address = parse_address(&token.address).unwrap_or(ADDRESS_ZERO);
        let data = encode_get_pair(&token_address, &WETH_ADDRESS);
        let request = create_eth_call_request(format!("getPair_{}", i), &FACTORY_ADDRESS, data);
        batch_requests.push(request);
    }

    let responses = send_batch_request(&client, &URLS, &batch_requests).await?;

    let mut pair_addresses = vec![ADDRESS_ZERO; tokens.len()];
    for resp in responses {
        if let Some(id) = resp["id"].as_str() {
            if let Some(result) = resp["result"].as_str() {
                if let Some(index) = id
                    .strip_prefix("getPair_")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    let hex_data = result.strip_prefix("0x").unwrap_or(result);
                    let data = hex::decode(hex_data).map_err(|e| {
                        UniswapDexError::ApiError(format!("Hex decode error: {}", e))
                    })?;
                    let decoded = decode_get_pair_returns(&data)?;
                    pair_addresses[index] = decoded;
                }
            }
        }
    }

    let mut batch_requests_2 = Vec::new();
    for (i, &pair_address) in pair_addresses.iter().enumerate() {
        if pair_address != ADDRESS_ZERO {
            let data_reserves = encode_get_reserves();
            let request_reserves =
                create_eth_call_request(format!("getReserves_{}", i), &pair_address, data_reserves);
            batch_requests_2.push(request_reserves);

            let data_token0 = encode_token0();
            let request_token0 =
                create_eth_call_request(format!("token0_{}", i), &pair_address, data_token0);
            batch_requests_2.push(request_token0);
        }
    }

    let responses_2 = send_batch_request(&client, &URLS, &batch_requests_2).await?;

    let mut reserves = vec![None; tokens.len()];
    let mut token0s = vec![None; tokens.len()];
    for resp in responses_2 {
        if let Some(id) = resp["id"].as_str() {
            if let Some(result) = resp["result"].as_str() {
                let hex_data = result.strip_prefix("0x").unwrap_or(result);
                let data = hex::decode(hex_data)
                    .map_err(|e| UniswapDexError::ApiError(format!("Hex decode error: {}", e)))?;
                if let Some(index) = id
                    .strip_prefix("getReserves_")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    let decoded = decode_get_reserves_returns(&data)?;
                    reserves[index] = Some(decoded);
                } else if let Some(index) = id
                    .strip_prefix("token0_")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    let decoded = decode_token0_returns(&data)?;
                    token0s[index] = Some(decoded);
                }
            }
        }
    }

    for i in 0..tokens.len() {
        if let (Some((reserve0, reserve1)), Some(token0_address)) = (reserves[i], token0s[i]) {
            let token_address_bytes = parse_address(&tokens[i].address).unwrap_or(ADDRESS_ZERO);
            let (reserve_token, reserve_weth) = if token_address_bytes == token0_address {
                (reserve0, reserve1)
            } else {
                (reserve1, reserve0)
            };

            if reserve_token != 0 {
                let reserve_weth_eth = reserve_weth as f64 / 1e18;
                let reserve_token_tokens =
                    reserve_token as f64 / 10f64.powi(tokens[i].decimals as i32);
                let new_rate = reserve_weth_eth / reserve_token_tokens;

                tokens[i].last_price = tokens[i].rate;
                tokens[i].rate = new_rate;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::tokens::{TokenStatus, TokenType};

    #[test]
    fn test_address_encoding() {
        let addr: [u8; 20] = [
            0x6B, 0x17, 0x54, 0x74, 0xE8, 0x90, 0x94, 0xC4, 0x4D, 0xA9, 0x8B, 0x95, 0x4E, 0xED,
            0xEA, 0xC4, 0x95, 0x27, 0x1D, 0x0F,
        ];
        let formatted = format_address(&addr);
        assert_eq!(formatted, "0x6b175474e89094c44da98b954eedeac495271d0f");

        let parsed = parse_address(&formatted).unwrap();
        assert_eq!(parsed, addr);
    }

    #[test]
    fn test_get_pair_encoding() {
        let token_a = [0xAA; 20];
        let token_b = [0xBB; 20];
        let encoded = encode_get_pair(&token_a, &token_b);
        assert_eq!(&encoded[0..4], &SELECTOR_GET_PAIR);
        // First address padded: 12 zero bytes + 20 address bytes
        assert_eq!(&encoded[4..16], &[0u8; 12]);
        assert_eq!(&encoded[16..36], &[0xAA; 20]);
    }

    #[tokio::test]
    async fn test_get_token_prices_in_eth() {
        let mut tokens = vec![
            Token {
                address: "0x6B175474E89094C44Da98b954EedeAC495271d0F".to_string(), // DAI
                scope: 0,
                name: "Dai Stablecoin".to_string(),
                symbol: "DAI".to_string(),
                token_type: TokenType::FT,
                decimals: 18,
                listed: true,
                status: TokenStatus::Available,
                chain_id: 1,
                rate: 0.0,
                last_price: 0.0,
            },
            Token {
                address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(), // USDC
                scope: 0,
                name: "USD Coin".to_string(),
                symbol: "USDC".to_string(),
                token_type: TokenType::FT,
                decimals: 6,
                listed: true,
                status: TokenStatus::Available,
                chain_id: 1,
                rate: 0.0,
                last_price: 0.0,
            },
            Token {
                address: "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599".to_string(), // WBTC
                scope: 0,
                name: "Wrapped Bitcoin".to_string(),
                symbol: "WBTC".to_string(),
                token_type: TokenType::FT,
                decimals: 8,
                listed: true,
                status: TokenStatus::Available,
                chain_id: 1,
                rate: 0.0,
                last_price: 0.0,
            },
        ];

        get_token_prices_in_eth(&mut tokens)
            .await
            .expect("Failed to fetch token prices");

        assert!(tokens[0].rate > 0.0, "DAI price should be positive");
        assert!(tokens[1].rate > 0.0, "USDC price should be positive");
        assert!(tokens[2].rate > 0.0, "WBTC price should be positive");
    }
}
