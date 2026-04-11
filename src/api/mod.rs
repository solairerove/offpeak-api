use crate::api::handlers::{get_city, get_city_arrivals, get_city_weather, list_cities};
use crate::data::models::AppData;
use axum::Router;
use axum::routing::get;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

pub mod handlers;

pub fn create_router(data: Arc<AppData>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/v1/cities", get(list_cities))
        .route("/api/v1/cities/{slug}", get(get_city))
        .route("/api/v1/cities/{slug}/weather", get(get_city_weather))
        .route("/api/v1/cities/{slug}/arrivals", get(get_city_arrivals))
        .with_state(data)
        .layer(cors)
}
