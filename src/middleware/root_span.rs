use actix_web::HttpMessage;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use tracing::Span;
use tracing_actix_web::RootSpanBuilder;

use crate::authn::PrincipalContext;

use super::request_id::{CorrelationIdValue, RequestIdValue};

/// Custom root span builder that adds `request_id` and `principal` to the
/// per-request tracing span created by `TracingLogger`.
pub struct MregRootSpan;

impl RootSpanBuilder for MregRootSpan {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let request_id = request
            .extensions()
            .get::<RequestIdValue>()
            .map(|v| v.0.clone())
            .unwrap_or_default();

        let correlation_id = request
            .extensions()
            .get::<CorrelationIdValue>()
            .and_then(|v| v.0.clone())
            .unwrap_or_default();

        let principal = request
            .extensions()
            .get::<PrincipalContext>()
            .map(|context| context.principal.id.clone())
            .or_else(|| {
                request
                    .headers()
                    .get("X-Mreg-User")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "anonymous".to_string());

        let method = request.method().as_str();
        let path = request.path();

        tracing::info_span!(
            "http_request",
            correlation_id = %correlation_id,
            request_id = %request_id,
            principal = %principal,
            http.method = %method,
            http.target = %path,
            http.status_code = tracing::field::Empty,
        )
    }

    fn on_request_end<B>(span: Span, outcome: &Result<ServiceResponse<B>, actix_web::Error>) {
        match outcome {
            Ok(response) => {
                let status = response.status().as_u16();
                span.record("http.status_code", status);

                if response.status().is_server_error() {
                    tracing::error!(parent: &span, http.status_code = status, "request failed");
                } else if response.status().is_client_error() {
                    tracing::warn!(parent: &span, http.status_code = status, "client error");
                } else {
                    tracing::info!(parent: &span, http.status_code = status, "request completed");
                }
            }
            Err(error) => {
                let status = error.as_response_error().status_code().as_u16();
                span.record("http.status_code", status);
                tracing::error!(parent: &span, http.status_code = status, error = %error, "request error");
            }
        }
    }
}
