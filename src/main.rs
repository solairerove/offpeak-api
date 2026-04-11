use std::path::Path;
use std::sync::Arc;

mod api;
mod data;
mod scoring;

#[tokio::main]
async fn main() {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());

    let app_data = data::load_app_data(Path::new(&data_dir))
        .unwrap_or_else(|e| panic!("Failed to load app data from '{}': {}", data_dir, e));

    let city_count = app_data.cities.len();
    let app_data = Arc::new(app_data);

    let app = api::create_router(app_data);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind to address '{}': {}", addr, e));

    println!("offpeak-api: {} cities loaded", city_count);
    println!("Listening on http://{}", addr);

    axum::serve(listener, app).await.unwrap();
}
