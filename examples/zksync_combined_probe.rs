// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

/// Diagnostic example for investigating zkSync combined retrieval behavior.
///
/// This probe exercises the same building blocks used by semioscan's combined
/// retrieval path on zkSync:
///
/// 1. semioscan's chunked ERC-20 transfer scan
/// 2. repeated typed `eth_getTransactionByHash` lookups through an
///    `Ethereum`-typed provider
/// 3. repeated permissive raw `eth_getTransactionByHash` lookups into
///    `AnyRpcTransaction`
/// 4. repeated `eth_getTransactionReceipt` lookups
/// 5. the full `CombinedCalculator::calculate_combined_data_ethereum` path
///
/// It is intended for incident reproduction and provider comparison when logs
/// succeed but typed transaction enrichment does not.
///
/// Run with:
/// ```bash
/// ZKSYNC_RPC_URL=https://your-zksync-rpc \
/// cargo run --package semioscan --example zksync_combined_probe
/// ```
///
/// Optional environment variables:
/// - `ZKSYNC_PROBE_TX_HASH`
/// - `ZKSYNC_PROBE_START_BLOCK`
/// - `ZKSYNC_PROBE_END_BLOCK`
/// - `ZKSYNC_PROBE_FROM_ADDRESS`
/// - `ZKSYNC_PROBE_TO_ADDRESS`
/// - `ZKSYNC_PROBE_TOKEN`
/// - `ZKSYNC_PROBE_ATTEMPTS` (default: 3)
/// - `ZKSYNC_PROBE_DELAY_MS` (default: 250)
/// - `ZKSYNC_PROBE_ALT_RPC_URL` (optional second provider to compare)
///
/// For convenience, the probe-specific variables fall back to existing
/// `ZKSYNC_*` names from local `.env` files when available.
use alloy_chains::NamedChain;
use alloy_eips::Typed2718;
use alloy_network::{AnyRpcTransaction, Ethereum, ReceiptResponse, TransactionResponse};
use alloy_primitives::{Address, TxHash, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{Filter, Log as RpcLog};
use alloy_sol_types::SolEvent;
use anyhow::{Context, Result};
use semioscan::{
    create_typed_http_provider, fetch_logs_chunked, CombinedCalculator, CombinedDataResult,
    ProviderConfig, SemioscanConfig, Transfer,
};
use std::{
    borrow::Cow, env, error::Error as StdError, str::FromStr, time::Duration, time::Instant,
};
use tokio::time::sleep;
use tracing_subscriber::{fmt::Subscriber, EnvFilter};
use url::Url;

// Public on-chain defaults for the March 11, 2026 zkSync incident.
// Override these with ZKSYNC_PROBE_* env vars when probing other cases.
const INCIDENT_TX_HASH: &str = "0x09d047b22ceb10d30bd1a36969e45eb9f63b6d01f16439f4fd0b9f0114177cff";
const INCIDENT_START_BLOCK: u64 = 68_850_251;
const INCIDENT_END_BLOCK: u64 = 68_870_123;
const INCIDENT_FROM: &str = "0x0D05a7D3448512B78fa8A9e46c4872C88C4a0D05";
const INCIDENT_TO: &str = "0x5E1c87A1589BCC4325Db77Be49874941b2297a7B";
const INCIDENT_TOKEN: &str = "0x1d17CBcF0D6D143135aE902365D2E5e2A16538D4";

struct ProbeConfig {
    tx_hash: TxHash,
    start_block: u64,
    end_block: u64,
    from_address: Address,
    to_address: Address,
    token_address: Address,
    attempts: usize,
    delay: Duration,
}

struct TransferScanSummary {
    raw_logs: usize,
    decoded_logs: usize,
    target_tx_logs: usize,
    total_amount: U256,
}

fn env_value(keys: &[&str], default: &str) -> String {
    keys.iter()
        .find_map(|key| env::var(key).ok().filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| default.to_string())
}

fn parse_tx_hash(keys: &[&str], default: &str) -> Result<TxHash> {
    let raw = env_value(keys, default);
    TxHash::from_str(&raw).with_context(|| format!("failed to parse tx hash from `{raw}`"))
}

fn parse_address(keys: &[&str], default: &str) -> Result<Address> {
    let raw = env_value(keys, default);
    Address::from_str(&raw).with_context(|| format!("failed to parse address from `{raw}`"))
}

fn parse_u64(keys: &[&str], default: u64) -> Result<u64> {
    let raw = env_value(keys, &default.to_string());
    raw.parse::<u64>()
        .with_context(|| format!("failed to parse u64 from `{raw}`"))
}

fn parse_usize(keys: &[&str], default: usize) -> Result<usize> {
    let raw = env_value(keys, &default.to_string());
    raw.parse::<usize>()
        .with_context(|| format!("failed to parse usize from `{raw}`"))
}

fn load_config() -> Result<ProbeConfig> {
    Ok(ProbeConfig {
        tx_hash: parse_tx_hash(
            &["ZKSYNC_PROBE_TX_HASH", "ZKSYNC_TX_HASH"],
            INCIDENT_TX_HASH,
        )?,
        start_block: parse_u64(
            &["ZKSYNC_PROBE_START_BLOCK", "ZKSYNC_START_BLOCK"],
            INCIDENT_START_BLOCK,
        )?,
        end_block: parse_u64(
            &["ZKSYNC_PROBE_END_BLOCK", "ZKSYNC_END_BLOCK"],
            INCIDENT_END_BLOCK,
        )?,
        from_address: parse_address(
            &["ZKSYNC_PROBE_FROM_ADDRESS", "ZKSYNC_FROM_ADDRESS"],
            INCIDENT_FROM,
        )?,
        to_address: parse_address(
            &["ZKSYNC_PROBE_TO_ADDRESS", "ZKSYNC_TO_ADDRESS"],
            INCIDENT_TO,
        )?,
        token_address: parse_address(
            &["ZKSYNC_PROBE_TOKEN", "ZKSYNC_TEST_TOKEN", "ZKSYNC_TOKEN"],
            INCIDENT_TOKEN,
        )?,
        attempts: parse_usize(&["ZKSYNC_PROBE_ATTEMPTS"], 3)?,
        delay: Duration::from_millis(parse_u64(&["ZKSYNC_PROBE_DELAY_MS"], 250)?),
    })
}

fn collect_error_chain(error: &dyn StdError) -> Vec<String> {
    let mut chain = vec![error.to_string()];
    let mut source = error.source();

    while let Some(err) = source {
        chain.push(err.to_string());
        source = err.source();
    }

    chain
}

fn print_lookup_error(label: &str, error: &(dyn StdError + 'static), elapsed: Duration) {
    println!("  {label}: ERR after {} ms", elapsed.as_millis());
    println!("    chain: {:?}", collect_error_chain(error));
}

fn print_phase_error(label: &str, error: &(dyn StdError + 'static)) {
    println!("  {label}: ERR");
    println!("    chain: {:?}", collect_error_chain(error));
}

fn redact_rpc_url(rpc_url: &str) -> String {
    Url::parse(rpc_url)
        .map(|url| match url.port() {
            Some(port) => format!(
                "{}://{}:{port}",
                url.scheme(),
                url.host_str().unwrap_or("?")
            ),
            None => format!("{}://{}", url.scheme(), url.host_str().unwrap_or("?")),
        })
        .unwrap_or_else(|_| "<invalid-rpc-url>".to_string())
}

fn transfer_filter(config: &ProbeConfig) -> Filter {
    Filter::new()
        .event_signature(Transfer::SIGNATURE_HASH)
        .address(config.token_address)
        .from_block(config.start_block)
        .to_block(config.end_block)
        .topic1(config.from_address)
        .topic2(config.to_address)
}

async fn probe_transfer_scan<P>(provider: &P, config: &ProbeConfig) -> Result<TransferScanSummary>
where
    P: Provider<Ethereum> + Clone,
{
    let semioscan_config = SemioscanConfig::default();
    let chunk_size = semioscan_config
        .get_max_block_range(NamedChain::ZkSync)
        .as_u64();
    let started = Instant::now();
    let logs: Vec<RpcLog> = fetch_logs_chunked(provider, transfer_filter(config), chunk_size)
        .await
        .context("chunked transfer scan failed")?;
    let elapsed = started.elapsed();

    let mut decoded_logs = 0usize;
    let mut total_amount = U256::ZERO;
    for log in &logs {
        if let Ok(event) = Transfer::decode_log(&log.inner) {
            decoded_logs += 1;
            total_amount = total_amount.saturating_add(event.value);
        }
    }
    let matching_target_tx = logs
        .iter()
        .filter(|log| log.transaction_hash == Some(config.tx_hash))
        .count();

    println!(
        "  chunked transfer scan: ok in {} ms, chunk_size={}, raw_logs={}, decoded_logs={}, target_tx_logs={}, total_amount={}",
        elapsed.as_millis(),
        chunk_size,
        logs.len(),
        decoded_logs,
        matching_target_tx,
        total_amount
    );

    if matching_target_tx == 0 {
        println!("    target tx hash was not present in the log scan result");
    }

    Ok(TransferScanSummary {
        raw_logs: logs.len(),
        decoded_logs,
        target_tx_logs: matching_target_tx,
        total_amount,
    })
}

async fn probe_transaction_lookup<P>(provider: &P, config: &ProbeConfig) -> Result<()>
where
    P: Provider<Ethereum> + Clone,
{
    println!("  transaction lookups (typed Ethereum response):");
    for attempt in 1..=config.attempts {
        let started = Instant::now();
        match provider.get_transaction_by_hash(config.tx_hash).await {
            Ok(Some(transaction)) => {
                println!(
                    "    [{attempt}/{}] ok in {} ms, block={:?}, type={:?}",
                    config.attempts,
                    started.elapsed().as_millis(),
                    transaction.block_number(),
                    transaction.transaction_type()
                );
            }
            Ok(None) => {
                println!(
                    "    [{attempt}/{}] missing in {} ms",
                    config.attempts,
                    started.elapsed().as_millis()
                );
            }
            Err(error) => {
                let elapsed = started.elapsed();
                println!("    [{attempt}/{}]", config.attempts);
                print_lookup_error("tx lookup", &error, elapsed);
            }
        }

        if attempt < config.attempts && !config.delay.is_zero() {
            sleep(config.delay).await;
        }
    }

    Ok(())
}

async fn probe_raw_transaction_lookup<P>(provider: &P, config: &ProbeConfig) -> Result<()>
where
    P: Provider<Ethereum> + Clone,
{
    println!("  transaction lookups (permissive raw decode):");
    for attempt in 1..=config.attempts {
        let started = Instant::now();
        match provider
            .raw_request::<_, Option<AnyRpcTransaction>>(
                Cow::Borrowed("eth_getTransactionByHash"),
                (config.tx_hash,),
            )
            .await
        {
            Ok(Some(transaction)) => {
                println!(
                    "    [{attempt}/{}] ok in {} ms, type=0x{:x}",
                    config.attempts,
                    started.elapsed().as_millis(),
                    transaction.ty()
                );
            }
            Ok(None) => {
                println!(
                    "    [{attempt}/{}] missing in {} ms",
                    config.attempts,
                    started.elapsed().as_millis()
                );
            }
            Err(error) => {
                let elapsed = started.elapsed();
                println!("    [{attempt}/{}]", config.attempts);
                print_lookup_error("raw tx lookup", &error, elapsed);
            }
        }

        if attempt < config.attempts && !config.delay.is_zero() {
            sleep(config.delay).await;
        }
    }

    Ok(())
}

async fn probe_receipt_lookup<P>(provider: &P, config: &ProbeConfig) -> Result<()>
where
    P: Provider<Ethereum> + Clone,
{
    println!("  receipt lookups:");
    for attempt in 1..=config.attempts {
        let started = Instant::now();
        match provider.get_transaction_receipt(config.tx_hash).await {
            Ok(Some(receipt)) => {
                println!(
                    "    [{attempt}/{}] ok in {} ms, block={:?}, logs={}",
                    config.attempts,
                    started.elapsed().as_millis(),
                    receipt.block_number(),
                    receipt.logs().len()
                );
            }
            Ok(None) => {
                println!(
                    "    [{attempt}/{}] missing in {} ms",
                    config.attempts,
                    started.elapsed().as_millis()
                );
            }
            Err(error) => {
                let elapsed = started.elapsed();
                println!("    [{attempt}/{}]", config.attempts);
                print_lookup_error("receipt lookup", &error, elapsed);
            }
        }

        if attempt < config.attempts && !config.delay.is_zero() {
            sleep(config.delay).await;
        }
    }

    Ok(())
}

fn print_combined_summary(result: &CombinedDataResult) -> Result<()> {
    println!("  combined retrieval:");
    println!(
        "    tx_count={}, total_amount={}, total_gas_cost={}, is_partial={}",
        result.transaction_count,
        result.total_amount_transferred,
        result.overall_total_gas_cost,
        result.is_partial()
    );
    println!(
        "    skipped_logs={}, fallback_attempts={}, fallback_recovered={}",
        result.retrieval_metadata.skipped_logs,
        result.retrieval_metadata.fallback_attempts,
        result.retrieval_metadata.fallback_recovered
    );

    if result.is_partial() {
        let metadata = serde_json::to_string_pretty(&result.retrieval_metadata)
            .context("failed to serialize retrieval metadata")?;
        println!("    partial metadata:\n{metadata}");
    }

    Ok(())
}

async fn probe_combined_path<P>(provider: P, config: &ProbeConfig) -> Result<CombinedDataResult>
where
    P: Provider<Ethereum> + Send + Sync + Clone + 'static,
    <Ethereum as alloy_network::Network>::TransactionResponse:
        alloy_provider::network::eip2718::Typed2718 + Clone + Send + Sync,
    <Ethereum as alloy_network::Network>::ReceiptResponse: Clone + Send + Sync + std::fmt::Debug,
{
    let calculator = CombinedCalculator::new(provider);
    let started = Instant::now();
    let result = calculator
        .calculate_combined_data_ethereum(
            NamedChain::ZkSync,
            config.from_address,
            config.to_address,
            config.token_address,
            config.start_block,
            config.end_block,
        )
        .await
        .context("calculate_combined_data_ethereum failed")?;

    println!(
        "  combined retrieval finished in {} ms",
        started.elapsed().as_millis()
    );
    print_combined_summary(&result)?;

    Ok(result)
}

async fn run_probe(label: &str, rpc_url: &str, config: &ProbeConfig) -> Result<()> {
    println!("\n=== {label} ===");
    println!("rpc_url: {}", redact_rpc_url(rpc_url));

    let provider = create_typed_http_provider::<Ethereum>(ProviderConfig::new(rpc_url))
        .context("failed to create Ethereum-typed provider")?;

    println!(
        "target_tx: {}, block_window: {}..={}, token: {}, from: {}, to: {}",
        config.tx_hash,
        config.start_block,
        config.end_block,
        config.token_address,
        config.from_address,
        config.to_address
    );

    match provider.get_block_number().await {
        Ok(latest_block) => println!("latest_block: {latest_block}"),
        Err(error) => print_phase_error("get_block_number", &error),
    }

    let transfer_summary = match probe_transfer_scan(&provider, config).await {
        Ok(summary) => Some(summary),
        Err(error) => {
            print_phase_error("chunked transfer scan", error.as_ref());
            None
        }
    };

    probe_transaction_lookup(&provider, config).await?;
    probe_raw_transaction_lookup(&provider, config).await?;
    probe_receipt_lookup(&provider, config).await?;

    let combined_result = match probe_combined_path(provider.clone(), config).await {
        Ok(result) => Some(result),
        Err(error) => {
            print_phase_error("combined retrieval", error.as_ref());
            None
        }
    };

    if let (Some(scan), Some(combined)) = (transfer_summary, combined_result) {
        println!(
            "  comparison: scanned_total_amount={}, combined_total_amount={}, matches={}, scanned_decoded_logs={}, scanned_raw_logs={}, scanned_target_tx_logs={}",
            scan.total_amount,
            combined.total_amount_transferred,
            scan.total_amount == combined.total_amount_transferred,
            scan.decoded_logs,
            scan.raw_logs,
            scan.target_tx_logs
        );
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = Subscriber::builder()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .context("Failed to set tracing subscriber")?;

    dotenvy::dotenv().ok();

    let primary_rpc_url =
        env::var("ZKSYNC_RPC_URL").context("ZKSYNC_RPC_URL environment variable not set")?;
    let alternate_rpc_url = env::var("ZKSYNC_PROBE_ALT_RPC_URL")
        .or_else(|_| env::var("ZKSYNC_ALT_RPC_URL"))
        .ok();
    let config = load_config()?;

    run_probe("Primary zkSync provider", &primary_rpc_url, &config).await?;

    if let Some(rpc_url) = alternate_rpc_url {
        run_probe("Alternate zkSync provider", &rpc_url, &config).await?;
    }

    Ok(())
}
