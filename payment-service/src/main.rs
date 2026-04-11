use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::Serialize;

const LISTEN_ADDR: &str = "127.0.0.1:3002";

#[derive(Serialize)]
struct Payment {
    id: String,
    order_id: String,
    amount: f64,
    currency: String,
    status: String,
}

fn mock_payment(id: &str, status: &str) -> Payment {
    Payment {
        id: id.to_owned(),
        order_id: id.to_owned(),
        amount: 111.0,
        currency: "UAH".to_owned(),
        status: status.to_owned(),
    }
}

async fn create_payment(Path(id): Path<String>) -> (StatusCode, Json<Payment>) {
    (StatusCode::CREATED, Json(mock_payment(&id, "processing")))
}

async fn get_payment(Path(id): Path<String>) -> Json<Payment> {
    Json(mock_payment(&id, "completed"))
}

async fn refund_payment(Path(id): Path<String>) -> Json<Payment> {
    Json(mock_payment(&id, "refunded"))
}

async fn slow_payment(Path(id): Path<String>) -> Json<Payment> {
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    Json(mock_payment(&id, "completed"))
}

async fn health() -> StatusCode {
    StatusCode::OK
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/payments/{id}", post(create_payment).get(get_payment))
        .route("/v1/payments/{id}/refund", patch(refund_payment))
        .route("/v1/payments/{id}/slow", get(slow_payment));

    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR).await?;

    println!("payment-service listening on {LISTEN_ADDR}");
    axum::serve(listener, app).await?;

    Ok(())
}
