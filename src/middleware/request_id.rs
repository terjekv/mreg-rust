use std::{
    future::{Future, Ready, ready},
    pin::Pin,
};

use actix_web::{
    Error, HttpMessage,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
};
use uuid::Uuid;

/// Middleware that generates a request ID (or uses `X-Request-Id` if provided)
/// and adds it to the response headers.
///
/// The request ID is stored as a request extension so downstream handlers
/// and tracing spans can access it.
pub struct RequestId;

impl<S, B> Transform<S, ServiceRequest> for RequestId
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = RequestIdMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequestIdMiddleware { service }))
    }
}

pub struct RequestIdMiddleware<S> {
    service: S,
}

/// Typed wrapper for a request ID, stored as a request extension.
#[derive(Clone, Debug)]
pub struct RequestIdValue(pub String);

/// Correlation ID propagated across services, stored as a request extension.
/// Empty when `X-Correlation-Id` is not provided.
#[derive(Clone, Debug)]
pub struct CorrelationIdValue(pub Option<String>);

impl<S, B> Service<ServiceRequest> for RequestIdMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let request_id = req
            .headers()
            .get("X-Request-Id")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let correlation_id = req
            .headers()
            .get("X-Correlation-Id")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        req.extensions_mut()
            .insert(RequestIdValue(request_id.clone()));
        req.extensions_mut()
            .insert(CorrelationIdValue(correlation_id.clone()));

        let fut = self.service.call(req);

        Box::pin(async move {
            let mut response = fut.await?;
            let headers = response.headers_mut();
            headers.insert(
                actix_web::http::header::HeaderName::from_static("x-request-id"),
                actix_web::http::header::HeaderValue::from_str(&request_id).unwrap_or_else(|_| {
                    actix_web::http::header::HeaderValue::from_static("unknown")
                }),
            );
            if let Some(ref cid) = correlation_id {
                headers.insert(
                    actix_web::http::header::HeaderName::from_static("x-correlation-id"),
                    actix_web::http::header::HeaderValue::from_str(cid).unwrap_or_else(|_| {
                        actix_web::http::header::HeaderValue::from_static("unknown")
                    }),
                );
            }
            Ok(response)
        })
    }
}
