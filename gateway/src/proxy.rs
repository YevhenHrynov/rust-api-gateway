use std::time::Duration;

use http::{Request, Response};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use tracing::warn;

use crate::config::RetryConfig;
use crate::error::{ErrorResponse, GatewayError};

const TEXT_YAML: &str = "text/yaml";
const APPLICATION_JSON: &str = "application/json";

pub type ResponseBody = BoxBody<Bytes, hyper::Error>;

pub struct ProxyClient {
    client: Client<HttpConnector, ResponseBody>,
}

impl Default for ProxyClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxyClient {
    pub fn new() -> Self {
        let client = Client::builder(TokioExecutor::new()).build(HttpConnector::new());
        ProxyClient { client }
    }

    pub async fn forward(
        &self,
        req: Request<Incoming>,
        upstream_base: &str,
        upstream_path: &str,
    ) -> Result<Response<ResponseBody>, GatewayError> {
        let query = req
            .uri()
            .query()
            .map(|q| format!("?{}", q))
            .unwrap_or_default();

        let uri: http::Uri = format!("{}{}{}", upstream_base, upstream_path, query).parse()?;

        let (parts, body) = req.into_parts();
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| GatewayError::Proxy(e.to_string()))?
            .to_bytes();

        let mut headers = parts.headers.clone();
        headers.remove(http::header::HOST);

        let proxied_req = self.build_request(parts.method, uri, headers, body_bytes)?;
        let resp: Response<Incoming> = self
            .client
            .request(proxied_req)
            .await
            .map_err(|e| GatewayError::Proxy(e.to_string()))?;

        let (parts, body) = resp.into_parts();
        Ok(Response::from_parts(parts, body.boxed()))
    }

    pub async fn forward_with_retry(
        &self,
        req: Request<Incoming>,
        upstream_base: &str,
        upstream_path: &str,
        config: &RetryConfig,
    ) -> Result<Response<ResponseBody>, GatewayError> {
        let query = req
            .uri()
            .query()
            .map(|q| format!("?{}", q))
            .unwrap_or_default();

        let uri: http::Uri = format!("{}{}{}", upstream_base, upstream_path, query).parse()?;

        let (parts, body) = req.into_parts();
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| GatewayError::Proxy(e.to_string()))?
            .to_bytes();

        let mut headers = parts.headers.clone();
        headers.remove(http::header::HOST);

        let mut last_err = None;

        for attempt in 0..=config.max_retries {
            if attempt > 0 {
                let delay = Duration::from_millis(config.base_delay_ms * 2u64.pow(attempt - 1));
                warn!(
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    "retrying request"
                );
                tokio::time::sleep(delay).await;
            }

            let req = self.build_request(
                parts.method.clone(),
                uri.clone(),
                headers.clone(),
                body_bytes.clone(),
            )?;

            match self.client.request(req).await {
                Ok(resp) if resp.status().is_server_error() && attempt < config.max_retries => {
                    warn!(
                        attempt,
                        status = resp.status().as_u16(),
                        "upstream returned server error, will retry"
                    );
                    last_err = Some(GatewayError::Proxy(format!(
                        "upstream status {}",
                        resp.status()
                    )));
                }
                Ok(resp) => {
                    let (parts, body) = resp.into_parts();
                    return Ok(Response::from_parts(parts, body.boxed()));
                }
                Err(e) if attempt < config.max_retries => {
                    warn!(attempt, error = %e, "upstream connection failed, will retry");
                    last_err = Some(GatewayError::Proxy(e.to_string()));
                }
                Err(e) => {
                    return Err(GatewayError::Proxy(e.to_string()));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| GatewayError::Proxy("all retries exhausted".to_owned())))
    }

    fn build_request(
        &self,
        method: http::Method,
        uri: http::Uri,
        headers: http::HeaderMap,
        body: Bytes,
    ) -> Result<Request<ResponseBody>, GatewayError> {
        let mut builder = Request::builder().method(method).uri(uri);
        *builder
            .headers_mut()
            .ok_or_else(|| GatewayError::Proxy("failed to build request".to_owned()))? = headers;
        Ok(builder.body(Full::new(body).map_err(|never| match never {}).boxed())?)
    }
}

pub(crate) fn json_error_fallback(
    status: http::StatusCode,
    message: &str,
) -> Response<ResponseBody> {
    json_error(status, message).unwrap_or_else(|_| {
        let body = serde_json::to_string(&ErrorResponse {
            error: status.canonical_reason().unwrap_or("Unknown"),
            message,
            status: status.as_u16(),
        })
        .unwrap_or_default();
        let mut resp = Response::new(owned_body(&body));
        *resp.status_mut() = status;
        resp.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static(APPLICATION_JSON),
        );
        resp
    })
}

pub(crate) fn not_found() -> Result<Response<ResponseBody>, GatewayError> {
    json_error(http::StatusCode::NOT_FOUND, "not found")
}

pub(crate) fn bad_gateway(msg: &str) -> Result<Response<ResponseBody>, GatewayError> {
    json_error(http::StatusCode::BAD_GATEWAY, msg)
}

pub(crate) fn json_error(
    status: http::StatusCode,
    message: &str,
) -> Result<Response<ResponseBody>, GatewayError> {
    let body = ErrorResponse {
        error: status.canonical_reason().unwrap_or("Unknown"),
        message,
        status: status.as_u16(),
    };

    let json = serde_json::to_string(&body)?;

    Ok(Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, APPLICATION_JSON)
        .body(owned_body(&json))?)
}

pub(crate) fn redirect(location: &str) -> Result<Response<ResponseBody>, GatewayError> {
    Ok(Response::builder()
        .status(http::StatusCode::MOVED_PERMANENTLY)
        .header(http::header::LOCATION, location)
        .body(owned_body(""))?)
}

pub(crate) fn yaml_response(body: &str) -> Result<Response<ResponseBody>, GatewayError> {
    Ok(Response::builder()
        .status(http::StatusCode::OK)
        .header(http::header::CONTENT_TYPE, TEXT_YAML)
        .body(owned_body(body))?)
}

pub(crate) fn bytes_body(data: Vec<u8>) -> ResponseBody {
    Full::new(Bytes::from(data))
        .map_err(|never| match never {})
        .boxed()
}

pub(crate) fn json_response(body: &str) -> Result<Response<ResponseBody>, GatewayError> {
    Ok(Response::builder()
        .status(http::StatusCode::OK)
        .header(http::header::CONTENT_TYPE, APPLICATION_JSON)
        .body(owned_body(body))?)
}

pub(crate) fn service_unavailable(service: &str) -> Result<Response<ResponseBody>, GatewayError> {
    json_error(
        http::StatusCode::SERVICE_UNAVAILABLE,
        &format!("service '{}' is currently unavailable", service),
    )
}

pub(crate) fn circuit_open(service: &str) -> Result<Response<ResponseBody>, GatewayError> {
    json_error(
        http::StatusCode::SERVICE_UNAVAILABLE,
        &format!("circuit breaker open for service '{}'", service),
    )
}

pub(crate) fn owned_body(data: &str) -> ResponseBody {
    Full::new(Bytes::copy_from_slice(data.as_bytes()))
        .map_err(|never| match never {})
        .boxed()
}
