use std::{
    future::{Future, Ready, ready},
    pin::Pin,
    rc::Rc,
};

use actix_web::{
    Error, HttpMessage, ResponseError,
    body::EitherBody,
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    http::header,
};

use crate::{AppState, errors::AppError};

pub struct Authn;

impl<S, B> Transform<S, ServiceRequest> for Authn
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = AuthnMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthnMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct AuthnMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AuthnMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let app_state = req.app_data::<actix_web::web::Data<AppState>>().cloned();
        let exempt = is_exempt_path(req.path());

        if exempt {
            let fut = self.service.call(req);
            return Box::pin(async move { fut.await.map(ServiceResponse::map_into_left_body) });
        }

        let service = self.service.clone();
        Box::pin(async move {
            let auth_result = async {
                let state = app_state.ok_or_else(|| AppError::internal("missing app state"))?;
                if !state.authn.requires_bearer_auth() {
                    return Ok(());
                }

                let token = bearer_token(req.headers())
                    .ok_or_else(|| AppError::unauthorized("missing Authorization: Bearer token"))?;
                let context = state.authn.authenticate_bearer(&token).await?;
                req.extensions_mut().insert(context);
                Ok::<(), AppError>(())
            }
            .await;

            match auth_result {
                Ok(()) => service
                    .call(req)
                    .await
                    .map(ServiceResponse::map_into_left_body),
                Err(error) => Ok(req.into_response(error.error_response().map_into_right_body())),
            }
        })
    }
}

fn bearer_token(headers: &actix_web::http::header::HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

fn is_exempt_path(path: &str) -> bool {
    matches!(
        path,
        "/api/v1/auth/login"
            | "/api/v1/system/health"
            | "/api/v1/system/version"
            | "/auth/login"
            | "/system/health"
            | "/system/version"
    ) || path.starts_with("/swagger-ui/")
        || path == "/api-docs/openapi.json"
}
