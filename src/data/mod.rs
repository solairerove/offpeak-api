pub mod models;

use models::*;
use std::collections::HashMap;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// ── internal row types (not serialized) ──────────────────────────────────────

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

/// Intermediate struct for holidays.csv reference table.
struct HolidayRef {
    city_slug: String,
    name: String,
    crowd_impact: String,
    price_impact: String,
    closure_impact: String,
    notes: String,
}

/// Intermediate struct for occurrences.csv.
struct OccurrenceRow {
    holiday_id: String,
    year: i32,
    date_start: String,
    date_end: String,
    month_start: u8,
    month_end: u8,
}

struct NoteRow {
    city: String,
    category: String,
    note: String,
}

// ── CSV parsers ───────────────────────────────────────────────────────────────

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

/// Loads holidays_v2.csv (reference table) into a map keyed by holiday id.
fn parse_holidays(path: &Path) -> Result<HashMap<String, HolidayRef>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;
    let mut map = HashMap::new();
    for result in rdr.records() {
        let r = result?;
        let id = r[0].trim().to_string();
        map.insert(
            id.clone(),
            HolidayRef {
                city_slug: r[1].trim().to_string(),
                name: r[2].trim().to_string(),
                crowd_impact: normalise_crowd(r[3].trim()).to_string(),
                price_impact: normalise_price(r[4].trim()).to_string(),
                closure_impact: normalise_closure(r[5].trim()).to_string(),
                notes: r[6].trim().to_string(),
            },
        );
    }
    Ok(map)
}

/// Loads occurrences.csv. Multiple rows with the same (holiday_id, year) are
/// intentional — lunisolar events on sub-annual cycles (e.g. Galungan on
/// Bali's 210-day Pawukon cycle) produce two occurrences in a Gregorian year.
fn parse_occurrences(path: &Path) -> Result<Vec<OccurrenceRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;
    let mut rows = Vec::new();
    for result in rdr.records() {
        let r = result?;
        let date_start = r[2].trim().to_string();
        let date_end = r[3].trim().to_string();
        rows.push(OccurrenceRow {
            holiday_id: r[0].trim().to_string(),
            year: r[1].parse()?,
            month_start: month_from_iso(&date_start),
            month_end: month_from_iso(&date_end),
            date_start,
            date_end,
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

// ── public entry point ────────────────────────────────────────────────────────

pub fn load_app_data(data_dir: &Path) -> Result<AppData> {
    let weather_rows = parse_weather(&data_dir.join("Weather.csv"))?;
    let arrival_rows = parse_arrivals(&data_dir.join("Arrivals.csv"))?;
    let note_rows = parse_notes(&data_dir.join("Notes.csv"))?;
    let holiday_refs = parse_holidays(&data_dir.join("holidays_v2.csv"))?;
    let occurrence_rows = parse_occurrences(&data_dir.join("occurrences.csv"))?;

    let cities = build_cities(
        weather_rows,
        arrival_rows,
        holiday_refs,
        occurrence_rows,
        note_rows,
    );
    Ok(AppData { cities })
}

// ── city assembly ─────────────────────────────────────────────────────────────

fn build_cities(
    weather_rows: Vec<WeatherRow>,
    arrival_rows: Vec<ArrivalRow>,
    holiday_refs: HashMap<String, HolidayRef>,
    occurrence_rows: Vec<OccurrenceRow>,
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

    // ── 1. Build holidays: join reference table with occurrences ──────────────

    // Group occurrences by holiday_id. Multiple entries per (holiday_id, year)
    // are preserved — they represent separate occurrences within the same year.
    let mut occ_by_id: HashMap<String, Vec<HolidayOccurrence>> = HashMap::new();
    for row in &occurrence_rows {
        occ_by_id
            .entry(row.holiday_id.clone())
            .or_default()
            .push(HolidayOccurrence {
                year: row.year,
                date_start: row.date_start.clone(),
                date_end: row.date_end.clone(),
                month_start: row.month_start,
                month_end: row.month_end,
            });
    }

    // Startup validation: warn on data gaps, never panic.
    for id in occ_by_id.keys() {
        if !holiday_refs.contains_key(id) {
            println!(
                "warn: occurrences.csv has unknown holiday_id '{id}' (no matching holidays entry)"
            );
        }
    }

    // Build per-city holiday lists.
    let mut holidays_by_city: HashMap<String, Vec<Holiday>> = HashMap::new();
    for (id, href) in &holiday_refs {
        let mut occs = occ_by_id.remove(id).unwrap_or_default();
        if occs.is_empty() {
            println!("warn: holiday '{id}' has zero occurrences in occurrences.csv");
        }
        // Sort by year then date_start so multi-occurrence years are in
        // chronological order (important for the Galungan/Kuningan pattern).
        occs.sort_by(|a, b| a.year.cmp(&b.year).then(a.date_start.cmp(&b.date_start)));
        holidays_by_city
            .entry(href.city_slug.clone())
            .or_default()
            .push(Holiday {
                id: id.clone(),
                name: href.name.clone(),
                crowd_impact: href.crowd_impact.clone(),
                price_impact: href.price_impact.clone(),
                closure_impact: href.closure_impact.clone(),
                notes: href.notes.clone(),
                occurrences: occs,
            });
    }

    // ── 2. Weather ────────────────────────────────────────────────────────────

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
            monthly_scores: vec![],
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

    // ── 3. Arrivals ───────────────────────────────────────────────────────────

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

    // ── 4. Attach holidays to cities ──────────────────────────────────────────

    for (slug, holidays) in holidays_by_city {
        if let Some(city) = cities.get_mut(&slug) {
            city.holidays = holidays;
        } else {
            println!(
                "warn: holidays_v2.csv references city_slug '{slug}' not found in Weather.csv"
            );
        }
    }

    // ── 5. Notes ──────────────────────────────────────────────────────────────

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

// ── helpers ───────────────────────────────────────────────────────────────────

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

/// Extracts the month number from an ISO 8601 date string ("YYYY-MM-DD").
/// Returns 0 on malformed input; 0 is filtered by scoring logic.
fn month_from_iso(date: &str) -> u8 {
    date.split('-')
        .nth(1)
        .and_then(|m| m.parse().ok())
        .unwrap_or(0)
}

fn normalise_crowd(s: &str) -> &str {
    match s.trim() {
        v @ ("extreme" | "very_high" | "high" | "moderate" | "low" | "none") => v,
        _ => "none",
    }
}

fn normalise_price(s: &str) -> &str {
    match s.trim() {
        v @ ("high" | "moderate" | "low" | "none") => v,
        _ => "none",
    }
}

fn normalise_closure(s: &str) -> &str {
    match s.trim() {
        v @ ("significant" | "minimal" | "none") => v,
        _ => "none",
    }
}
