#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let mnemonic = std::env::var("PRIVATE_KEY")
        .expect("PRIVATE_KEY not set");

    println!("⚡ HY4 RUST SPAMMER - MAXIMUM SPEED");

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

        let voucher_bytes = hex::decode(voucher_id.trim_start_matches("0x"))?;
        let mut voucher_arr = [0u8; 32];
        voucher_arr.copy_from_slice(&voucher_bytes);
        let voucher = VoucherId(voucher_arr);

        match api
            .send_message_with_voucher(voucher, bet_token, payload, 25_000_000_000, 0, false)
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
                    tokio::time::sleep(Duration::from_secs(2)).await;
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
