//! Minimal logger.
//!
//! The logger prints `[elapsed LEVEL] message` to stderr with a simple
//! elapsed-time prefix. Use `init_with_level` to install it once at startup.

use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

use log::{LevelFilter, Log, Metadata, Record};

#[cfg(feature = "tracing")]
use tracing_subscriber::fmt::format::FmtSpan;
#[cfg(feature = "tracing")]
use tracing_subscriber::util::SubscriberInitExt;
#[cfg(feature = "tracing")]
use tracing_subscriber::{fmt, EnvFilter};

struct SimpleLogger {
    level: LevelFilter,
    started: Instant,
}

impl Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let elapsed = self.started.elapsed().as_secs_f64();
        let mut stderr = std::io::stderr();
        let _ = writeln!(
            stderr,
            "[{:7.3}s {:>5}] {}",
            elapsed,
            record.level(),
            record.args()
        );
    }

    fn flush(&self) {}
}

static LOGGER: OnceLock<SimpleLogger> = OnceLock::new();

/// Install the simple logger with the provided level filter.
///
/// Calling this more than once is a no-op after the first successful
/// initialization.
pub fn init_with_level(level: LevelFilter) -> Result<(), log::SetLoggerError> {
    if LOGGER.get().is_none() {
        let logger = LOGGER.get_or_init(|| SimpleLogger {
            level,
            started: Instant::now(),
        });
        log::set_logger(logger)?;
        log::set_max_level(level);
    }
    Ok(())
}

#[cfg(feature = "tracing")]
pub fn init_tracing(json: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if json {
        let _ = fmt()
            .with_env_filter(filter)
            .with_span_events(FmtSpan::CLOSE)
            .json()
            .flatten_event(true)
            .finish()
            .try_init();
    } else {
        let _ = fmt()
            .with_env_filter(filter)
            .with_span_events(FmtSpan::CLOSE)
            .with_timer(fmt::time::Uptime::default())
            .finish()
            .try_init();
    }
}
