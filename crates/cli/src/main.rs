use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use acta::TracingGuard;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use num_format::{Locale, ToFormattedString};
use tracing::{error, info, warn};
use xray_qbrute_core::backend::{ProgressReporter, SearchConfig, SearchError};
use xray_qbrute_core::candidate::{self, CandidateConfig, DEFAULT_DIFFICULTY_BITS, MAX_UUID_INDEX};
use xray_qbrute_core::search::{self, BackendKind};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(
        value_name = "HEX8",
        help = "Node suffix (last 8 hex digits of the node ID)"
    )]
    node_suffix: String,

    #[arg(
        long,
        default_value = "90243a74",
        value_name = "HEX8",
        help = "Commit hash (8 hex characters)"
    )]
    commit: String,

    #[arg(long, value_enum, default_value_t = BackendKind::Auto)]
    backend: BackendKind,

    #[arg(
        long,
        default_value_t = DEFAULT_DIFFICULTY_BITS,
        value_name = "BITS",
        value_parser = clap::value_parser!(u32).range(..=64)
    )]
    difficulty: u32,

    #[arg(long, default_value_t = 0, value_name = "COUNT")]
    threads: usize,

    #[arg(long, default_value_t = 0, value_name = "COUNT")]
    start: u64,

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

fn format_rate(rate_per_second: f64) -> String {
    const UNITS: [&str; 5] = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s"];
    let mut value = rate_per_second;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS.get(unit).copied().unwrap_or("TH/s"))
}

#[allow(clippy::print_stderr)]
#[allow(clippy::single_call_fn)]
fn init_logging() -> TracingGuard {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,wgpu=error"));
    let path_width: usize = env!("ACTA_PATH_WIDTH").parse().unwrap_or(40);
    let formatter = acta::Formatter::new().with_path_width(path_width);
    let config = acta::Config::builder()
        .with_filter(filter)
        .with_writers([acta::Writer::stderr().with_format(acta::Format::Compact(formatter))])
        .build();
    match acta::init(config) {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("error: failed to initialize logging: {error}");
            std::process::exit(2);
        }
    }
}

#[allow(clippy::cast_sign_loss, clippy::expect_used, clippy::print_stdout)]
fn main() {
    let args = Args::parse();
    let _logging = init_logging();
    if args.threads > 0
        && rayon::ThreadPoolBuilder::new()
            .num_threads(args.threads)
            .build_global()
            .is_err()
    {
        error!(threads = args.threads, "failed to configure --threads");
        std::process::exit(2);
    }

    let result = (|| -> Result<(), SearchError> {
        let commit = parse_hex_u32("--commit", &args.commit).map_err(SearchError::InvalidConfig)?;
        let node_suffix =
            parse_hex_u32("node-suffix", &args.node_suffix).map_err(SearchError::InvalidConfig)?;

        let config = SearchConfig {
            candidate: CandidateConfig {
                commit,
                node_suffix,
            },
            start_index: args.start,
            max_index: args.max_index,
            leading_zero_bits: args.difficulty,
        };

        info!(
            threads = rayon::current_num_threads(),
            commit = %args.commit,
            node_suffix = %args.node_suffix,
            backend = %args.backend,
            range_start = format!("{:#x}", config.start_index),
            range_end = format!("{:#x}", config.max_index),
            "xray-qbrute search starting"
        );

        let processed_total = Arc::new(AtomicU64::new(0));

        let prepared = search::prepare(&config, args.backend)?;
        let progress_bar = if args.no_progress {
            ProgressBar::hidden()
        } else {
            let bar = ProgressBar::new(1000);
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg}")
                    .expect("progress template is static and valid"),
            );
            bar
        };
        let callback_bar = progress_bar.clone();
        let callback_total = Arc::clone(&processed_total);
        let callback_start = Instant::now();
        let progress: ProgressReporter = Arc::new(move |count| {
            callback_total.fetch_add(count, Ordering::Relaxed);
            let total = callback_total.load(Ordering::Relaxed);
            let probability =
                -(-(total as f64) / 2_f64.powi(config.leading_zero_bits as i32)).exp_m1();
            callback_bar.set_position((probability * 1000.0).min(999.999) as u64);
            let rate = total as f64 / callback_start.elapsed().as_secs_f64().max(f64::EPSILON);
            callback_bar.set_message(format!(
                "{} candidates · P(hit) {:.2}% · {}",
                total.to_formatted_string(&Locale::en),
                probability * 100.0,
                format_rate(rate)
            ));
        });

        let interrupt_total = Arc::clone(&processed_total);
        let interrupt_start = config.start_index;
        let interrupt_bar = progress_bar.clone();
        ctrlc::set_handler(move || {
            let total = interrupt_total.load(Ordering::Relaxed);
            let next = (interrupt_start + total) & (MAX_UUID_INDEX - 1);
            interrupt_bar.suspend(|| {
                warn!(
                    "interrupted after {total} candidates; resume with --start {next} (0x{next:x})"
                );
            });
            std::process::exit(130);
        })
        .map_err(|error| {
            SearchError::Runtime(format!("failed to install Ctrl+C handler: {error}"))
        })?;

        let start = Instant::now();
        let outcome = prepared.search(&config, &progress)?;
        let wall_elapsed = start.elapsed();
        let elapsed = outcome.search_elapsed.unwrap_or(wall_elapsed);

        for reason in &outcome.fallback_reasons {
            warn!(reason = %reason, "backend fallback");
        }

        if let Some(device) = &outcome.device_name {
            info!(device = %device, "device selected");
        }
        info!(backend = %outcome.backend_name, "backend used");
        if let Some(kernel_config) = &outcome.kernel_config {
            info!(kernel = %kernel_config, "kernel configuration");
        }
        if let Some(tuning_elapsed) = outcome.tuning_elapsed {
            info!(
                elapsed_s = tuning_elapsed.as_secs_f64(),
                "GPU kernel tuning"
            );
        }
        let speed = outcome.evaluated as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
        let probability =
            -(-(outcome.evaluated as f64) / 2_f64.powi(config.leading_zero_bits as i32)).exp_m1();
        let expected_seconds =
            2_f64.powi(config.leading_zero_bits as i32) / speed.max(f64::EPSILON);

        if outcome.hit.is_some() {
            progress_bar.set_position(1000);
        }
        progress_bar.finish_with_message(if outcome.hit.is_some() {
            "Found!"
        } else {
            "Not found"
        });

        if let Some(hit) = outcome.hit {
            let hash = candidate::digest(hit, config.candidate);
            if !candidate::matches_leading_zero_bits(
                candidate::digest_word(&hash),
                config.leading_zero_bits,
            ) {
                return Err(SearchError::Runtime(format!(
                    "backend returned an invalid hit at index {hit}"
                )));
            }

            let uuid = uuid::Uuid::from_bytes(candidate::candidate_bytes(hit, config.candidate))
                .to_string();
            println!("\n===== FOUND =====");
            println!("UUID      : {uuid}");
            println!("answer    : /answer {uuid}");
            println!("hash[:10] : {}", hex::encode(&hash[..10]));
            println!("hit       : index {hit}");
            println!("evaluated : {} candidates", outcome.evaluated);
            println!("time      : {:.2}s", elapsed.as_secs_f64());
            println!("rate      : {}", format_rate(speed));
            println!("P(hit in range)   : {:.4}%", probability * 100.0);
            println!(
                "expected (1 hit)  : ~{:.1}s at current rate ({})",
                expected_seconds,
                format_rate(speed)
            );
        } else {
            println!("\n===== NOT FOUND =====");
            println!("evaluated : {} candidates", outcome.evaluated);
            println!("time      : {:.2}s", elapsed.as_secs_f64());
            println!("rate      : {}", format_rate(speed));
            println!("P(hit in range)   : {:.4}%", probability * 100.0);
            println!(
                "expected to hit   : ~{:.1}s at current rate ({})",
                expected_seconds,
                format_rate(speed)
            );
            let next = (config.start_index + outcome.evaluated) & (MAX_UUID_INDEX - 1);
            println!("resume            : --start {next}  (0x{next:x})");
        }

        Ok(())
    })();
    if let Err(error) = result {
        error!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod test {
    use super::format_rate;
    use num_format::{Locale, ToFormattedString};

    #[test]
    fn integer_grouping_adds_thousands_separators() {
        assert_eq!(0_u64.to_formatted_string(&Locale::en), "0");
        assert_eq!(999_u64.to_formatted_string(&Locale::en), "999");
        assert_eq!(1000_u64.to_formatted_string(&Locale::en), "1,000");
        assert_eq!(1_000_000_u64.to_formatted_string(&Locale::en), "1,000,000");
    }

    #[test]
    fn rate_below_1000_stays_in_hash_per_second() {
        assert_eq!(format_rate(0.0), "0.0 H/s");
        assert_eq!(format_rate(999.9), "999.9 H/s");
    }

    #[test]
    fn rate_scales_through_si_units() {
        assert_eq!(format_rate(1000.0), "1.0 kH/s");
        assert_eq!(format_rate(1_000_000.0), "1.0 MH/s");
        assert_eq!(format_rate(1_000_000_000.0), "1.0 GH/s");
        assert_eq!(format_rate(1_000_000_000_000.0), "1.0 TH/s");
    }

    #[test]
    fn rate_clamps_to_terahash_at_overflow() {
        assert_eq!(format_rate(1e15), "1000.0 TH/s");
    }
}
