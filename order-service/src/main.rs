use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

const LISTEN_ADDR: &str = "127.0.0.1:3001";

#[derive(Serialize)]
struct Order {
    id: String,
    item: String,
    quantity: u32,
    status: String,
}

fn mock_order(id: &str) -> Order {
    Order {
        id: id.to_owned(),
        item: "RustBook".to_owned(),
        quantity: 8,
        status: "pending".to_owned(),
    }
}

async fn list_orders() -> Json<Vec<Order>> {
    Json(vec![mock_order("1"), mock_order("2")])
}

async fn create_order() -> (StatusCode, Json<Order>) {
    let id = uuid::Uuid::new_v4().to_string();
    (StatusCode::CREATED, Json(mock_order(&id)))
}

async fn get_order(Path(id): Path<String>) -> Json<Order> {
    Json(mock_order(&id))
}

async fn update_order(Path(id): Path<String>) -> Json<Order> {
    let mut order = mock_order(&id);
    order.status = "confirmed".to_owned();
    Json(order)
}

async fn delete_order(Path(_): Path<String>) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn health() -> StatusCode {
    StatusCode::OK
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/orders", get(list_orders).post(create_order))
        .route(
            "/v1/orders/{id}",
            get(get_order).put(update_order).delete(delete_order),
        );

    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR).await?;

    println!("order-service listening on {LISTEN_ADDR}");
    axum::serve(listener, app).await?;

    Ok(())
}
