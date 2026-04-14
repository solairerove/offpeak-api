# STATE.md — offpeak-api handoff snapshot

Generated: 2026-04-14

---

## 1. Overview

`offpeak-api` is a read-only JSON REST API that serves travel-planning data (weather, tourist arrivals, holidays, pricing, and notes) for a small, fixed set of cities. It is written in Rust (edition 2024) using Axum 0.8 / Tokio 1. There is no database; all data is loaded once at startup from **six CSV files** into an in-memory `HashMap<String, CityData>` wrapped in `Arc`. `AppData` also holds a `RwLock`-guarded scores cache. Dependencies: `axum`, `tokio` (full), `serde`/`serde_json`, `csv`, `tower-http` (cors), `tower`. Deployed on Railway via a multi-stage Alpine Docker build.

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
| `DATA_DIR` | `"data"` | Directory containing the six CSV files |

The server panics on startup if any CSV file is missing/unparseable, or if the port cannot be bound.

Railway injects `PORT` automatically; `DATA_DIR` is not set in `railway.toml`, so it uses the default `"data"`, which is baked into the Docker image at `/app/data/`.

---

## 3. Data model

All structs are in `src/data/models.rs`.

```rust
pub type ScoresCacheKey = (String, i32, Vec<i32>);

pub struct AppData {
    pub cities: HashMap<String, CityData>,
    pub scores_cache: RwLock<HashMap<ScoresCacheKey, Vec<MonthScore>>>,
}
```
Top-level container. Key is city slug. `scores_cache` is an in-process cache keyed by `(slug, planning_year, years)`. Read path uses `RwLock::read()`; write path uses `RwLock::write()` on cache miss.

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
    pub pricing: Vec<PricingEntry>,
    pub monthly_scores: Vec<MonthScore>,
}
```
One entry per city. `monthly_scores` is precomputed at load time for the default (all years, current year) combination. The handler overwrites `monthly_scores` in the JSON response with a freshly computed (or cached) value for the requested `planning_year`/`years` combination.

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
One row per month from `weather.csv`. Twelve entries per city.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ArrivalsData {
    pub years: Vec<i32>,           // sorted unique years present in data
    pub data: Vec<ArrivalEntry>,   // raw rows
    pub monthly_index: Vec<MonthlyIndex>,  // computed; see §6
}
```

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ArrivalEntry {
    pub year: i32,
    pub month: i8,               // 1–12; invalid months parse to 0 and are dropped by scoring
    pub visitors_thousands: i32,
}
```

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct MonthlyIndex {
    pub month: u8,
    pub normalized: f64,  // 1.0–10.0, rounded to 1 decimal place
}
```

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Holiday {
    pub id: String,          // stable identifier from holidays.csv, e.g. "hk-cny"
    pub name: String,
    pub crowd_impact: String,   // "extreme","very_high","high","moderate","low","none"
    pub price_impact: String,   // "high","moderate","low","none"
    pub closure_impact: String, // "significant","minimal","none"
    pub notes: String,
    pub occurrences: Vec<HolidayOccurrence>,
}
```
Loaded by joining `holidays.csv` (reference table) with `occurrences.csv` (dated events). Impact strings are normalised at load time (see §7).

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct HolidayOccurrence {
    pub year: i32,
    pub date_start: String,  // ISO 8601: "YYYY-MM-DD"
    pub date_end: String,    // ISO 8601: "YYYY-MM-DD". May be year+1 for Dec→Jan events.
    pub month_start: u8,     // derived from date_start at load time
    pub month_end: u8,       // derived from date_end at load time
}
```
Multiple occurrences per year are valid — lunisolar events (e.g. Galungan on Bali's 210-day Pawukon cycle) can fall twice in a Gregorian year. Sorted by `(year, date_start)` at load time.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub category: String,  // lowercased from CSV
    pub text: String,
}
```
From `notes.csv`. "General" city rows are appended to every city's notes list.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct PricingEntry {
    pub year: i32,
    pub month: u8,
    pub price_index: f64,
}
```
From `pricing.csv`. Rows with an unparseable `price_index` are skipped with a `warn:` log; all other parse errors are fatal.

---

```rust
#[derive(Debug, Clone, Serialize)]
pub struct MonthScore {
    pub month: u8,
    pub comfort: i32,              // 2–10; sum of heat_score (1–5) + rain_score (1–5)
    pub crowd_index: f64,          // 1.0–10.0 from monthly_index; falls back to 5.0 if no data
    pub typhoon_penalty: f64,      // 0.0/0.5/2.0/6.0
    pub holiday_penalty: i32,      // 0–3; worst active holiday for planning_year
    pub price_index: Option<f64>,  // None if no pricing data
    pub price_penalty: Option<f64>,// None if no pricing data
    pub overall: f64,              // 1.0–10.0, rounded to 1 decimal
}
```
Computed by `scoring::compute_monthly_scores`. See §6 for formulas.

---

## 4. CSV schema

All files are in `DATA_DIR`. **File names are lowercase.**

### weather.csv

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

### arrivals.csv

Columns:

| Index | Header               | Type   | Notes                                         |
|-------|----------------------|--------|-----------------------------------------------|
| 0     | City                 | String |                                               |
| 1     | Year                 | i32    |                                               |
| 2     | Month                | String | "Jan"–"Dec"                                   |
| 3     | Visitors (thousands) | i32    |                                               |
| 4     | Notes                | String | Present in CSV but **NOT parsed/stored**      |

Column 4 (inline notes) is silently ignored.

### holidays.csv

Reference table — one row per holiday event type.

| Index | Header          | Type   | Notes                                        |
|-------|-----------------|--------|----------------------------------------------|
| 0     | id              | String | Stable identifier; foreign key from occurrences.csv |
| 1     | city_slug       | String | City slug, e.g. `hong-kong`                  |
| 2     | name            | String | Display name                                 |
| 3     | crowd_impact    | String | Normalised to enum string                    |
| 4     | price_impact    | String | Normalised to enum string                    |
| 5     | closure_impact  | String | Normalised to enum string                    |
| 6     | notes           | String | Free text                                    |

### occurrences.csv

Dated occurrences — joined onto `holidays.csv` at load time.

| Index | Header      | Type   | Notes                                          |
|-------|-------------|--------|------------------------------------------------|
| 0     | holiday_id  | String | Foreign key into `holidays.csv` id column      |
| 1     | year        | i32    |                                                |
| 2     | date_start  | String | ISO 8601 "YYYY-MM-DD"; `month_start` derived   |
| 3     | date_end    | String | ISO 8601 "YYYY-MM-DD"; `month_end` derived     |

Multiple rows with the same `(holiday_id, year)` are valid.

Validation: unknown `holiday_id` values (no matching `holidays.csv` row) print a `warn:` log at startup. Holiday refs with zero occurrences also print a `warn:`. Neither is fatal.

### pricing.csv

| Index | Header      | Type   | Notes                                          |
|-------|-------------|--------|------------------------------------------------|
| 0     | City        | String | Display name; slug derived                     |
| 1     | Year        | i32    |                                                |
| 2     | Month       | String | "Jan"–"Dec"                                    |
| 3     | price_index | f64    | Unparseable values skipped with `warn:` log    |

### notes.csv

| Index | Header       | Type   | Notes                                                     |
|-------|--------------|--------|-----------------------------------------------------------|
| 0     | City/General | String | "General" rows are broadcast to all cities                |
| 1     | Category     | String | Stored lowercased                                         |
| 2     | Note         | String | Free text                                                 |

---

## 5. API surface

Base path: `/api/v1`

CORS: `Allow-Origin: *`, `Allow-Methods: *`, `Allow-Headers: *`.

---

### `GET /api/v1/cities`

Returns sorted list of all cities as `[{slug, name}]` objects.

**Response:**
```json
[
  {"slug": "da-nang",   "name": "Da Nang"},
  {"slug": "hong-kong", "name": "Hong Kong"}
]
```

**Errors:** none.

---

### `GET /api/v1/cities/{slug}`

Returns full `CityData` for a city, with `monthly_scores` computed for the requested parameters.

**Path params:** `slug`

**Query params:**

| Param          | Type            | Required | Description                                                                    |
|----------------|-----------------|----------|--------------------------------------------------------------------------------|
| `planning_year`| i32             | No       | Year used for holiday lookups. Defaults to current calendar year.              |
| `years`        | comma-sep i32   | No       | Arrival years to use for crowd scoring. Empty = all years.                     |

**Response:** Full `CityData` shape; `monthly_scores` is always present and reflects the query params:

```json
{
  "city": "Hong Kong",
  "slug": "hong-kong",
  "weather": [...],
  "arrivals": { "years": [...], "data": [...], "monthly_index": [...] },
  "holidays": [
    {
      "id": "hk-cny",
      "name": "Chinese New Year",
      "crowd_impact": "extreme",
      "price_impact": "high",
      "closure_impact": "significant",
      "notes": "...",
      "occurrences": [
        {
          "year": 2025,
          "date_start": "2025-01-29",
          "date_end": "2025-02-12",
          "month_start": 1,
          "month_end": 2
        }
      ]
    }
  ],
  "notes": [...],
  "pricing": [...],
  "monthly_scores": [
    {
      "month": 1,
      "comfort": 9,
      "crowd_index": 7.2,
      "typhoon_penalty": 0.0,
      "holiday_penalty": 3,
      "price_index": 130.5,
      "price_penalty": 3.5,
      "overall": 5.8
    }
  ]
}
```

**Caching:** scores are cached in `AppData.scores_cache` keyed by `(slug, planning_year, years)`. Cache is never evicted (process lifetime).

**Errors:** `404` if slug not found.

---

### `GET /api/v1/cities/{slug}/weather`

Returns the weather array only.

**Response:** array of 12 `WeatherMonth` objects.

**Errors:** `404` if slug not found.

---

### `GET /api/v1/cities/{slug}/arrivals`

Returns arrivals data, optionally filtered by year list.

**Query params:**

| Param  | Type          | Required | Description                                               |
|--------|---------------|----------|-----------------------------------------------------------|
| `years`| comma-sep i32 | No       | Inclusive filter. `?years=2018,2024`. Empty = all years.  |

If `years` is absent or empty, returns the precomputed `ArrivalsData` verbatim. If provided, filters `data` and `years`, recomputes `monthly_index` from filtered data.

**Response:** `ArrivalsData` shape.

**Errors:** `404` if slug not found.

---

## 6. Derived metrics

All scoring logic is in `src/scoring.rs`.

### `compute_monthly_index(data: &[ArrivalEntry]) -> Vec<MonthlyIndex>`

Same algorithm as before: average arrivals per month across years, then min-max normalize to 1.0–10.0. Returns empty vec for empty input. Returns `[{month, normalized: 5.0}]` when all averages are equal (division-by-zero guard).

Formula: `normalized = 1.0 + 9.0 * (avg - min) / (max - min)`

### `compute_comfort_score(heat_index: i32, rain_days: i32) -> i32`

Returns sum of two independent 1–5 scores (range: 2–10).

**Heat score:**
| heat_index_c | score |
|---|---|
| ≤ 25 | 5 |
| 26–28 | 4 |
| 29–31 | 3 |
| 32–34 | 2 |
| ≥ 35 | 1 |

**Rain score:**
| rain_days | score |
|---|---|
| ≤ 7 | 5 |
| 8–12 | 4 |
| 13–16 | 3 |
| 17–20 | 2 |
| ≥ 21 | 1 |

### `compute_price_index(pricing: &[PricingEntry], month: u8, years: &[i32]) -> Option<f64>`

Averages `price_index` across all entries matching `month` (and `years` if non-empty). Returns `None` if no matching entries.

### `price_penalty(index: f64) -> f64`

| price_index | penalty |
|---|---|
| ≤ 70 | 0.0 |
| 71–90 | 1.0 |
| 91–110 | 2.0 |
| 111–130 | 3.5 |
| 131–160 | 5.5 |
| > 160 | 8.0 |

### `typhoon_penalty(risk: &str) -> f64` (private)

| risk | penalty |
|---|---|
| "none" | 0.0 |
| "low" | 0.5 |
| "moderate" | 2.0 |
| "high" | 6.0 |
| other | 0.0 |

### `get_worst_holiday_penalty(holidays: &[Holiday], month: u8, year: i32) -> i32`

Iterates all holidays, checks all occurrences where `occurrence.year == year`, checks if `month` falls within `[month_start, month_end]` (with Dec→Jan wrap support). Returns the worst penalty across active holidays:

| crowd_impact | penalty |
|---|---|
| "extreme" | 3 |
| "very_high" | 2 |
| "high" | 2 |
| "moderate" | 1 |
| other | 0 |

### `compute_overall_score(comfort, crowd, holiday_penalty, typhoon, pp) -> f64`

Two formulas depending on whether pricing data exists:

**With pricing (5 components):**
```
0.30 * comfort
+ 0.30 * (11.0 - crowd)
+ 0.15 * (10.0 - holiday_penalty)
+ 0.15 * (10.0 - typhoon_penalty)
+ 0.10 * (10.0 - price_penalty)
```

**Without pricing (4 components):**
```
0.35 * comfort
+ 0.35 * (11.0 - crowd)
+ 0.15 * (10.0 - holiday_penalty)
+ 0.15 * (10.0 - typhoon_penalty)
```

Result is clamped to [1.0, 10.0] and rounded to 1 decimal place.

### `compute_monthly_scores(city, year, years) -> Vec<MonthScore>`

Filters arrivals by `years`, calls `compute_monthly_index`, then for each month 1–12 assembles a `MonthScore` using all the functions above. Always returns exactly 12 entries.

---

## 7. Architecture decisions

- **Decision: Arc<AppData> as Axum state.** Reason: single allocation at startup; all handler calls borrow a cheap Arc clone.

- **Decision: RwLock for scores cache inside AppData.** Reason: `Arc` alone is immutable; the cache requires interior mutability. Read-heavy access pattern makes `RwLock` appropriate over `Mutex`. Data fields in `AppData` are still effectively read-only post-startup.

- **Decision: Panic on startup failure.** Reason: if CSVs are absent or malformed the service is useless; Railway will restart and the error surfaces in logs immediately. Exception: `pricing.csv` rows with unparseable `price_index` are silently skipped (logged as `warn:`); other malformed rows in any CSV are fatal.

- **Decision: CSV parsed by column index, not header name.** Reason: `StringRecord` API used throughout. Brittle to column reordering.

- **Decision: Holidays split into two CSV files.** `holidays.csv` is a reference table (one row per holiday type); `occurrences.csv` contains dated instances (one row per occurrence). Joined at load time. This supports lunisolar events that fall twice in a Gregorian year.

- **Decision: `city_to_slug` strips the part before "/" in the city name.** Reason: `holidays.csv` uses `"Vietnam / Da Nang"` style; slug is derived from the suffix after the last `/`, trimmed and lowercased with spaces replaced by `-`.

- **Decision: General notes appended to every city.** `Notes.csv` rows with city "General" are cloned onto each city's `notes` vec after city-specific notes.

- **Decision: `monthly_index` precomputed at load AND recomputed on filtered arrivals requests.** Same as before; two code paths produce identical results for the unfiltered case.

- **Decision: `monthly_scores` cached per `(slug, planning_year, years)`.** Computed on first request for a given combination, stored in `AppData.scores_cache`. Never evicted within a process lifetime.

- **Decision: `current_year()` computed from `SystemTime` without a time library.** Uses `1970 + secs / 31_557_600` (Julian year). Off by hours/days near year boundaries but acceptable for planning purposes.

- **Decision: No logging framework.** Only `println!` in `main.rs` and `data/mod.rs`. No tracing/log crate.

- **Decision: CORS allows everything.** Public read-only API; no credentials.

- **Decision: Impact fields normalised at parse time.** Unknown values fall back to `"none"`. Done via `normalise_crowd`/`normalise_price`/`normalise_closure` helpers in `data/mod.rs`.

---

## 8. What is NOT implemented

- **No search or filtering on cities** — only slug-exact lookup.
- **No weather filtering** — `/weather` always returns all 12 months.
- **No holiday endpoint** — holidays only in full city response.
- **No notes endpoint** — notes only in full city response.
- **No health check endpoint** — Railway uses `GET /api/v1/cities` (returns 200 with data, not a lightweight ping).
- **Duration field from old Holidays.csv** — the previous CSV had a Duration column; it was dropped in the schema redesign and is not stored anywhere.
- **Inline notes from Arrivals.csv** — column 4 silently ignored.
- **No authentication or rate limiting.**
- **No write endpoints** — entirely read-only.
- **Scores cache has no TTL or eviction** — grows unboundedly with unique `(slug, year, years)` combinations.

---

## 9. Known issues and TODOs

1. **`serde_json::to_value(...).unwrap()` in all handlers** — will panic if serialization fails. Serialization of these structs cannot actually fail (no custom serializers, no maps with non-string keys), but errors are not surfaced gracefully.

2. **Comment noise in `scoring.rs`** — lines 15–23 contain inline teaching comments about Rust iterator semantics. Not harmful but add visual noise.

3. **`cargo fmt` formatting issue** — `src/scoring.rs` line 21 has a missing space before the inline comment (reported by `cargo fmt --check`).

4. **`month_str_to_num` returns `i8`; `WeatherMonth.month` stores it as `u8`** — the cast `month_str_to_num(...) as u8` wraps the `0` sentinel to `0u8`, which is consistent, but the `i8` return type is unnecessary.

5. **No tracing/structured logging** — stdout only; no request IDs, no latency logging, no error logging for 404s.

6. **CSV parsed by column index** — column reordering in any CSV silently produces wrong data with no error.

7. **`current_year()` approximation** — uses Julian year seconds (`31_557_600`), not accounting for leap years precisely. Could return the wrong year for ~hours around Jan 1.

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

data/
├── weather.csv
├── arrivals.csv
├── holidays.csv
├── occurrences.csv
├── pricing.csv
└── notes.csv
```
