use anyhow::Result;
use gclient::GearApi;
use gclient::gear::runtime_types::pallet_gear_voucher::internal::VoucherId;
use gprimitives::ActorId;
use parity_scale_codec::{Encode, Output};
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

// ✅ Wrap raw bytes to bypass SCALE Vec<u8> double-encoding
struct RawBytes(Vec<u8>);

impl Encode for RawBytes {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        dest.write(&self.0);
    }
}

#[derive(Deserialize, Debug, Clone)]
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

    let mut payload = Vec::new();
    payload.push((service.len() as u8) << 2);
    payload.extend_from_slice(service);
    payload.push((method.len() as u8) << 2);
    payload.extend_from_slice(method);
    payload.extend_from_slice(&spender);
    payload.extend_from_slice(&amount.to_le_bytes());
    payload
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let mnemonic = std::env::var("PRIVATE_KEY").expect("PRIVATE_KEY not set");

    println!("⚡ HY4 RUST SPAMMER - STAGGERED FIRE-AND-FORGET");
    println!("🔌 Connecting to Vara...");

    let http_client = Client::new();

    let api = Arc::new(tokio::sync::Mutex::new(
        GearApi::builder()
            .suri(&mnemonic)
            .uri(RPC)
            .build()
            .await?
    ));

    // Access the inner account ID before entering the loop
    let account_id = api.lock().await.account_id().clone();
    println!("✅ Connected | account: {:?}", account_id);

    let mut voucher_id = get_voucher(&http_client).await?;
    println!("🎫 Voucher: {}", voucher_id);

    let bet_token: ActorId = hex::decode(BET_TOKEN)
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();

    let counter = Arc::new(AtomicU64::new(0));
    let mut loop_count: u64 = 0;

    println!("🚀 LOOP STARTED - 500ms staggered fire-and-forget");

    loop {
        loop_count += 1;

        // Refresh voucher every 50 loops
        if loop_count % 50 == 0 {
            match get_voucher(&http_client).await {
                Ok(v) => {
                    println!("🔄 Voucher refreshed: {}", v);
                    voucher_id = v;
                }
                Err(e) => eprintln!("⚠️ Voucher refresh error: {}", e),
            }
        }

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

        // Unique amount per loop
        let amount = 20_000_000_000_000u128 + (loop_count % 99999) as u128;
        let payload = build_approve_payload(amount);

        // Clone the Arc to pass the Mutex reference into the spawned task
        let api_clone = Arc::clone(&api);
        let bet_token_clone = bet_token;
        let counter_clone = Arc::clone(&counter);
        let task_id = loop_count;

        tokio::spawn(async move {
            let api_lock = api_clone.lock().await;
            match api_lock
                .send_message_with_voucher(
                    VoucherId(voucher_arr),
                    bet_token_clone,
                    RawBytes(payload),
                    25_000_000_000,
                    0,
                    false,
                )
                .await
            {
                Ok(_) => {
                    let n = counter_clone.fetch_add(1, Ordering::Relaxed) + 1;
                    println!("[✅] Global #{} (Task {})", n, task_id);
                }
                Err(e) => {
                    eprintln!("[❌] Task {}: {}", task_id, e);
                }
            }
        });

        // 🕒 THE MAGIC FIX: Stagger submissions by 750ms!
        // This gives the node's tx pool enough time to receive the TX and update the
        // `account_next_index` (pending nonce). The Mutex lock ensures we don't accidentally
        // reset the internally tracked nonce via `api.clone()`.
        tokio::time::sleep(Duration::from_millis(750)).await;
    }
}
