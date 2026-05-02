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

// ✅ FIX 1: u128 is 16 bytes — was wrongly using 32 bytes before
fn build_approve_payload(amount: u128) -> Vec<u8> {
    let service = b"BetToken";
    let method = b"Approve";
    let spender = hex::decode(BET_LANE).unwrap();

    let mut payload = Vec::new();
    // SCALE compact encoding for string lengths
    payload.push((service.len() as u8) << 2);
    payload.extend_from_slice(service);
    payload.push((method.len() as u8) << 2);
    payload.extend_from_slice(method);
    // Spender ActorId: 32 bytes
    payload.extend_from_slice(&spender);
    // Amount: u128 = exactly 16 bytes little-endian
    payload.extend_from_slice(&amount.to_le_bytes());
    payload
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let mnemonic = std::env::var("PRIVATE_KEY")
        .expect("PRIVATE_KEY not set");

    println!("⚡ HY4 RUST SPAMMER - FIRE AND FORGET MODE");

    let http_client = Client::new();

    println!("🔌 Connecting to Vara...");

    // ✅ Use builder with explicit URI — no env var hack needed
    let api = GearApi::builder()
        .suri(&mnemonic)
        .uri(RPC)
        .build()
        .await?;

    println!("✅ Connected | account: {:?}", api.account_id());

    let mut voucher_id = get_voucher(&http_client).await?;
    println!("🎫 Voucher: {}", voucher_id);

    let bet_token: ActorId = hex::decode(BET_TOKEN)
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();

    let counter = Arc::new(AtomicU64::new(0));
    let mut loop_count: u64 = 0;

    println!("🚀 LOOP STARTED - fire-and-forget, 500ms pace");

    loop {
        loop_count += 1;

        // Refresh voucher every 50 iterations
        if loop_count % 50 == 0 {
            match get_voucher(&http_client).await {
                Ok(v) => {
                    println!("🔄 Voucher refreshed: {}", v);
                    voucher_id = v;
                }
                Err(e) => eprintln!("⚠️ Voucher refresh error: {}", e),
            }
        }

        // Parse voucher
        let voucher_bytes = match hex::decode(voucher_id.trim_start_matches("0x")) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("⚠️ Bad voucher hex: {}", e);
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        let mut voucher_arr = [0u8; 32];
        voucher_arr.copy_from_slice(&voucher_bytes);
        let voucher = VoucherId(voucher_arr);

        // ✅ FIX 2: fetch nonce explicitly and pin it to this submission
        // Without this, concurrent clones all read the same nonce → 1014 collision
        let nonce = match api.rpc_nonce().await {
            Ok(n) => n,
            Err(e) => {
                eprintln!("⚠️ Nonce fetch error: {}", e);
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };

        let amount = 20_000_000_000_000u128 + (loop_count % 99999) as u128;
        let payload = build_approve_payload(amount);

        // Clone and pin nonce to this specific submission
        let mut api_clone = api.clone();
        api_clone.set_nonce(nonce);
        let counter_clone = counter.clone();

        // ✅ FIX 3: fire-and-forget — don't block on tx finalization
        // The .await here only waits for the tx to be SUBMITTED to the pool,
        // not for it to be included in a block. We move on immediately.
        tokio::spawn(async move {
            match api_clone
                .send_message_with_voucher(
                    voucher,
                    bet_token,
                    payload,
                    25_000_000_000, // 25B gas
                    0,              // value
                    false,          // keep_alive
                )
                .await
            {
                Ok(_) => {
                    let n = counter_clone.fetch_add(1, Ordering::Relaxed) + 1;
                    println!("[✅] #{}", n);
                }
                Err(e) => eprintln!("[❌] {}", e),
            }
        });

        // Pace to ~2 txs/sec — gives nonce time to update on-chain
        // If you still get 1014 errors bump this to 1000ms
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
