use crate::data::models::{AppData, ArrivalsData};
use crate::scoring::{compute_monthly_index, compute_monthly_scores};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use std::sync::Arc;

/// Deserializes `years` from a comma-separated string or a single value.
/// Use `?years=2018,2024,2025` or `?years=2026`.
fn deserialize_years<'de, D>(deserializer: D) -> Result<Vec<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.split(',')
        .map(|v| v.trim().parse::<i32>().map_err(serde::de::Error::custom))
        .collect()
}

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
    pub planning_year: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_years")]
    pub years: Vec<i32>,
}

pub async fn get_city(
    Path(slug): Path<String>,
    Query(params): Query<CityQuery>,
    State(data): State<Arc<AppData>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let city = data.cities.get(&slug).ok_or(StatusCode::NOT_FOUND)?;
    let year = params.planning_year.unwrap_or_else(current_year);
    let mut selected_years = params.years;
    selected_years.sort_unstable();
    selected_years.dedup();
    let key = (slug.clone(), year, selected_years.clone());

    let cached = data.scores_cache.read().unwrap().get(&key).cloned();
    let scores = match cached {
        Some(s) => s,
        None => {
            let s = compute_monthly_scores(city, year, &selected_years);
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
    #[serde(default, deserialize_with = "deserialize_years")]
    pub years: Vec<i32>,
}

pub async fn get_city_arrivals(
    Path(slug): Path<String>,
    Query(params): Query<ArrivalsQuery>,
    State(data): State<Arc<AppData>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let city = data.cities.get(&slug).ok_or(StatusCode::NOT_FOUND)?;

    if params.years.is_empty() {
        return Ok(Json(serde_json::to_value(&city.arrivals).unwrap()));
    }

    let mut selected_years = params.years;
    selected_years.sort_unstable();
    selected_years.dedup();

    let filtered_data: Vec<_> = city
        .arrivals
        .data
        .iter()
        .filter(|e| selected_years.contains(&e.year))
        .cloned()
        .collect();

    let mut years: Vec<i32> = city
        .arrivals
        .years
        .iter()
        .copied()
        .filter(|y| selected_years.contains(y))
        .collect();
    years.sort_unstable();

    let response = ArrivalsData {
        years,
        data: filtered_data.clone(),
        monthly_index: compute_monthly_index(&filtered_data),
    };

    Ok(Json(serde_json::to_value(&response).unwrap()))
}
