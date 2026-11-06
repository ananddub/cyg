//! Optional OpenTelemetry bridge for the `tracing` pipeline.
//!
//! Compiled only with `--features otel`, and even then it stays inert unless an
//! OTLP endpoint is configured. The default build has no OpenTelemetry code and
//! no exporter dependency in the binary, which keeps the 150 MiB image budget.
//!
//! Enable it with:
//!
//! ```text
//! cargo build --release --features otel
//! OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4318 \
//! OTEL_SERVICE_NAME=cyg-game \
//!   ./target/release/cyg-gaming run .
//! ```
//!
//! `OTEL_TRACES_SAMPLER` understands the standard names (`always_on`,
//! `always_off`, `traceidratio`, `parentbased_*`) with
//! `OTEL_TRACES_SAMPLER_ARG` for the ratio. `/metrics` and the compact/JSON
//! log layer are untouched; this only adds trace export.

type BoxLayer<S> = Box<dyn tracing_subscriber::Layer<S> + Send + Sync + 'static>;

/// Keeps the tracer provider alive for the life of the process and flushes the
/// batch processor on drop, so the last spans survive a clean shutdown.
#[derive(Default)]
pub struct Guard {
    #[cfg(feature = "otel")]
    provider: Option<opentelemetry_sdk::trace::SdkTracerProvider>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(feature = "otel")]
        if let Some(provider) = self.provider.take() {
            if let Err(err) = provider.shutdown() {
                eprintln!("[otel] tracer shutdown: {err}");
            }
        }
    }
}

#[cfg(not(feature = "otel"))]
pub fn layer<S>() -> (Option<BoxLayer<S>>, Guard) {
    (None, Guard::default())
}

#[cfg(feature = "otel")]
pub fn layer<S>() -> (Option<BoxLayer<S>>, Guard)
where
    S: tracing::Subscriber
        + for<'a> tracing_subscriber::registry::LookupSpan<'a>
        + Send
        + Sync
        + 'static,
{
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::trace::SdkTracerProvider;

    if std::env::var("OTEL_SDK_DISABLED").is_ok_and(|v| v.eq_ignore_ascii_case("true")) {
        return (None, Guard::default());
    }
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
        .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
        .ok()
        .filter(|v| !v.trim().is_empty());
    let Some(endpoint) = endpoint else {
        return (None, Guard::default());
    };

    use opentelemetry_otlp::WithExportConfig;
    let exporter = match opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint.trim())
        .build()
    {
        Ok(exporter) => exporter,
        Err(err) => {
            eprintln!("[otel] exporter disabled: {err}");
            return (None, Guard::default());
        }
    };

    let service = std::env::var("OTEL_SERVICE_NAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "cyg-gaming".to_string());
    let provider = SdkTracerProvider::builder()
        .with_resource(Resource::builder().with_service_name(service).build())
        .with_sampler(sampler())
        .with_batch_exporter(exporter)
        .build();

    let tracer = provider.tracer("cyg-gaming");
    let _ = opentelemetry::global::set_tracer_provider(provider.clone());
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
    (
        Some(Box::new(layer)),
        Guard {
            provider: Some(provider),
        },
    )
}

#[cfg(feature = "otel")]
fn sampler() -> opentelemetry_sdk::trace::Sampler {
    use opentelemetry_sdk::trace::Sampler;

    let name = std::env::var("OTEL_TRACES_SAMPLER")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let ratio = std::env::var("OTEL_TRACES_SAMPLER_ARG")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    match name.as_str() {
        "always_off" => Sampler::AlwaysOff,
        "traceidratio" => Sampler::TraceIdRatioBased(ratio),
        "parentbased_always_off" => Sampler::ParentBased(Box::new(Sampler::AlwaysOff)),
        "parentbased_traceidratio" => {
            Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio)))
        }
        "always_on" => Sampler::AlwaysOn,
        _ => Sampler::ParentBased(Box::new(Sampler::AlwaysOn)),
    }
}
