use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

const LISTEN_ADDR: &str = "127.0.0.1:3001";

#[derive(Clone)]
struct AppState {
    orders_available: Arc<AtomicBool>,
}

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

async fn list_orders(State(state): State<AppState>) -> Result<Json<Vec<Order>>, StatusCode> {
    if !state.orders_available.load(Ordering::Relaxed) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    Ok(Json(vec![mock_order("1"), mock_order("2")]))
}

async fn create_order(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<Order>), StatusCode> {
    if !state.orders_available.load(Ordering::Relaxed) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let id = uuid::Uuid::new_v4().to_string();
    Ok((StatusCode::CREATED, Json(mock_order(&id))))
}

#[derive(Deserialize)]
struct AvailabilityBody {
    available: bool,
}

async fn set_orders_availability(
    State(state): State<AppState>,
    Json(body): Json<AvailabilityBody>,
) -> StatusCode {
    state
        .orders_available
        .store(body.available, Ordering::Relaxed);
    StatusCode::OK
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
    let state = AppState {
        orders_available: Arc::new(AtomicBool::new(true)),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/orders", get(list_orders).post(create_order))
        .route(
            "/v1/orders/{id}",
            get(get_order).put(update_order).delete(delete_order),
        )
        .route("/admin/orders/availability", post(set_orders_availability))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR).await?;

    println!("order-service listening on {LISTEN_ADDR}");
    axum::serve(listener, app).await?;

    Ok(())
}
