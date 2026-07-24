use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use xray_qbrute::backend::{ProgressReporter, SearchConfig, SearchError};
use xray_qbrute::candidate::{self, CandidateConfig, DEFAULT_DIFFICULTY_BITS, MAX_UUID_INDEX};
use xray_qbrute::search::{self, BackendKind};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "ebac62b9", value_name = "HEX8")]
    node_suffix: String,

    #[arg(long, default_value = "eb366895", value_name = "HEX8")]
    commit_last8: String,

    #[arg(long, value_enum, default_value_t = BackendKind::Auto)]
    backend: BackendKind,

    #[arg(long, default_value_t = MAX_UUID_INDEX, value_name = "COUNT")]
    max_index: u64,

    #[arg(long)]
    no_progress: bool,
}

fn parse_hex_u32(name: &str, value: &str) -> Result<u32, String> {
    if value.len() != 8 {
        return Err(format!(
            "{name} must contain exactly 8 hexadecimal characters"
        ));
    }
    u32::from_str_radix(value, 16)
        .map_err(|_| format!("{name} contains a non-hexadecimal character"))
}

fn make_progress_bar(max_index: u64, hidden: bool) -> ProgressBar {
    if hidden {
        return ProgressBar::hidden();
    }

    let progress = ProgressBar::new(max_index);
    progress.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({per_sec}) {msg}",
            )
            .expect("progress template is static and valid"),
    );
    progress
}

fn run(args: Args) -> Result<(), SearchError> {
    let commit =
        parse_hex_u32("--commit-last8", &args.commit_last8).map_err(SearchError::InvalidConfig)?;
    let node_suffix =
        parse_hex_u32("--node-suffix", &args.node_suffix).map_err(SearchError::InvalidConfig)?;

    let candidate = CandidateConfig {
        commit,
        node_suffix,
    };
    let config = SearchConfig {
        candidate,
        max_index: args.max_index,
        leading_zero_bits: DEFAULT_DIFFICULTY_BITS,
    };

    println!("===== xray-qbrute =====");
    println!("Threads: {}", rayon::current_num_threads());
    println!("COMMIT_LAST8: {}", args.commit_last8);
    println!("NODE_SUFFIX: {}", args.node_suffix);
    println!("Backend request: {}", args.backend);
    println!("Range: [0, {})", config.max_index);

    let progress_bar = make_progress_bar(config.max_index, args.no_progress);
    let callback_bar = progress_bar.clone();
    let progress: ProgressReporter = Arc::new(move |count| callback_bar.inc(count));
    let start = Instant::now();
    let outcome = search::search(&config, args.backend, progress)?;
    let elapsed = start.elapsed();

    for reason in &outcome.fallback_reasons {
        eprintln!("backend fallback: {reason}");
    }

    if let Some(device) = &outcome.device_name {
        println!("Device: {device}");
    }
    println!("Backend: {}", outcome.backend_name);
    let speed = outcome.processed as f64 / elapsed.as_secs_f64().max(f64::EPSILON) / 1e6;

    if let Some(hit) = outcome.hit {
        let bytes = candidate::candidate_bytes(hit.index, config.candidate);
        let hash = candidate::digest(hit.index, config.candidate);
        let first_word = candidate::digest_word(&hash);
        if !candidate::matches_leading_zero_bits(first_word, config.leading_zero_bits) {
            return Err(SearchError::Runtime(format!(
                "backend returned an invalid hit at index {}",
                hit.index
            )));
        }

        let uuid = candidate::bytes_to_uuid_string(&bytes);
        println!("\n===== FOUND =====");
        println!("UUID      : {uuid}");
        println!("answer    : /answer {uuid}");
        println!("hash[:10] : {}", hex::encode(&hash[..10]));
        println!("attempts  : {}", outcome.processed);
        println!("time      : {:.2}s", elapsed.as_secs_f64());
        println!("rate      : {speed:.1} M/s");
        progress_bar.finish_with_message("Found!");
    } else {
        println!("\n===== NOT FOUND =====");
        println!("searched  : {} candidates", outcome.processed);
        println!("time      : {:.2}s", elapsed.as_secs_f64());
        println!("rate      : {speed:.1} M/s");
        progress_bar.finish_with_message("Not found");
    }

    Ok(())
}

fn main() {
    let args = Args::parse();
    if let Err(error) = run(args) {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
}
