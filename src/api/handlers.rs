use crate::data::models::{AppData, ArrivalsData};
use crate::scoring::{compute_monthly_index, compute_monthly_scores};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use std::sync::Arc;

#[derive(serde::Serialize)]
pub struct CityListItem {
    pub slug: String,
    pub name: String,
}

pub async fn list_cities(State(data): State<Arc<AppData>>) -> Json<Vec<CityListItem>> {
    let mut cities: Vec<CityListItem> = data.cities.values()
        .map(|c| CityListItem { slug: c.slug.clone(), name: c.city.clone() })
        .collect();
    cities.sort_by(|a, b| a.slug.cmp(&b.slug));

    Json(cities)
}

fn current_year() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    1970 + (secs / 31_557_600) as i32
}

#[derive(Deserialize)]
pub struct CityQuery {
    pub year: Option<i32>,
    pub year_from: Option<i32>,
    pub year_to: Option<i32>,
}

pub async fn get_city(
    Path(slug): Path<String>,
    Query(params): Query<CityQuery>,
    State(data): State<Arc<AppData>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let city = data.cities.get(&slug).ok_or(StatusCode::NOT_FOUND)?;
    let year = params.year.unwrap_or_else(current_year);
    let key = (slug.clone(), year, params.year_from, params.year_to);

    let cached = data.scores_cache.read().unwrap().get(&key).cloned();
    let scores = match cached {
        Some(s) => s,
        None => {
            let s = compute_monthly_scores(city, year, params.year_from, params.year_to);
            data.scores_cache.write().unwrap().insert(key, s.clone());
            s
        }
    };

    let mut value = serde_json::to_value(city).unwrap();
    value["monthly_scores"] = serde_json::to_value(scores).unwrap();

    Ok(Json(value))
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
