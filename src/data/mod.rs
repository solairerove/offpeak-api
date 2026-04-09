pub mod models;

use models::*;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct WeatherRow {
    city: String,
    month: String,
    avg_high_c: i32,
    avg_low_c: i32,
    humidity_pct: i32,
    rainfall_mm: i32,
    rain_days: i32,
    heat_index_c: i32,
    typhoon_risk: String,
    notes: String,
}

struct ArrivalRow {
    city: String,
    year: i32,
    month: String,
    visitors_thousands: i32,
}

struct HolidayRow {
    city: String,
    holiday: String,
    typical_period: String,
    crowd_impact: String,
    price_impact: String,
    closure_impact: String,
    notes: String,
}

struct NoteRow {
    city: String,
    category: String,
    note: String,
}

pub fn load_app_data(data_dir: &Path) -> Result<AppData> {
    todo!()
}

fn city_to_slug(city: &str) -> String {
    let base = match city.find('/') {
        Some(pos) => city[pos + 1..].trim(),
        None => city.trim(),
    };
    base.to_lowercase().replace(' ', "-")
}

fn month_str_to_num(s: &str) -> i8 {
    match s.trim() {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => 0,
    }
}

fn parse_typical_period(period: &str) -> (u8, u8) {
    const MONTHS: [(&str, u8); 12] = [
        ("Jan", 1),
        ("Feb", 2),
        ("Mar", 3),
        ("Apr", 4),
        ("May", 5),
        ("Jun", 6),
        ("Jul", 7),
        ("Aug", 8),
        ("Sep", 9),
        ("Oct", 10),
        ("Nov", 11),
        ("Dec", 12),
    ];

    let mut found: Vec<(usize, u8)> = MONTHS
        .iter()
        .filter_map(|(name, num)| period.find(name).map(|pos| (pos, *num)))
        .collect();

    found.sort_by_key(|(pos, _)| *pos);

    match found.as_slice() {
        [] => (0, 0),
        [(_, m)] => (*m, *m),
        [(_, m1), (_, m2), ..] => (*m1, *m2),
    }
}

fn normalise_crowd(s: &str) -> &str {
    match s.trim() {
        "Extreme" => "extreme",
        "Very High" => "very_high",
        "High" => "high",
        "Moderate" => "moderate",
        "Low" => "low",
        _ => "none",
    }
}

fn normalise_price(s: &str) -> &str {
    let t = s.trim();
    if t.starts_with("High") {
        "high"
    } else if t.starts_with("Moderate") {
        "moderate"
    } else {
        "none"
    }
}

fn normalise_closure(s: &str) -> &str {
    let t = s.trim();
    if t.is_empty() || t == "None" {
        "none"
    } else if t.starts_with("Minimal") || t.starts_with("Government") {
        "minimal"
    } else {
        "significant"
    }
}
