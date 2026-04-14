# Scoring Migration: Backend

**Scope:** Move all score computation from frontend (`scoring.ts`) to backend (`scoring.rs`). Frontend becomes a display layer: receives pre-computed scores from API, maps values to colors, renders cells. No business logic on the client.

---

## Motivation

- Two implementations of the same logic (`scoring.rs` and `scoring.ts`) already diverge silently
- Typhoon bug: `typhoon_risk` existed in the data, was not wired into overall — only caught manually
- Adding pricing or AQI as score components means touching two codebases instead of one
- Scoring is the core product logic and belongs with tests, not in JSX

---

## What frontend stops doing

Remove from `src/lib/scoring.ts`:
- `computeComfortScore`
- `computeOverallScore`
- `getWorstHolidayPenalty`
- `typhoonRiskToScore`
- `typhoonPenalty`

Keep in `scoring.ts`:
- `computeMonthlyIndex` — still needed for dynamic year-range filtering client-side (see §Frontend)
- `getHolidaysForMonth` — still needed for filtering holidays by planning year
- Color helpers move to `colors.ts`, unchanged

---

## API changes

### New query parameter on `GET /api/v1/cities/{slug}`

```
GET /api/v1/cities/{slug}?year={year}
```

| Param  | Type | Required | Description                                                        |
|--------|------|----------|--------------------------------------------------------------------|
| `year` | i32  | No       | Planning year for holiday resolution. Defaults to current year.    |

When `year` is provided, each holiday in the response includes only the occurrence for that year (or no occurrence if none exists). Scores are computed using holidays resolved to that year. This replaces client-side `planningYear` holiday filtering.

### New field: `monthly_scores` on `CityData`

Added alongside existing fields. Computed at request time using the resolved `year` parameter.

```json
{
  "city": "Hong Kong",
  "slug": "hong-kong",
  "weather": [...],
  "arrivals": {...},
  "holidays": [...],
  "notes": [...],
  "monthly_scores": [
    {
      "month": 1,
      "comfort": 7,
      "crowd_index": 8.3,
      "typhoon_penalty": 0.0,
      "holiday_penalty": 3,
      "overall": 5.4
    }
  ]
}
```

`monthly_scores` has exactly 12 entries, one per month, sorted by month 1–12.

`crowd_index` is computed over **all years** (same as current precomputed `monthly_index`). Dynamic year-range filtering remains a client-side concern — see §Frontend.

### New struct — `src/data/models.rs`

```rust
#[derive(Debug, Clone, Serialize)]
pub struct MonthScore {
    pub month: u8,
    pub comfort: i32,          // 2–10
    pub crowd_index: f64,      // 1.0–10.0, 1 decimal
    pub typhoon_penalty: f64,  // 0 | 0.5 | 2.0 | 6.0
    pub holiday_penalty: i32,  // 0–3
    pub overall: f64,          // 1.0–10.0, 1 decimal
}
```

All component values exposed, not just overall. Frontend can display breakdowns without recomputing.

---

## Backend scoring logic — `src/scoring.rs`

### `compute_comfort_score(heat_index: i32, rain_days: i32) -> i32`

```rust
pub fn compute_comfort_score(heat_index: i32, rain_days: i32) -> i32 {
    let heat = if heat_index <= 25 { 5 }
               else if heat_index <= 28 { 4 }
               else if heat_index <= 31 { 3 }
               else if heat_index <= 34 { 2 }
               else { 1 };

    let rain = if rain_days <= 7  { 5 }
               else if rain_days <= 12 { 4 }
               else if rain_days <= 16 { 3 }
               else if rain_days <= 20 { 2 }
               else { 1 };

    heat + rain  // 2–10
}
```

### `typhoon_penalty(risk: &str) -> f64`

```rust
fn typhoon_penalty(risk: &str) -> f64 {
    match risk {
        "none"     => 0.0,
        "low"      => 0.5,
        "moderate" => 2.0,
        "high"     => 6.0,
        _          => 0.0,
    }
}
```

Private — not exported.

### `get_worst_holiday_penalty(holidays: &[Holiday], month: u8, year: i32) -> i32`

Takes resolved holidays (already filtered to the city). Finds holidays active in `month` for `year` using occurrence lookup. Returns worst penalty among active holidays.

```rust
pub fn get_worst_holiday_penalty(holidays: &[Holiday], month: u8, year: i32) -> i32 {
    let mut worst = 0i32;
    for h in holidays {
        let active = h.occurrences.iter().find(|o| o.year == year)
            .map(|o| {
                if o.month_start <= o.month_end {
                    month >= o.month_start && month <= o.month_end
                } else {
                    month >= o.month_start || month <= o.month_end  // Dec→Jan wrap
                }
            })
            .unwrap_or(false);

        if active {
            let p = match h.crowd_impact.as_str() {
                "extreme"   => 3,
                "very_high" => 2,
                "high"      => 2,
                "moderate"  => 1,
                _           => 0,
            };
            worst = worst.max(p);
        }
    }
    worst
}
```

### `compute_overall_score(comfort: i32, crowd: f64, holiday_penalty: i32, typhoon: &str) -> f64`

```rust
pub fn compute_overall_score(
    comfort: i32,
    crowd: f64,
    holiday_penalty: i32,
    typhoon: &str,
) -> f64 {
    let tp = typhoon_penalty(typhoon);
    let raw = 0.35 * comfort as f64
            + 0.35 * (11.0 - crowd)
            + 0.15 * (10.0 - holiday_penalty as f64)
            + 0.15 * (10.0 - tp);
    let clamped = raw.max(1.0).min(10.0);
    (clamped * 10.0).round() / 10.0
}
```

### `compute_monthly_scores(city: &CityData, year: i32) -> Vec<MonthScore>`

```rust
pub fn compute_monthly_scores(city: &CityData, year: i32) -> Vec<MonthScore> {
    let monthly_index = compute_monthly_index(&city.arrivals.data);  // all years

    (1u8..=12).map(|month| {
        let weather = city.weather.iter().find(|w| w.month == month).unwrap();
        let crowd = monthly_index.iter()
            .find(|m| m.month == month)
            .map(|m| m.normalized)
            .unwrap_or(5.0);

        let comfort = compute_comfort_score(weather.heat_index_c, weather.rain_days);
        let hp = get_worst_holiday_penalty(&city.holidays, month, year);
        let overall = compute_overall_score(comfort, crowd, hp, &weather.typhoon_risk);

        MonthScore {
            month,
            comfort,
            crowd_index: crowd,
            typhoon_penalty: typhoon_penalty(&weather.typhoon_risk),
            holiday_penalty: hp,
            overall,
        }
    }).collect()
}
```

### Tests — `src/scoring.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comfort_extremes() {
        assert_eq!(compute_comfort_score(39, 15), 4);  // heat=1, rain=3
        assert_eq!(compute_comfort_score(22, 3),  10); // heat=5, rain=5
    }

    #[test]
    fn typhoon_penalty_values() {
        assert_eq!(typhoon_penalty("none"),     0.0);
        assert_eq!(typhoon_penalty("low"),      0.5);
        assert_eq!(typhoon_penalty("moderate"), 2.0);
        assert_eq!(typhoon_penalty("high"),     6.0);
        assert_eq!(typhoon_penalty("unknown"),  0.0);
    }

    #[test]
    fn overall_high_typhoon_depresses_score() {
        let without = compute_overall_score(8, 3.0, 0, "none");
        let with_high = compute_overall_score(8, 3.0, 0, "high");
        assert!(with_high < without);
        // 0.15 * (10 - 6) = 0.6 difference
        assert!((without - with_high - 0.6).abs() < 0.05);
    }

    #[test]
    fn overall_clamped_to_1_10() {
        let low = compute_overall_score(2, 10.0, 3, "high");
        let high = compute_overall_score(10, 1.0, 0, "none");
        assert!(low >= 1.0);
        assert!(high <= 10.0);
    }

    #[test]
    fn holiday_penalty_dec_jan_wrap() {
        // christmas-hk: month_start=12, month_end=1
        // month 1 (January) should be active
        // tested via get_worst_holiday_penalty with a mock holiday
    }
}
```

---

## Handler changes — `src/api/handlers.rs`

`get_city` handler gains optional `year` query param. Default = current year via `chrono` or manual extraction from system time. If `chrono` is not a dependency, derive year from `std::time::SystemTime`.

```rust
#[derive(Deserialize)]
pub struct CityQuery {
    pub year: Option<i32>,
}

pub async fn get_city(
    Path(slug): Path<String>,
    Query(params): Query<CityQuery>,
    State(data): State<Arc<AppData>>,
) -> impl IntoResponse {
    let year = params.year.unwrap_or_else(current_year);
    // ...
    let scores = compute_monthly_scores(&city, year);
    // attach to response
}
```

`current_year()`:
```rust
fn current_year() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Approximate: good enough for a default year
    1970 + (secs / 31_557_600) as i32
}
```

---

## Frontend changes (offpeak-web)

### src/types.ts

Add new type:

```typescript
export interface MonthScore {
  month: number;
  comfort: number;
  crowd_index: number;
  typhoon_penalty: number;
  holiday_penalty: number;
  overall: number;
}
```

Add `monthly_scores: MonthScore[]` to `CityData`.

### src/api.ts

Pass `planningYear` as query param:

```typescript
export function fetchCity(slug: string, year: number): Promise<CityData> {
  return fetch(`${API_URL}/api/v1/cities/${slug}?year=${year}`)
    .then(r => { if (!r.ok) throw new Error(r.statusText); return r.json(); });
}
```

`fetchCity` is now called with `planningYear`. When `planningYear` changes, the city is re-fetched (not re-used from cache — scores depend on year). The cache key becomes `${slug}:${year}`.

### src/lib/scoring.ts

**Remove:** `computeComfortScore`, `computeOverallScore`, `getWorstHolidayPenalty`, `typhoonRiskToScore`, `typhoonPenalty`.

**Keep:** `computeMonthlyIndex`, `getHolidaysForMonth`.

`computeMonthlyIndex` stays on the frontend because crowd_index from the API is computed over all years, and the year-range selector is a live interactive filter — re-fetching on every year toggle would be wasteful. The frontend recomputes crowd_index from raw `arrivals.data` filtered to `selectedYears`, same as now. This is the one intentional exception to "no logic on frontend".

### src/components/Heatmap.tsx

`scores` useMemo becomes a simple lookup instead of computation:

```typescript
const scores = useMemo(() => {
  return city.monthly_scores.map(ms => {
    // crowd_index overridden with client-side computed value for selected years
    const crowd = monthlyIndex.find(m => m.month === ms.month)?.normalized ?? ms.crowd_index;
    // overall recomputed only because crowd changed
    const overall = computeOverallFromComponents(ms.comfort, crowd, ms.holiday_penalty, ms.typhoon_penalty);
    return { ...ms, crowd_index: crowd, overall };
  });
}, [city.monthly_scores, monthlyIndex]);
```

`computeOverallFromComponents` is a minimal helper that applies the same formula as the backend but only for the crowd substitution case:

```typescript
function computeOverallFromComponents(
  comfort: number,
  crowd: number,
  holidayPenalty: number,
  typhoonPenalty: number,
): number {
  const raw = 0.35 * comfort + 0.35 * (11 - crowd) + 0.15 * (10 - holidayPenalty) + 0.15 * (10 - typhoonPenalty);
  return Math.round(Math.max(1, Math.min(10, raw)) * 10) / 10;
}
```

This is unavoidable duplication caused by the interactive year filter. It is small, isolated, and the inputs (component penalties) come from the backend — the frontend does not compute penalties independently.

### src/components/MonthDetail.tsx

Scores section reads directly from `monthly_scores` for the selected month. No local computation. Shows comfort, crowd_index, overall — and now also typhoon_penalty and holiday_penalty as informational fields.

### src/App.tsx

`planningYear` changes trigger a re-fetch of the current city. Update cache key to include year: `${slug}:${year}`. On city switch, use cached entry for `${newSlug}:${planningYear}` if it exists.

---

## Migration checklist

- [ ] Add `compute_comfort_score` to `scoring.rs`, with tests
- [ ] Add `typhoon_penalty` (private) to `scoring.rs`, with tests
- [ ] Add `get_worst_holiday_penalty` to `scoring.rs`, with tests including Dec→Jan wrap
- [ ] Add `compute_overall_score` to `scoring.rs`, with tests
- [ ] Add `compute_monthly_scores` to `scoring.rs`
- [ ] Add `MonthScore` struct to `models.rs`
- [ ] Add `monthly_scores: Vec<MonthScore>` to `CityData`
- [ ] Add `year` query param to `get_city` handler
- [ ] Add `current_year()` helper
- [ ] Attach `compute_monthly_scores` result to city response in handler
- [ ] Add `MonthScore` type to `types.ts`
- [ ] Add `monthly_scores` to `CityData` type
- [ ] Update `fetchCity` to accept and pass `year`
- [ ] Update cache key to `${slug}:${year}`
- [ ] Re-fetch city when `planningYear` changes
- [ ] Remove `computeComfortScore`, `computeOverallScore`, `getWorstHolidayPenalty`, `typhoonRiskToScore`, `typhoonPenalty` from `scoring.ts`
- [ ] Update `Heatmap.tsx` scores useMemo to read from `monthly_scores`
- [ ] Add isolated `computeOverallFromComponents` helper for crowd substitution
- [ ] Update `MonthDetail.tsx` to read from `monthly_scores`
- [ ] Verify: changing planning year re-fetches and scores update
- [ ] Verify: changing year-range filter still updates crowd and overall
- [ ] Run `cargo test` — all scoring tests pass
- [ ] Run `tsc` — no type errors
