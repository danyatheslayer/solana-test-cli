use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::task;

#[derive(Debug, Deserialize)]
struct Config {
    sender_wallets: Vec<String>,
    recipient_wallets: Vec<String>,
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    config_path: String,

    #[arg(short, long)]
    lamports: u64,
}

#[derive(Debug)]
struct TransactionResult {
    from: String,
    to: String,
    transaction_hash: Option<String>,
    status: String,
    duration: Duration,
}

async fn send_transaction(
    client: &RpcClient,
    from_keypair: &Keypair,
    to_pubkey: &Pubkey,
    lamports: u64,
) -> Result<(String, Duration)> {
    let start_time = Instant::now();

    let blockhash = client
        .get_latest_blockhash()
        .context("Failed to get latest blockhash")?;

    let transaction = Transaction::new_signed_with_payer(
        &[solana_sdk::system_instruction::transfer(
            &from_keypair.pubkey(),
            to_pubkey,
            lamports,
        )],
        Some(&from_keypair.pubkey()),
        &[from_keypair],
        blockhash,
    );

    let signature = client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to send transaction")?;

    let duration = start_time.elapsed();
    Ok((signature.to_string(), duration))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config_content =
        fs::read_to_string(&args.config_path).context("Unable to read config file")?;
    let config: Config = serde_yaml::from_str(&config_content).context("Failed to parse config")?;

    let client = Arc::new(RpcClient::new("https://api.devnet.solana.com"));
    let results = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::new();

    for (sender, recipient) in config
        .sender_wallets
        .iter()
        .zip(config.recipient_wallets.iter())
    {
        let sender_keypair = Keypair::from_base58_string(sender);

        let recipient_pubkey = match recipient.parse::<Pubkey>() {
            Ok(pk) => pk,
            Err(_) => continue,
        };

        let client = client.clone();
        let results_clone = Arc::clone(&results);
        let lamports = args.lamports;

        let handle = task::spawn(async move {
            let sender_address = sender_keypair.pubkey().to_string();
            let recipient_address = recipient_pubkey.to_string();

            match send_transaction(&client, &sender_keypair, &recipient_pubkey, lamports).await {
                Ok((hash, duration)) => {
                    let result = TransactionResult {
                        from: sender_address,
                        to: recipient_address,
                        transaction_hash: Some(hash),
                        status: "Success".to_string(),
                        duration,
                    };
                    results_clone.lock().unwrap().push(result);
                }
                Err(e) => {
                    let result = TransactionResult {
                        from: sender_address,
                        to: recipient_address,
                        transaction_hash: None,
                        status: format!("Failed: {}", e),
                        duration: Duration::new(0, 0),
                    };
                    results_clone.lock().unwrap().push(result);
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await?;
    }

    for result in results.lock().unwrap().iter() {
        println!(
            "From: {} | To: {} | Hash: {} | Status: {} | Duration: {:?}",
            result.from,
            result.to,
            result
                .transaction_hash
                .clone()
                .unwrap_or_else(|| "N/A".to_string()),
            result.status,
            result.duration
        );
    }

    Ok(())
}
