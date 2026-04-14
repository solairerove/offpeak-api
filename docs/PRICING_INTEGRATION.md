# Pricing Integration

**Scope:** Add historical hotel pricing as a score component. New `pricing.csv`, new `price_index` in monthly scores, new heatmap row on frontend. Pricing affects overall score and is visible as a standalone row.

---

## Why price_index, not raw ADR

Raw ADR (average daily rate) is currency-denominated and inflation-sensitive. $150/night in Hong Kong vs $80/night in Da Nang are not comparable. Normalizing to an index (100 = annual average for that city and year) makes values:
- Comparable across cities
- Inflation-invariant across years
- Interpretable: index 65 = 35% cheaper than average, index 155 = 55% more expensive

---

## CSV schema — `pricing.csv`

```
city,year,month,price_index,source,notes
```

| Field         | Type   | Notes                                                                    |
|---------------|--------|--------------------------------------------------------------------------|
| `city`        | String | Must match city name in Weather.csv exactly                              |
| `year`        | i32    | Year of observation                                                      |
| `month`       | String | "Jan"–"Dec", same format as Weather.csv and Arrivals.csv                |
| `price_index` | f64    | 100 = annual average for this city+year. Computed before entry.         |
| `source`      | String | e.g. `"HKTB ADR"`, `"STB RevPAR"`, `"manual"`. Informational only.     |
| `notes`       | String | Optional. e.g. `"CNY peak"`, `"estimated"`.                             |

One row per city/year/month. Multiple years per city are expected and used for averaging, same pattern as Arrivals.csv.

### How to compute price_index before entering data

Collect raw ADR (or RevPAR, or package price) for all 12 months of a year. Compute annual average. Divide each month by the average and multiply by 100.

```
raw:   Jan=180, Feb=210, Mar=160, ..., Dec=195
avg:   sum / 12
index: Jan = (180 / avg) * 100
```

This must be done per city per year before entering the CSV. The API does not recompute it — `price_index` is treated as a pre-normalized input.

---

## Backend changes (offpeak-api, Rust)

### New struct — `src/data/models.rs`

```rust
#[derive(Debug, Clone, Serialize)]
pub struct PricingEntry {
    pub year: i32,
    pub month: u8,
    pub price_index: f64,
}
```

`source` and `notes` are parsed but not stored — informational only, not serialized.

Add to `CityData`:

```rust
pub struct CityData {
    // existing fields...
    pub pricing: Vec<PricingEntry>,
}
```

### New loader — `src/data/mod.rs`

`load_pricing(path)` — same pattern as arrivals loader.

Column index:

| Index | Field         |
|-------|---------------|
| 0     | `city`        |
| 1     | `year`        |
| 2     | `month`       | ← parsed via `month_str_to_num`, same as elsewhere
| 3     | `price_index` |
| 4     | `source`      | ← parsed, discarded
| 5     | `notes`       | ← parsed, discarded

Rows with unparseable `price_index` are skipped with a `println!` warning, consistent with existing error handling pattern.

### Scoring — `src/scoring.rs`

#### `compute_price_index(pricing: &[PricingEntry], month: u8) -> Option<f64>`

Average `price_index` across all years for the given month. Returns `None` if no data exists — allows graceful handling of missing data per city.

```rust
pub fn compute_price_index(pricing: &[PricingEntry], month: u8) -> Option<f64> {
    let values: Vec<f64> = pricing.iter()
        .filter(|p| p.month == month)
        .map(|p| p.price_index)
        .collect();

    if values.is_empty() {
        return None;
    }

    Some(values.iter().sum::<f64>() / values.len() as f64)
}
```

#### `price_penalty(index: f64) -> f64`

Converts price index to a penalty value. Higher price = higher penalty. Nonlinear — extreme peaks (CNY, Golden Week) get disproportionate penalty.

```rust
pub fn price_penalty(index: f64) -> f64 {
    if index <= 70.0       { 0.0 }
    else if index <= 90.0  { 1.0 }
    else if index <= 110.0 { 2.0 }
    else if index <= 130.0 { 3.5 }
    else if index <= 160.0 { 5.5 }
    else                   { 8.0 }
}
```

Thresholds rationale:
- ≤70: significantly cheaper than average — no penalty
- 71–90: slightly below average — negligible
- 91–110: around average — neutral
- 111–130: noticeably expensive — meaningful penalty
- 131–160: peak season pricing — strong penalty
- >160: extreme peak (CNY, Golden Week) — near-maximum penalty

Calibrate after seeing real data distributions. The threshold at 110 being "neutral" is intentional — months slightly above average should not be punished.

#### Updated `MonthScore` struct

```rust
#[derive(Debug, Clone, Serialize)]
pub struct MonthScore {
    pub month: u8,
    pub comfort: i32,
    pub crowd_index: f64,
    pub typhoon_penalty: f64,
    pub holiday_penalty: i32,
    pub price_index: Option<f64>,   // None if no pricing data for this month
    pub price_penalty: Option<f64>, // None if no pricing data
    pub overall: f64,
}
```

`Option` because pricing data may not exist for all cities on day one. A city without pricing data continues to function — pricing component is excluded from the overall formula when `None`.

#### Updated `compute_overall_score`

Two formula branches based on whether pricing data is present:

**With pricing:**
```
overall = 0.30*comfort + 0.30*(11-crowd) + 0.15*(10-holidayPenalty) + 0.15*(10-typhoonPenalty) + 0.10*(10-pricePenalty)
```

**Without pricing:**
```
overall = 0.35*comfort + 0.35*(11-crowd) + 0.15*(10-holidayPenalty) + 0.15*(10-typhoonPenalty)
```

Weights sum to 1.0 in both cases. Comfort and crowd each give up 0.05 to fund the pricing component.

Updated signature:

```rust
pub fn compute_overall_score(
    comfort: i32,
    crowd: f64,
    holiday_penalty: i32,
    typhoon: &str,
    price_penalty: Option<f64>,
) -> f64 {
    let tp = typhoon_penalty(typhoon);
    let raw = match price_penalty {
        Some(pp) => 0.30 * comfort as f64
                  + 0.30 * (11.0 - crowd)
                  + 0.15 * (10.0 - holiday_penalty as f64)
                  + 0.15 * (10.0 - tp)
                  + 0.10 * (10.0 - pp),
        None =>     0.35 * comfort as f64
                  + 0.35 * (11.0 - crowd)
                  + 0.15 * (10.0 - holiday_penalty as f64)
                  + 0.15 * (10.0 - tp),
    };
    (raw.max(1.0).min(10.0) * 10.0).round() / 10.0
}
```

#### Updated `compute_monthly_scores`

```rust
pub fn compute_monthly_scores(city: &CityData, year: i32) -> Vec<MonthScore> {
    let monthly_index = compute_monthly_index(&city.arrivals.data);

    (1u8..=12).map(|month| {
        let weather = city.weather.iter().find(|w| w.month == month).unwrap();
        let crowd = monthly_index.iter()
            .find(|m| m.month == month)
            .map(|m| m.normalized)
            .unwrap_or(5.0);

        let comfort  = compute_comfort_score(weather.heat_index_c, weather.rain_days);
        let hp       = get_worst_holiday_penalty(&city.holidays, month, year);
        let pi       = compute_price_index(&city.pricing, month);
        let pp       = pi.map(price_penalty);
        let overall  = compute_overall_score(comfort, crowd, hp, &weather.typhoon_risk, pp);

        MonthScore {
            month,
            comfort,
            crowd_index: crowd,
            typhoon_penalty: typhoon_penalty(&weather.typhoon_risk),
            holiday_penalty: hp,
            price_index: pi,
            price_penalty: pp,
            overall,
        }
    }).collect()
}
```

### Tests

```rust
#[test]
fn price_penalty_thresholds() {
    assert_eq!(price_penalty(65.0),  0.0);
    assert_eq!(price_penalty(85.0),  1.0);
    assert_eq!(price_penalty(100.0), 2.0);
    assert_eq!(price_penalty(120.0), 3.5);
    assert_eq!(price_penalty(145.0), 5.5);
    assert_eq!(price_penalty(170.0), 8.0);
}

#[test]
fn price_index_averages_across_years() {
    let entries = vec![
        PricingEntry { year: 2023, month: 2, price_index: 160.0 },
        PricingEntry { year: 2024, month: 2, price_index: 170.0 },
    ];
    let result = compute_price_index(&entries, 2).unwrap();
    assert!((result - 165.0).abs() < 0.01);
}

#[test]
fn price_index_returns_none_for_missing_month() {
    let entries = vec![
        PricingEntry { year: 2024, month: 3, price_index: 110.0 },
    ];
    assert!(compute_price_index(&entries, 2).is_none());
}

#[test]
fn overall_without_pricing_uses_four_component_formula() {
    let score = compute_overall_score(8, 3.0, 0, "none", None);
    let expected = (0.35 * 8.0 + 0.35 * 8.0 + 0.15 * 10.0 + 0.15 * 10.0 * 10.0).round() / 10.0;
    assert!((score - expected).abs() < 0.05);
}

#[test]
fn overall_high_price_depresses_score() {
    let cheap     = compute_overall_score(7, 4.0, 0, "none", Some(0.0));
    let expensive = compute_overall_score(7, 4.0, 0, "none", Some(8.0));
    // 0.10 * (10-0) vs 0.10 * (10-8) → 0.8 difference
    assert!(expensive < cheap);
    assert!((cheap - expensive - 0.8).abs() < 0.05);
}
```

---

## Frontend changes (offpeak-web)

### Sample API response

`GET /api/v1/cities/hong-kong` now returns `monthly_scores` with two new nullable fields. Cities without pricing data return `null` for both; cities with data return computed values.

```json
{
  "city": "Hong Kong",
  "slug": "hong-kong",
  "pricing": [...],
  "monthly_scores": [
    {
      "month": 1,
      "comfort": 8,
      "crowd_index": 6.2,
      "typhoon_penalty": 0.0,
      "holiday_penalty": 0,
      "price_index": 110.0,
      "price_penalty": 2.0,
      "overall": 7.4
    },
    {
      "month": 2,
      "comfort": 8,
      "crowd_index": 7.1,
      "typhoon_penalty": 0.0,
      "holiday_penalty": 3,
      "price_index": 157.5,
      "price_penalty": 5.5,
      "overall": 5.8
    }
  ]
}
```

The `pricing` array (raw per-year entries) is present in the response but is not needed by the frontend — use `monthly_scores[].price_index` instead, which is already averaged across years.

### Realistic value range for price_index

Based on current data (HK + Da Nang, 2023–2024):

| Range    | Typical months                        |
|----------|---------------------------------------|
| 55–70    | Da Nang typhoon season (Sep–Oct)      |
| 70–90    | Off-season shoulder months            |
| 90–115   | Average months — the majority         |
| 115–135  | HK Golden Week / Christmas, Da Nang summer peak |
| 135–160+ | HK CNY (Feb), Da Nang Jul peak        |

For color scale calibration: treat 100 as neutral midpoint. Values below 80 are clearly cheap (green end), values above 130 are clearly expensive (red end).

### src/types.ts

Update `MonthScore`:

```typescript
export interface MonthScore {
  month: number;
  comfort: number;
  crowd_index: number;
  typhoon_penalty: number;
  holiday_penalty: number;
  price_index: number | null;
  price_penalty: number | null;
  overall: number;
}
```

### src/lib/scoring.ts

Update `computeOverallFromComponents` (the crowd-substitution helper that re-runs the formula after client-side crowd recalculation):

```typescript
function computeOverallFromComponents(
  comfort: number,
  crowd: number,
  holidayPenalty: number,
  typhoonPenalty: number,
  pricePenalty: number | null,
): number {
  const raw = pricePenalty !== null
    ? 0.30 * comfort + 0.30 * (11 - crowd) + 0.15 * (10 - holidayPenalty) + 0.15 * (10 - typhoonPenalty) + 0.10 * (10 - pricePenalty)
    : 0.35 * comfort + 0.35 * (11 - crowd) + 0.15 * (10 - holidayPenalty) + 0.15 * (10 - typhoonPenalty);
  return Math.round(Math.max(1, Math.min(10, raw)) * 10) / 10;
}
```

### src/components/Heatmap.tsx

Add `Price` row after the Typhoon row. Render the row only if at least one month in `monthly_scores` has non-null `price_index` — if no city has pricing data, the row is absent entirely rather than showing all `—`.

Cell value: `price_index` as integer (e.g. `"65"`, `"155"`). Color: `lowerIsBetter=true`. Null months: neutral gray, display `"—"`.

### src/components/MonthDetail.tsx

Add to Scores section:
- `price_index` not null: `"Price index: 65"` — add a static note on first render or in tooltip: `"100 = annual average for this city"`
- `price_index` null: `"Price: no data"`

---

## Data entry process

1. Find ADR or RevPAR for the city. Sources: HKTB Monthly Hotel Statistics (HK), STB Monthly Hotel Statistics (SG), tourism board annual reports.
2. Collect all 12 months for a year. Full-year data required to compute a valid index.
3. Compute annual average: `sum(all 12 months) / 12`.
4. Compute index per month: `(month_value / annual_avg) * 100`. Round to 1 decimal.
5. Sanity check: sum of all 12 indices should equal ~1200.
6. Enter into `pricing.csv`. Tag `source` and any anomalous months in `notes`.

Partial-year data (e.g. only 9 months available) should not be used to compute an index — the average will be skewed. Either find the missing months or skip that year entirely.

---

## Migration checklist

- [x] Create `data/pricing.csv` — enter HK and Da Nang data
- [x] Add `PricingEntry` struct to `models.rs`
- [x] Add `pricing: Vec<PricingEntry>` to `CityData`
- [x] Write `load_pricing` loader in `data/mod.rs`
- [x] Add `compute_price_index` to `scoring.rs` with tests
- [x] Add `price_penalty` to `scoring.rs` with tests
- [x] Update `MonthScore` struct: add `price_index`, `price_penalty` as `Option<f64>`
- [x] Update `compute_overall_score`: new signature, two formula branches
- [x] Update `compute_monthly_scores` to compute and pass pricing through
- [x] Add all pricing-related tests
- [ ] Update `MonthScore` type in `types.ts`
- [ ] Update `computeOverallFromComponents` in `scoring.ts`
- [ ] Add conditional Price row to `Heatmap.tsx`
- [ ] Update `MonthDetail.tsx` Scores section
- [ ] Enter real pricing data, re-check penalty thresholds against actual distributions
