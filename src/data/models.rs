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
    pub id: String,
    pub name: String,
    pub crowd_impact: String,
    pub price_impact: String,
    pub closure_impact: String,
    pub notes: String,
    /// All known occurrences, sorted by (year, date_start).
    /// Multiple entries with the same year are valid — some lunisolar events
    /// (e.g. Galungan on Bali's 210-day Pawukon cycle) fall twice in a
    /// Gregorian year.
    pub occurrences: Vec<HolidayOccurrence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HolidayOccurrence {
    pub year: i32,
    /// ISO 8601: "YYYY-MM-DD". Gregorian year of event start.
    pub date_start: String,
    /// ISO 8601: "YYYY-MM-DD". May be in year+1 for Dec→Jan events.
    pub date_end: String,
    /// Derived from date_start at load time; not stored in CSV.
    pub month_start: u8,
    /// Derived from date_end at load time; not stored in CSV.
    pub month_end: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub category: String,
    pub text: String,
}
