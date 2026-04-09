pub mod models;

use models::*;
use std::collections::HashMap;
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

fn parse_arrivals(path: &Path) -> Result<Vec<ArrivalRow>> {
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
    let weather_rows = parse_weather(&data_dir.join("Weather.csv"))?;
    let arrival_rows = parse_arrivals(&data_dir.join("Arrivals.csv"))?;
    let holiday_rows = parse_holidays(&data_dir.join("Holidays.csv"))?;
    let note_rows = parse_notes(&data_dir.join("Notes.csv"))?;

    let cities = build_cities(weather_rows, arrival_rows, holiday_rows, note_rows);
    Ok(AppData { cities })
}

fn build_cities(
    weather_rows: Vec<WeatherRow>,
    arrival_rows: Vec<ArrivalRow>,
    holiday_rows: Vec<HolidayRow>,
    note_rows: Vec<NoteRow>,
) -> HashMap<String, CityData> {
    let mut cities: HashMap<String, CityData> = HashMap::new();

    let general_notes: Vec<Note> = note_rows
        .iter()
        .filter(|r| r.city.trim().eq_ignore_ascii_case("general"))
        .map(|r| Note {
            category: r.category.to_lowercase(),
            text: r.note.clone(),
        })
        .collect();

    for row in &weather_rows {
        let slug = city_to_slug(&row.city);
        let entry = cities.entry(slug.clone()).or_insert_with(|| CityData {
            slug: slug.clone(),
            city: row.city.clone(),
            weather: vec![],
            arrivals: ArrivalsData {
                years: vec![],
                data: vec![],
                monthly_index: vec![],
            },
            holidays: vec![],
            notes: vec![],
        });
        entry.weather.push(WeatherMonth {
            month: month_str_to_num(&row.month) as u8,
            avg_high_c: row.avg_high_c,
            avg_low_c: row.avg_low_c,
            humidity_pct: row.humidity_pct,
            rainfall_mm: row.rainfall_mm,
            rain_days: row.rain_days,
            heat_index_c: row.heat_index_c,
            typhoon_risk: row.typhoon_risk.to_lowercase(),
            notes: row.notes.clone(),
        });
    }

    for row in &arrival_rows {
        let slug = city_to_slug(&row.city);
        if let Some(city) = cities.get_mut(&slug) {
            let year = row.year;
            if !city.arrivals.years.contains(&year) {
                city.arrivals.years.push(year);
            }
            city.arrivals.data.push(ArrivalEntry {
                year,
                month: month_str_to_num(&row.month),
                visitors_thousands: row.visitors_thousands,
            });
        }
    }

    for city in cities.values_mut() {
        city.arrivals.years.sort_unstable();
        city.arrivals.monthly_index = crate::scoring::compute_monthly_index(&city.arrivals.data);
    }

    for row in &holiday_rows {
        let slug = city_to_slug(&row.city);
        if let Some(city) = cities.get_mut(&slug) {
            let (start, end) = parse_typical_period(&row.typical_period);
            city.holidays.push(Holiday {
                name: row.holiday.clone(),
                typical_month_start: start,
                typical_month_end: end,
                crowd_impact: normalise_crowd(&row.crowd_impact).to_string(),
                price_impact: normalise_price(&row.price_impact).to_string(),
                closure_impact: normalise_closure(&row.closure_impact).to_string(),
                notes: row.notes.clone(),
            });
        }
    }

    for row in &note_rows {
        if row.city.trim().eq_ignore_ascii_case("general") {
            continue;
        }
        let slug = city_to_slug(&row.city);
        if let Some(city) = cities.get_mut(&slug) {
            city.notes.push(Note {
                category: row.category.to_lowercase(),
                text: row.note.clone(),
            });
        }
    }

    for city in cities.values_mut() {
        city.notes.extend(general_notes.clone());
    }

    cities
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
