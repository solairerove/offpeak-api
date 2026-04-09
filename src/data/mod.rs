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

fn parse_weather(path: &Path) -> Result<Vec<WeatherRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;
    let mut rows = Vec::new();
    for result in rdr.records() {
        let r = result?;
        rows.push(WeatherRow {
            city: r[0].to_string(),
            month: r[1].to_string(),
            avg_high_c: r[2].parse()?,
            avg_low_c: r[3].parse()?,
            humidity_pct: r[4].parse()?,
            rainfall_mm: r[5].parse()?,
            rain_days: r[6].parse()?,
            heat_index_c: r[7].parse()?,
            typhoon_risk: r[8].to_string(),
            notes: r[9].to_string(),
        });
    }

    Ok(rows)
}

fn parse_arrival(path: &Path) -> Result<Vec<ArrivalRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;
    let mut rows = Vec::new();
    for result in rdr.records() {
        let r = result?;
        rows.push(ArrivalRow {
            city: r[0].to_string(),
            year: r[1].parse()?,
            month: r[2].to_string(),
            visitors_thousands: r[3].parse()?,
        });
    }

    Ok(rows)
}

fn parse_holidays(path: &Path) -> Result<Vec<HolidayRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;
    let mut rows = Vec::new();
    for result in rdr.records() {
        let r = result?;
        rows.push(HolidayRow {
            city: r[0].to_string(),
            holiday: r[1].to_string(),
            typical_period: r[2].to_string(),
            crowd_impact: r[4].to_string(),
            price_impact: r[5].to_string(),
            closure_impact: r[6].to_string(),
            notes: r[7].to_string(),
        });
    }

    Ok(rows)
}

fn parse_notes(path: &Path) -> Result<Vec<NoteRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;
    let mut rows = Vec::new();
    for result in rdr.records() {
        let r = result?;
        rows.push(NoteRow {
            city: r[0].to_string(),
            category: r[1].parse()?,
            note: r[2].to_string(),
        });
    }

    Ok(rows)
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
