# STATE.md — offpeak-api handoff snapshot

Generated: 2026-04-12

---

## 1. Overview

`offpeak-api` is a read-only JSON REST API that serves travel-planning data (weather, tourist arrivals, holidays, and notes) for a small, fixed set of cities. It is written in Rust (edition 2024) using Axum 0.8 / Tokio 1. There is no database; all data is loaded once at startup from four CSV files into an in-memory `HashMap<String, CityData>` wrapped in `Arc`. Dependencies: `axum`, `tokio` (full), `serde`/`serde_json`, `csv`, `tower-http` (cors), `tower`. Deployed on Railway via a multi-stage Alpine Docker build.

---

## 2. How to run

```bash
# Dev
cargo run

# Release
cargo build --release
./target/release/offpeak-api

# Tests
cargo test
cargo test scoring::tests   # scoring module only
cargo test -- --nocapture   # with stdout

# Format / lint
cargo fmt
cargo fmt --check
cargo clippy --all-targets
```

**Environment variables** (both optional):

| Variable   | Default  | Description                            |
|------------|----------|----------------------------------------|
| `PORT`     | `3000`   | TCP port to listen on                  |
| `DATA_DIR` | `"data"` | Directory containing the four CSV files|

The server panics on startup if any CSV file is missing/unparseable, or if the port cannot be bound.

Railway injects `PORT` automatically; `DATA_DIR` is not set in `railway.toml`, so it uses the default `"data"`, which is baked into the Docker image at `/app/data/`.

---

## 3. Data model

All structs are in `src/data/models.rs`.

```rust
pub struct AppData {
    pub cities: HashMap<String, CityData>,
}
```
Top-level container. Key is city slug. Not serialized directly; only its contents are served.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct CityData {
    pub city: String,
    pub slug: String,
    pub weather: Vec<WeatherMonth>,
    pub arrivals: ArrivalsData,
    pub holidays: Vec<Holiday>,
    pub notes: Vec<Note>,
}
```
One entry per city. `city` is the display name from the CSV. `slug` is derived (see §7). Aggregates all four CSV sources.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct WeatherMonth {
    pub month: u8,          // 1–12, converted from "Jan"/"Feb"/... string
    pub avg_high_c: i32,
    pub avg_low_c: i32,
    pub humidity_pct: i32,
    pub rainfall_mm: i32,
    pub rain_days: i32,
    pub heat_index_c: i32,
    pub typhoon_risk: String,  // lowercased: "none","low","moderate","high"
    pub notes: String,
}
```
One row per month from `Weather.csv`. Twelve entries per city.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ArrivalsData {
    pub years: Vec<i32>,           // sorted unique years present in data
    pub data: Vec<ArrivalEntry>,   // raw rows
    pub monthly_index: Vec<MonthlyIndex>,  // computed; see §6
}
```
Aggregated from `Arrivals.csv`. `monthly_index` is computed at load time (not in the CSV).

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ArrivalEntry {
    pub year: i32,
    pub month: i8,               // 1–12; invalid months parse to 0 and are dropped by scoring
    pub visitors_thousands: i32,
}
```
One row per city/year/month from `Arrivals.csv`. Column header is "Visitors (thousands)" but only column index 3 is used.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct MonthlyIndex {
    pub month: u8,
    pub normalized: f64,  // 1.0–10.0, rounded to 1 decimal place
}
```
Computed field; see §6.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Holiday {
    pub name: String,
    pub typical_month_start: u8,   // 0 if unparseable
    pub typical_month_end: u8,     // same as start for single-month events
    pub crowd_impact: String,      // "extreme","very_high","high","moderate","low","none"
    pub price_impact: String,      // "high","moderate","none"
    pub closure_impact: String,    // "significant","minimal","none"
    pub notes: String,
}
```
From `Holidays.csv`. Impact strings are normalized; see §7 for normalization rules.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub category: String,  // lowercased from CSV
    pub text: String,
}
```
From `Notes.csv`. "General" city rows are appended to every city's notes list.

---

## 4. CSV schema

### Weather.csv

Columns (0-indexed, parsed by position):

| Index | Header         | Type     | Notes                              |
|-------|----------------|----------|------------------------------------|
| 0     | City           | String   | Display name; slug derived from it |
| 1     | Month          | String   | "Jan"–"Dec"; converted to u8       |
| 2     | Avg High °C    | i32      | `.parse()`                         |
| 3     | Avg Low °C     | i32      |                                    |
| 4     | Humidity %     | i32      |                                    |
| 5     | Rainfall mm    | i32      |                                    |
| 6     | Rain Days      | i32      |                                    |
| 7     | Heat Index °C  | i32      |                                    |
| 8     | Typhoon Risk   | String   | Stored lowercased                  |
| 9     | Notes          | String   | Free text                          |

Example row:
```
Da Nang,Jan,25,19,84,96,18,25,None,"Cool, dry-ish. Best months start"
```

### Arrivals.csv

Columns:

| Index | Header               | Type   | Notes                                   |
|-------|----------------------|--------|-----------------------------------------|
| 0     | City                 | String |                                         |
| 1     | Year                 | i32    |                                         |
| 2     | Month                | String | "Jan"–"Dec"                             |
| 3     | Visitors (thousands) | i32    | Column header includes "(thousands)"; only index 3 is read |
| 4     | Notes                | String | Present in CSV but NOT parsed/stored    |

Example row:
```
Hong Kong,2018,Jan,5100,
```

Quirk: column 4 (inline notes) exists in the CSV but is silently ignored by the parser. The `ArrivalRow` struct has no `notes` field.

### Holidays.csv

Columns:

| Index | Header            | Type   | Notes                                         |
|-------|-------------------|--------|-----------------------------------------------|
| 0     | Country/City      | String | May contain "/" — slug derived from after "/"  |
| 1     | Holiday           | String | Stored as `name`                              |
| 2     | Typical Period    | String | Free text; parsed for month names             |
| 3     | Duration          | String | Present in CSV but **skipped** (index 4 used for Crowds) |
| 4     | Impact: Crowds    | String | Normalized to enum string                     |
| 5     | Impact: Prices    | String | Normalized to enum string                     |
| 6     | Impact: Closures  | String | Normalized to enum string                     |
| 7     | Notes             | String |                                               |

Example row:
```
Hong Kong,Chinese New Year,Late Jan - Mid Feb,3-7 days,Extreme,High (+50-100%),Many shops closed 1-3 days,Worst time for budget travel. Great for atmosphere.
```

Quirk: Column 3 ("Duration") is **silently skipped**. The parser reads `r[4]` for crowd impact, jumping over index 3. Duration is not stored anywhere.

### Notes.csv

Columns:

| Index | Header       | Type   | Notes                                                     |
|-------|--------------|--------|-----------------------------------------------------------|
| 0     | City/General | String | "General" rows are broadcast to all cities                |
| 1     | Category     | String | Stored lowercased; uses `.parse()` (infallible for String)|
| 2     | Note         | String | Free text                                                 |

Example row:
```
Hong Kong,Transport,"Buy Octopus card at airport immediately. Works on MTR, buses, ferries, convenience stores."
```

---

## 5. API surface

Base path: `/api/v1`

CORS: `Allow-Origin: *`, `Allow-Methods: *`, `Allow-Headers: *` (all requests accepted from any origin).

---

### `GET /api/v1/cities`

Returns sorted list of all city slugs.

**Response:**
```json
["da-nang", "hong-kong"]
```

**Errors:** none.

Note: `list_cities` calls `.sort()` twice (bug — see §9).

---

### `GET /api/v1/cities/{slug}`

Returns full `CityData` for a city.

**Path params:** `slug` — city slug (e.g. `hong-kong`)

**Response:**
```json
{
  "city": "Hong Kong",
  "slug": "hong-kong",
  "weather": [
    {
      "month": 1,
      "avg_high_c": 18,
      "avg_low_c": 14,
      "humidity_pct": 74,
      "rainfall_mm": 33,
      "rain_days": 6,
      "heat_index_c": 18,
      "typhoon_risk": "none",
      "notes": "Cool and dry. Good month"
    }
  ],
  "arrivals": {
    "years": [2018, 2019, 2023, 2024],
    "data": [
      { "year": 2018, "month": 1, "visitors_thousands": 5100 }
    ],
    "monthly_index": [
      { "month": 1, "normalized": 7.2 }
    ]
  },
  "holidays": [
    {
      "name": "Chinese New Year",
      "typical_month_start": 1,
      "typical_month_end": 2,
      "crowd_impact": "extreme",
      "price_impact": "high",
      "closure_impact": "significant",
      "notes": "Worst time for budget travel. Great for atmosphere."
    }
  ],
  "notes": [
    { "category": "transport", "text": "Buy Octopus card at airport immediately." },
    { "category": "aviation", "text": "Razor blades (for safety/T-razors) prohibited in carry-on." }
  ]
}
```

**Errors:** `404` if slug not found.

---

### `GET /api/v1/cities/{slug}/weather`

Returns the weather array only (same as `CityData.weather`).

**Path params:** `slug`

**Response:** array of `WeatherMonth` objects (12 items). Same shape as `weather` field above.

**Errors:** `404` if slug not found.

---

### `GET /api/v1/cities/{slug}/arrivals`

Returns arrivals data, optionally filtered by year range. When either query param is provided, `monthly_index` is recomputed over the filtered data.

**Path params:** `slug`

**Query params:**

| Param       | Type | Required | Description                                      |
|-------------|------|----------|--------------------------------------------------|
| `year_from` | i32  | No       | Inclusive lower bound. Defaults to earliest year.|
| `year_to`   | i32  | No       | Inclusive upper bound. Defaults to latest year.  |

If **neither** param is present, returns the precomputed `ArrivalsData` verbatim (no filtering, precomputed index).

If **either or both** params are present, filters `data` and `years`, recomputes `monthly_index` from filtered data.

**Response:** `ArrivalsData` shape (same as `arrivals` field in full city response).

**Errors:** `404` if slug not found. Panics (unwrap) if `city.arrivals.years` is empty and a query param is provided (see §9).

---

## 6. Derived metrics

**`compute_monthly_index`** — `src/scoring.rs`

Input: `&[ArrivalEntry]`
Output: `Vec<MonthlyIndex>` (one entry per month that has data; months with no entries are absent)

Algorithm (verbatim from code):

```rust
// Step 1: accumulate totals and counts per month (indices 1–12; index 0 unused)
let mut totals = [0f64; 13];
let mut counts = [0u32; 13];

for entry in data {
    let m = entry.month as usize;
    if (1..=12).contains(&m) {
        totals[m] += entry.visitors_thousands as f64;
        counts[m] += 1;
    }
}

// Step 2: compute per-month average across all years
let averages: Vec<(u8, f64)> = (1u8..=12)
    .filter(|&m| counts[m as usize] > 0)
    .map(|m| (m, totals[m as usize] / counts[m as usize] as f64))
    .collect();

// Step 3: find global min and max of averages
let min = averages.iter().map(|&(_, v)| v).fold(f64::MAX, f64::min);
let max = averages.iter().map(|(_, v)| *v).fold(f64::MIN, f64::max);

// Step 4: normalize to 1.0–10.0; round to 1 decimal place
averages.iter().map(|(month, avg)| {
    let normalized = if (max - min).abs() < f64::EPSILON {
        5.0   // all months equal → midpoint
    } else {
        1.0 + 9.0 * (avg - min) / (max - min)
    };
    MonthlyIndex {
        month: *month,
        normalized: (normalized * 10.0).round() / 10.0,
    }
})
```

Formula: `normalized = 1.0 + 9.0 * (avg - min) / (max - min)`, min-month = 1.0, max-month = 10.0.

The function averages across years before normalizing, so a month with data across multiple years contributes a single average value, not one point per year.

---

## 7. Architecture decisions

- **Decision: Arc<AppData> as Axum state.** Reason: single allocation at startup; all handler calls borrow a cheap Arc clone. No locks needed because data is read-only after load.

- **Decision: Panic on startup failure.** Reason: if CSVs are absent or malformed the service is useless; Railway will restart and the error surfaces in logs immediately.

- **Decision: CSV parsed by column index, not header name.** Reason: the `csv` crate's `StringRecord` API is used; headers are checked (`has_headers(true)`) but not accessed by name. Brittle to column reordering.

- **Decision: `city_to_slug` strips the part before "/" in the city name.** Reason: `Holidays.csv` uses `"Vietnam / Da Nang"` style; the slug is derived from the suffix after the last `/`, trimmed and lowercased with spaces replaced by `-`. Weather and Arrivals use plain city names with no `/`.

- **Decision: Holidays column 3 (Duration) is silently skipped.** Reason: the parser jumps from `r[2]` (typical_period) directly to `r[4]` (crowd impact). No Duration field exists in the domain model.

- **Decision: General notes appended to every city.** Reason: `Notes.csv` rows with city "General" are collected separately and cloned onto each city's `notes` vec after city-specific notes. Order: city-specific first, then general.

- **Decision: `monthly_index` is precomputed at load time AND recomputed on filtered arrivals requests.** Reason: the stored value on `CityData` covers all years; the arrivals endpoint recomputes when year filtering is active. The two code paths produce identical results for the unfiltered case.

- **Decision: No logging framework.** Only two `println!` calls in `main.rs` (city count and listen address). No tracing/log crate.

- **Decision: CORS allows everything.** Reason: public read-only API; no credentials.

- **Decision: `typhoon_risk` stored lowercased; impact fields normalized to snake_case strings.** Reason: consistent JSON output regardless of CSV casing variations. Done in `build_cities`, not in parsing functions.

- **Decision: `month_str_to_num` returns `0` for unrecognized strings.** Entries with month = 0 are silently ignored by `compute_monthly_index` (the `(1..=12).contains(&m)` guard).

---

## 8. What is NOT implemented

- **No search or filtering on cities** — only slug-exact lookup. There is no fuzzy search, no filtering by country/region, no pagination.
- **No weather filtering** — `/weather` always returns all 12 months; no month or season filter.
- **No holiday endpoint** — holidays are only returned as part of the full city response (`GET /cities/{slug}`). There is no `/cities/{slug}/holidays`.
- **No notes endpoint** — same; notes only appear in the full city response.
- **No health check endpoint** — Railway uses `GET /api/v1/cities` as the health check (returns 200 with data, not a lightweight ping). There is no `/health` or `/ping` route.
- **Duration field from Holidays.csv** — present in the CSV, not stored or exposed anywhere.
- **Inline notes from Arrivals.csv** — column 4 of Arrivals.csv contains per-row notes; silently ignored.
- **No authentication or rate limiting.**
- **No city creation/update/delete** — entirely read-only; no write endpoints exist.
- **Only two cities in the dataset** (Da Nang, Hong Kong). The code is generic but the data is minimal.

---

## 9. Known issues and TODOs

1. **Double sort in `list_cities`** (`src/api/handlers.rs:12`): `.sort()` is called twice in sequence — `slugs.sort()` then `(&mut *slugs).sort()`. The second call is a no-op but shows confusion about ownership/dereferencing. No functional impact.

2. **Panic on empty years vec with query params** (`src/api/handlers.rs:52-55`): if a city exists but has no arrival data (`city.arrivals.years` is empty), accessing `city.arrivals.years[0]` or `city.arrivals.years[city.arrivals.years.len() - 1]` will panic with an index-out-of-bounds. Currently safe because the data has arrivals for both cities, but fragile.

3. **`serde_json::to_value(...).unwrap()` in all handlers** — will panic if serialization fails. Serialization of these structs cannot actually fail (no custom serializers, no maps with non-string keys), but using `unwrap()` means the error is not surfaced gracefully.

4. **No tracing/structured logging** — stdout only; no request IDs, no latency logging, no error logging for 404s.

5. **CSV parsed by column index** — column reordering in any CSV silently produces wrong data with no error.

6. **`month_str_to_num` returns `i8`; `WeatherMonth.month` stores it as `u8`** — the cast `month_str_to_num(&row.month) as u8` will wrap on the `0` sentinel value to `0u8`, which is consistent (0 is never a valid month), but the i8 return type is unnecessary.

7. **Comment noise in `scoring.rs`** — lines 15–22 and 29–36 contain extensive inline teaching comments about Rust iterator semantics and ownership. These are not harmful but add visual noise.

8. **`cargo fmt` formatting issue** — `src/scoring.rs` line 21 has a missing space before the inline comment (reported by `cargo fmt --check`).

---

## 10. File tree

```
src/
├── api/
│   ├── handlers.rs
│   └── mod.rs
├── data/
│   ├── models.rs
│   └── mod.rs
├── main.rs
└── scoring.rs
```
