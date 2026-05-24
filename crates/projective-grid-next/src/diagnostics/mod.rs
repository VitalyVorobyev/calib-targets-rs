//! Typed event sink.
//!
//! [`NoOpSink`] is zero-cost; [`RecordingSink<F>`] captures every event for
//! tests and the bench harness. Counter aggregation lives in
//! [`stats::CounterStats`].
//!
//! Default: production callers either omit the sink entirely (the task
//! entry points accept `&mut NoOpSink` by default) or wire a
//! `RecordingSink` when they want a trace. The legacy `TopologicalStats`
//! counter-bag struct is replaced by a `RecordingSink → CounterStats`
//! pipeline that produces (most of) the same field layout.

pub mod events;
pub mod stats;

pub use events::{
    EdgeClass, Event, GrowRejectReason, MergeRejectReason, QuadRejectReason, Stage,
    ValidationReason,
};
pub use stats::CounterStats;

use crate::float::Float;

/// Receiver of typed pipeline events.
///
/// Implementors decide how to handle each event — drop it, accumulate
/// counters, push it onto a wire format, etc. The trait is generic over
/// the `Float` parameter so an `f32` pipeline and an `f64` pipeline have
/// distinct sink types and can't be mixed accidentally.
pub trait DiagnosticSink<F: Float> {
    /// Process a single event. Implementations should be cheap on the
    /// happy path; the production default is [`NoOpSink`], which
    /// monomorphises to an inline no-op.
    fn emit(&mut self, event: Event<F>);
}

/// Zero-cost sink. Used in production builds where diagnostics are
/// disabled.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpSink;

impl<F: Float> DiagnosticSink<F> for NoOpSink {
    #[inline]
    fn emit(&mut self, _event: Event<F>) {}
}

/// Sink that captures every event into a `Vec`. Useful for tests, the
/// bench harness, and any external tooling that needs the full trace.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RecordingSink<F: Float> {
    /// All events emitted so far, in arrival order.
    pub events: Vec<Event<F>>,
}

impl<F: Float> RecordingSink<F> {
    /// Construct an empty sink.
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Construct a sink with a pre-allocated capacity. Lets callers
    /// estimate the trace size to avoid intermediate reallocations.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
        }
    }

    /// Borrow the captured events.
    pub fn events(&self) -> &[Event<F>] {
        &self.events
    }

    /// Consume the sink and return the underlying `Vec<Event<F>>`.
    pub fn into_events(self) -> Vec<Event<F>> {
        self.events
    }

    /// Clear the recorded events.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl<F: Float> Default for RecordingSink<F> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: Float> DiagnosticSink<F> for RecordingSink<F> {
    #[inline]
    fn emit(&mut self, event: Event<F>) {
        self.events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::events::Stage;

    fn assert_noop_sink_is_cheap<F: Float>() {
        let mut sink: NoOpSink = NoOpSink;
        for _ in 0..100 {
            DiagnosticSink::<F>::emit(&mut sink, Event::StageStarted { stage: Stage::Seed });
        }
    }

    fn assert_recording_captures<F: Float>() {
        let mut sink = RecordingSink::<F>::new();
        sink.emit(Event::StageStarted { stage: Stage::Seed });
        sink.emit(Event::StageStarted { stage: Stage::Grow });
        sink.emit(Event::ComponentLabelled {
            id: 0,
            n_labels: 49,
        });
        assert_eq!(sink.events().len(), 3);
        let events = sink.into_events();
        assert!(matches!(
            events[2],
            Event::ComponentLabelled {
                id: 0,
                n_labels: 49
            }
        ));
    }

    fn assert_recording_clear<F: Float>() {
        let mut sink = RecordingSink::<F>::with_capacity(4);
        sink.emit(Event::StageStarted { stage: Stage::Seed });
        sink.clear();
        assert!(sink.events().is_empty());
    }

    #[test]
    fn noop_cheap_f32() {
        assert_noop_sink_is_cheap::<f32>();
    }
    #[test]
    fn noop_cheap_f64() {
        assert_noop_sink_is_cheap::<f64>();
    }
    #[test]
    fn recording_captures_f32() {
        assert_recording_captures::<f32>();
    }
    #[test]
    fn recording_captures_f64() {
        assert_recording_captures::<f64>();
    }
    #[test]
    fn recording_clear_f32() {
        assert_recording_clear::<f32>();
    }
    #[test]
    fn recording_clear_f64() {
        assert_recording_clear::<f64>();
    }
}
