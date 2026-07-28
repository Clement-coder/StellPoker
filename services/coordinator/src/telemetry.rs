//! OpenTelemetry initialisation for the coordinator.
//!
//! Instruments end-to-end request tracing:
//!   frontend → coordinator → MPC nodes → Soroban
//!
//! ## Configuration (environment variables)
//!
//! | Variable | Default | Description |
//! |---|---|---|
//! | `OTEL_ENABLED` | `false` | Set to `true` / `1` / `yes` to enable |
//! | `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://localhost:4317` | OTLP gRPC endpoint (Jaeger / Grafana Tempo) |
//! | `OTEL_SERVICE_NAME` | `stellar-poker-coordinator` | Service name shown in traces |
//! | `OTEL_SAMPLE_RATE` | `1.0` | Fraction of traces to sample (0.0–1.0) |
//!
//! ## Usage
//!
//! Call [`init_tracer`] once at startup.  It returns a [`TracerGuard`] whose
//! `Drop` impl flushes and shuts down the exporter pipeline — keep it alive
//! for the duration of the process.
//!
//! ```rust,ignore
//! let _otel = telemetry::init_tracer();
//! // … run server …
//! // _otel is dropped here, flushing any pending spans
//! ```
//!
//! ## Propagation
//!
//! The [`trace_request`] middleware injects a `traceparent` header
//! (W3C TraceContext) into every outbound request so MPC nodes can
//! continue the same trace.

use opentelemetry::global;
use opentelemetry::trace::TraceError;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    runtime,
    trace::{self as sdktrace, Sampler},
    Resource,
};
use opentelemetry_semantic_conventions::resource::{SERVICE_NAME, SERVICE_VERSION};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// ── Public helpers ────────────────────────────────────────────────────────────

/// RAII guard returned by [`init_tracer`].
///
/// Dropping this value flushes all pending spans and shuts down the OTLP
/// pipeline.  Keep the guard alive for the entire lifetime of the process.
pub struct TracerGuard;

impl Drop for TracerGuard {
    fn drop(&mut self) {
        global::shutdown_tracer_provider();
        tracing::info!("OpenTelemetry tracer provider shut down");
    }
}

/// Initialise the global OpenTelemetry + tracing-subscriber stack.
///
/// When `OTEL_ENABLED` is not truthy this is a no-op (returns `None`) and
/// only the normal `tracing` subscriber is configured.  This keeps local
/// development free from any OTLP dependency.
///
/// Safe to call only once.  Calling it a second time will panic inside the
/// `tracing` crate (duplicate global subscriber).
pub fn init_tracer() -> Option<TracerGuard> {
    let enabled = is_otel_enabled();
    let json_logs = std::env::var("REQUEST_LOG_FORMAT")
        .unwrap_or_default()
        .eq_ignore_ascii_case("json");

    if !enabled {
        // No OTel — just set up the plain tracing subscriber.
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));

        let subscriber = tracing_subscriber::registry().with(filter);
        if json_logs {
            subscriber
                .with(tracing_subscriber::fmt::layer().json())
                .init();
        } else {
            subscriber.with(tracing_subscriber::fmt::layer()).init();
        }
        return None;
    }

    let pipeline = build_otlp_pipeline();
    match pipeline {
        Ok(tracer) => {
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"));

            let subscriber = tracing_subscriber::registry()
                .with(filter)
                .with(otel_layer);

            if json_logs {
                subscriber
                    .with(tracing_subscriber::fmt::layer().json())
                    .init();
            } else {
                subscriber.with(tracing_subscriber::fmt::layer()).init();
            }

            let endpoint = otel_endpoint();
            tracing::info!(endpoint = %endpoint, "OpenTelemetry tracing enabled");
            Some(TracerGuard)
        }
        Err(e) => {
            // OTel setup failure is not fatal — fall back to plain logging.
            eprintln!("WARNING: OpenTelemetry init failed: {e} — falling back to plain tracing");
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"));
            let subscriber = tracing_subscriber::registry().with(filter);
            if json_logs {
                subscriber
                    .with(tracing_subscriber::fmt::layer().json())
                    .init();
            } else {
                subscriber.with(tracing_subscriber::fmt::layer()).init();
            }
            None
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn is_otel_enabled() -> bool {
    let v = std::env::var("OTEL_ENABLED").unwrap_or_default();
    matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn otel_endpoint() -> String {
    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string())
}

fn service_name() -> String {
    std::env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| "stellar-poker-coordinator".to_string())
}

fn sample_rate() -> f64 {
    std::env::var("OTEL_SAMPLE_RATE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0)
}

fn build_otlp_pipeline() -> Result<opentelemetry_sdk::trace::Tracer, TraceError> {
    let sampler = if (sample_rate() - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else {
        Sampler::TraceIdRatioBased(sample_rate())
    };

    let resource = Resource::new(vec![
        opentelemetry::KeyValue::new(SERVICE_NAME, service_name()),
        opentelemetry::KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
    ]);

    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(otel_endpoint()),
        )
        .with_trace_config(
            sdktrace::Config::default()
                .with_sampler(sampler)
                .with_resource(resource),
        )
        .install_batch(runtime::Tokio)
}

// ── Span helpers used by middleware and handlers ──────────────────────────────

/// Extract the `traceparent` header value from a set of headers, if present.
/// Used by the request middleware to propagate the incoming trace context.
pub fn extract_traceparent(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Build a `traceparent` header value from the current span context so it can
/// be forwarded to MPC nodes.
///
/// Returns `None` when OTel is disabled or there is no active span.
pub fn current_traceparent() -> Option<String> {
    use opentelemetry::trace::{SpanContext, TraceContextExt};
    use opentelemetry::Context;

    let ctx = Context::current();
    let span = ctx.span();
    let sc: &SpanContext = span.span_context();
    if !sc.is_valid() {
        return None;
    }
    // W3C traceparent format: 00-<trace_id>-<span_id>-<flags>
    let trace_id = format!("{:032x}", sc.trace_id());
    let span_id = format!("{:016x}", sc.span_id());
    let flags = if sc.trace_flags().is_sampled() { "01" } else { "00" };
    Some(format!("00-{}-{}-{}", trace_id, span_id, flags))
}
