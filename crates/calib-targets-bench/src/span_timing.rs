//! Shared scaffolding for the `*_stage_timing` harnesses.
//!
//! Each harness installs a [`TimingLayer`], runs the production `detect` path
//! unchanged, and reads per-span busy time back out of [`SpanTotals`]. The
//! layer measures *busy* time — the sum of enter→exit intervals — so a parent
//! span's total includes its children and nested spans stay comparable.
//!
//! This is instrumentation only: the harnesses run production code paths and
//! do not alter detector output. Their JSON reports are local-only artifacts.

use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tracing::{Id, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::{LookupSpan, Registry};

/// Accumulated busy time per span name, summed across every span instance
/// closed since the last [`SpanTotals::clear`].
#[derive(Default)]
pub struct SpanTotals {
    totals: Mutex<HashMap<&'static str, Duration>>,
}

impl SpanTotals {
    /// Drop everything accumulated so far. Call before each timed repeat.
    pub fn clear(&self) {
        self.totals
            .lock()
            .expect("span totals mutex poisoned")
            .clear();
    }

    /// Current totals in milliseconds.
    pub fn snapshot_ms(&self) -> HashMap<&'static str, f64> {
        self.totals
            .lock()
            .expect("span totals mutex poisoned")
            .iter()
            .map(|(&name, &duration)| (name, duration.as_secs_f64() * 1000.0))
            .collect()
    }

    fn add(&self, name: &'static str, duration: Duration) {
        *self
            .totals
            .lock()
            .expect("span totals mutex poisoned")
            .entry(name)
            .or_default() += duration;
    }
}

/// Per-span-instance state the layer stashes in the span's extensions.
struct SpanTiming {
    name: &'static str,
    entered_at: Option<Instant>,
    elapsed: Duration,
}

/// A `tracing` layer that sums busy time per span name into [`SpanTotals`].
pub struct TimingLayer {
    totals: Arc<SpanTotals>,
}

impl TimingLayer {
    /// Install a fresh layer as the global subscriber and hand back the totals
    /// it writes into.
    ///
    /// Fails if a global subscriber is already set — only one harness may run
    /// per process.
    pub fn install() -> Result<Arc<SpanTotals>, tracing::subscriber::SetGlobalDefaultError> {
        let totals = Arc::new(SpanTotals::default());
        let subscriber = Registry::default().with(TimingLayer {
            totals: Arc::clone(&totals),
        });
        tracing::subscriber::set_global_default(subscriber)?;
        Ok(totals)
    }
}

impl<S> Layer<S> for TimingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanTiming {
                name: attrs.metadata().name(),
                entered_at: None,
                elapsed: Duration::ZERO,
            });
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(timing) = span.extensions_mut().get_mut::<SpanTiming>() {
                timing.entered_at = Some(Instant::now());
            }
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(timing) = span.extensions_mut().get_mut::<SpanTiming>() {
                if let Some(start) = timing.entered_at.take() {
                    timing.elapsed += start.elapsed();
                }
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            if let Some(timing) = span.extensions_mut().remove::<SpanTiming>() {
                self.totals.add(timing.name, timing.elapsed);
            }
        }
    }
}

/// Distribution summary of a set of per-repeat timings, in milliseconds.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct SummaryStats {
    /// Arithmetic mean.
    pub mean_ms: f64,
    /// Median.
    pub p50_ms: f64,
    /// 95th percentile.
    pub p95_ms: f64,
    /// Largest observed value.
    pub max_ms: f64,
}

/// Summarize per-repeat timings. Returns zeros for an empty input.
pub fn summarize(mut values: Vec<f64>) -> SummaryStats {
    if values.is_empty() {
        return SummaryStats::default();
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean_ms = values.iter().sum::<f64>() / values.len() as f64;
    let percentile = |q: f64| {
        let idx = ((values.len() - 1) as f64 * q).round() as usize;
        values[idx.min(values.len() - 1)]
    };
    SummaryStats {
        mean_ms,
        p50_ms: percentile(0.50),
        p95_ms: percentile(0.95),
        max_ms: *values.last().unwrap_or(&0.0),
    }
}

/// Busy time for one span name, or `0.0` if the span never ran.
pub fn span_ms(spans: &HashMap<&'static str, f64>, name: &str) -> f64 {
    spans.get(name).copied().unwrap_or(0.0)
}

/// Run a command and return its trimmed stdout, or `None` if it is
/// unavailable, fails, or prints nothing.
pub fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// Best-effort CPU model string for report provenance.
pub fn cpu_name() -> Option<String> {
    command_output("sysctl", &["-n", "machdep.cpu.brand_string"]).or_else(|| {
        command_output(
            "sh",
            &["-c", "lscpu | sed -n 's/^Model name:[[:space:]]*//p'"],
        )
    })
}
