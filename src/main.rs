use anyhow::Result;
use gclient::GearApi;
use gclient::gear::runtime_types::pallet_gear_voucher::internal::VoucherId;
use gprimitives::ActorId;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::Duration;

const RPC: &str = "wss://rpc.vara.network";
const BASKET_MARKET: &str = "e5dd153b813c768b109094a9e2eb496c38216b1dbe868391f1d20ac927b7d2c2";
const BET_TOKEN: &str = "186f6cda18fea13d9fc5969eec5a379220d6726f64c1d5f4b346e89271f917bc";
const BET_LANE: &str = "35848dea0ab64f283497deaff93b12fe4d17649624b2cd5149f253ef372b29dc";
const HEX_ADDRESS: &str = "0x2a3d796f3e8401782789ebf3f92d12c8d9f0addb39643dbea01b96d230207a3f";
const VOUCHER_URL: &str = "https://voucher-backend-production-5a1b.up.railway.app/voucher";

// How many concurrent txs to fire at once
// Start at 3 — increase if no nonce errors
const CONCURRENCY: u64 = 3;

#[derive(Deserialize, Debug)]
struct VoucherResponse {
    #[serde(rename = "voucherId")]
    voucher_id: Option<String>,
    #[serde(rename = "canTopUpNow")]
    can_top_up_now: Option<bool>,
}

#[derive(Serialize)]
struct VoucherRequest {
    account: String,
    programs: Vec<String>,
}

async fn get_voucher(client: &Client) -> Result<String> {
    let url = format!("{}/{}", VOUCHER_URL, HEX_ADDRESS);
    let resp: VoucherResponse = client.get(&url).send().await?.json().await?;

    if let Some(id) = &resp.voucher_id {
        if resp.can_top_up_now == Some(false) {
            return Ok(id.clone());
        }
    }

    let body = VoucherRequest {
        account: HEX_ADDRESS.to_string(),
        programs: vec![
            format!("0x{}", BASKET_MARKET),
            format!("0x{}", BET_TOKEN),
            format!("0x{}", BET_LANE),
        ],
    };

    let post_resp: VoucherResponse = client
        .post(VOUCHER_URL)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;

    if let Some(id) = post_resp.voucher_id {
        return Ok(id);
    }

    if let Some(id) = resp.voucher_id {
        return Ok(id);
    }

    anyhow::bail!("No voucher available")
}

fn build_approve_payload(amount: u128) -> Vec<u8> {
    let service = b"BetToken";
    let method = b"Approve";
    let spender = hex::decode(BET_LANE).unwrap();

    let mut value = [0u8; 32];
    let amount_bytes = amount.to_le_bytes();
    value[..16].copy_from_slice(&amount_bytes);

    let mut payload = Vec::new();
    payload.push((service.len() as u8) << 2);
    payload.extend_from_slice(service);
    payload.push((method.len() as u8) << 2);
    payload.extend_from_slice(method);
    payload.extend_from_slice(&spender);
    payload.extend_from_slice(&value);
    payload
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let mnemonic = std::env::var("PRIVATE_KEY")
        .expect("PRIVATE_KEY not set");

    println!("⚡ HY4 RUST SPAMMER - CONCURRENT MODE");

    let http_client = Client::new();

    println!("🔌 Connecting to Vara...");
    std::env::set_var("GEAR_NODE_URL", RPC);

    let api = GearApi::builder()
        .suri(&mnemonic)
        .build()
        .await?;

    println!("✅ Connected");

    let mut voucher_id = get_voucher(&http_client).await?;
    println!("🎫 Voucher: {}", voucher_id);

    let bet_token: ActorId = hex::decode(BET_TOKEN)
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();

    let counter = Arc::new(AtomicU64::new(0));
    let mut loop_count: u64 = 0;

    println!("🚀 LOOP STARTED - concurrency={}", CONCURRENCY);

    loop {
        loop_count += 1;

        // Refresh voucher every 50 loop iterations
        if loop_count % 50 == 0 {
            match get_voucher(&http_client).await {
                Ok(v) => voucher_id = v,
                Err(e) => eprintln!("⚠️ Voucher error: {}", e),
            }
        }

        let voucher_bytes = hex::decode(voucher_id.trim_start_matches("0x"))?;
        let mut voucher_arr = [0u8; 32];
        voucher_arr.copy_from_slice(&voucher_bytes);
        let voucher = VoucherId(voucher_arr);

        // Spawn CONCURRENCY tasks simultaneously
        let mut handles = Vec::new();
        for i in 0..CONCURRENCY {
            let api_clone = api.clone();
            let counter_clone = counter.clone();
            let amount = 20_000_000_000_000u128 + (loop_count * CONCURRENCY + i) % 99999;
            let payload = build_approve_payload(amount);
            let voucher_clone = VoucherId(voucher_arr);

            let handle = tokio::spawn(async move {
                match api_clone
                    .send_message_with_voucher(
                        voucher_clone,
                        bet_token,
                        payload,
                        25_000_000_000,
                        0,
                        false,
                    )
                    .await
                {
                    Ok(_) => {
                        let n = counter_clone.fetch_add(1, Ordering::Relaxed) + 1;
                        println!("[✅] #{}", n);
                    }
                    Err(e) => {
                        eprintln!("[❌] {}", e);
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all concurrent tasks to finish before next batch
        for h in handles {
            let _ = h.await;
        }
    }
}
