use anyhow::Result;
use gclient::GearApi;
use gclient::gear::runtime_types::pallet_gear_voucher::internal::VoucherId;
use gprimitives::ActorId;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};

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

    println!("⚡ HY4 RUST SPAMMER - MAXIMUM SPEED");

    let http_client = Client::new();

    println!("🔌 Connecting to Vara...");

// ADD THIS LINE before builder:
    std::env::set_var("GEAR_NODE_URL", RPC);

    let api = GearApi::builder()
        .suri(&mnemonic)
        .build()
        .await?;
    println!("✅ Connected");

    let mut voucher_id = get_voucher(&http_client).await?;
    println!("🎫 Voucher: {}", voucher_id);

    // FIX 2: Use ActorId from gprimitives instead of ProgramId
    let bet_token: ActorId = hex::decode(BET_TOKEN)
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();

    let mut counter: u64 = 0;
    let mut errors: u32 = 0;

    println!("🚀 LOOP STARTED");

    loop {
        if counter > 0 && counter % 50 == 0 {
            match get_voucher(&http_client).await {
                Ok(v) => voucher_id = v,
                Err(e) => eprintln!("⚠️ Voucher refresh error: {}", e),
            }
        }

        let amount = 20_000_000_000_000u128 + (counter % 99999) as u128;
        let payload = build_approve_payload(amount);

        let voucher_bytes = hex::decode(
            voucher_id.trim_start_matches("0x")
        )?;
        let mut voucher_arr = [0u8; 32];
        voucher_arr.copy_from_slice(&voucher_bytes);
        let voucher = VoucherId(voucher_arr);

        match api
            .send_message_with_voucher(
                voucher,
                bet_token,
                payload,
                25_000_000_000,
                0,
                false,
            )
            .await
        {
            Ok((_message_id, _block_hash)) => {
                counter += 1;
                errors = 0;
                println!("[✅] #{}", counter);
            }
            Err(e) => {
                errors += 1;
                eprintln!("[❌] {}", e);
                if errors >= 5 {
                    sleep(Duration::from_secs(2)).await;
                    errors = 0;
                    match get_voucher(&http_client).await {
                        Ok(v) => voucher_id = v,
                        Err(e) => eprintln!("⚠️ {}", e),
                    }
                }
            }
        }
    }
}
