use serde::Serialize;
use std::collections::HashMap;

pub struct AppData {
    pub cities: HashMap<String, CityData>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CityData {
    pub city: String,
    pub slug: String,
    pub weather: Vec<WeatherMonth>,
    pub arrivals: ArrivalsData,
    pub holidays: Vec<Holiday>,
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeatherMonth {
    pub month: u8,
    pub avg_high_c: i32,
    pub avg_low_c: i32,
    pub humidity_pct: i32,
    pub rainfall_mm: i32,
    pub rain_days: i32,
    pub heat_index_c: i32,
    pub typhoon_risk: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrivalsData {
    pub years: Vec<i32>,
    pub data: Vec<ArrivalEntry>,
    pub monthly_index: Vec<MonthlyIndex>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrivalEntry {
    pub year: i32,
    pub month: i8,
    pub visitors_thousands: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthlyIndex {
    pub month: u8,
    pub normalized: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Holiday {
    pub name: String,
    pub typical_month_start: u8,
    pub typical_month_end: u8,
    pub crowd_impact: String,
    pub price_impact: String,
    pub closure_impact: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub category: String,
    pub text: String,
}
