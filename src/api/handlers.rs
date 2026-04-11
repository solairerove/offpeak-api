use crate::data::models::{AppData, ArrivalsData};
use crate::scoring::compute_monthly_index;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use std::sync::Arc;

pub async fn list_cities(State(data): State<Arc<AppData>>) -> Json<Vec<String>> {
    let mut slugs: Vec<String> = data.cities.keys().cloned().collect();
    slugs.sort();
    (&mut *slugs).sort();

    Json(slugs)
}

pub async fn get_city(
    Path(slug): Path<String>,
    State(data): State<Arc<AppData>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let city = data.cities.get(&slug).ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::to_value(city).unwrap()))
}

pub async fn get_city_weather(
    Path(slug): Path<String>,
    State(data): State<Arc<AppData>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let city = data.cities.get(&slug).ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::to_value(&city.weather).unwrap()))
}

#[derive(Deserialize)]
pub struct ArrivalsQuery {
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
}

pub async fn get_city_arrivals(
    Path(slug): Path<String>,
    Query(params): Query<ArrivalsQuery>,
    State(data): State<Arc<AppData>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let city = data.cities.get(&slug).ok_or(StatusCode::NOT_FOUND)?;

    if params.year_from.is_none() && params.year_to.is_none() {
        return Ok(Json(serde_json::to_value(&city.arrivals).unwrap()));
    }

    let year_from = params.year_from.unwrap_or(city.arrivals.years[0]);
    let year_to = params
        .year_to
        .unwrap_or(city.arrivals.years[city.arrivals.years.len() - 1]);

    let filtered_data: Vec<_> = city
        .arrivals
        .data
        .iter()
        .filter(|e| e.year >= year_from && e.year <= year_to)
        .cloned()
        .collect();

    let mut years: Vec<i32> = city
        .arrivals
        .years
        .iter()
        .copied()
        .filter(|&y| y >= year_from && y <= year_to)
        .collect();
    years.sort_unstable();

    let response = ArrivalsData {
        years,
        data: filtered_data.clone(),
        monthly_index: compute_monthly_index(&filtered_data),
    };

    Ok(Json(serde_json::to_value(&response).unwrap()))
}
