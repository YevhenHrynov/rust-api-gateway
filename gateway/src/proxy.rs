use http::{Request, Response};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use crate::error::{ErrorResponse, GatewayError};

const TEXT_YAML: &str = "text/yaml";

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

        let uri = format!("{}{}{}", upstream_base, upstream_path, query).parse()?;

        let (mut parts, body) = req.into_parts();
        parts.uri = uri;
        parts.headers.remove(http::header::HOST);

        let proxied_req = Request::from_parts(parts, body.boxed());

        let resp = self
            .client
            .request(proxied_req)
            .await
            .map_err(|e| GatewayError::Proxy(e.to_string()))?;

        let (parts, body) = resp.into_parts();
        Ok(Response::from_parts(parts, body.boxed()))
    }
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
        .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
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

pub(crate) fn static_body(data: &'static str) -> ResponseBody {
    Full::new(Bytes::from_static(data.as_bytes()))
        .map_err(|never| match never {})
        .boxed()
}

pub(crate) fn json_response(body: &str) -> Result<Response<ResponseBody>, GatewayError> {
    Ok(Response::builder()
        .status(http::StatusCode::OK)
        .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
        .body(owned_body(body))?)
}

pub(crate) fn service_unavailable(service: &str) -> Result<Response<ResponseBody>, GatewayError> {
    json_error(
        http::StatusCode::SERVICE_UNAVAILABLE,
        &format!("service '{}' is currently unavailable", service),
    )
}

pub(crate) fn owned_body(data: &str) -> ResponseBody {
    Full::new(Bytes::copy_from_slice(data.as_bytes()))
        .map_err(|never| match never {})
        .boxed()
}
