# Holiday Schema Migration: Floating Dates

**Scope:** Replace `holidays.csv` flat table with `holidays.csv` (reference) + `occurrences.csv` (concrete dates per year). Add `planningYear` selector to frontend. Motivation: lunisolar holidays (CNY, Tet), annually varying events (Fireworks Festival), and Easter shift month across years — the old `typical_month_start/end` was a lossy approximation that gets worse as the city list grows.

---

## New CSV schema

### `holidays.csv` — reference table

Replaces the old file entirely. No date fields.

```
id,city_slug,name,crowd_impact,price_impact,closure_impact,notes
```

| Field          | Type   | Notes                                                        |
|----------------|--------|--------------------------------------------------------------|
| `id`           | String | Slug. Globally unique. Format: `{event}-{city}`, e.g. `cny-hk` |
| `city_slug`    | String | Matches the city slug derived from Weather.csv               |
| `name`         | String | Display name                                                 |
| `crowd_impact` | String | `extreme` / `very_high` / `high` / `moderate` / `low` / `none` |
| `price_impact` | String | `high` / `moderate` / `low` / `none`                        |
| `closure_impact` | String | `significant` / `minimal` / `none`                        |
| `notes`        | String | Free text                                                    |

No `typical_period` or `duration` fields. Duration was already silently skipped by the API parser.

### `occurrences.csv` — concrete dates

New file. One or more rows per holiday per year.

```
holiday_id,year,date_start,date_end
```

| Field        | Type   | Notes                                                                                             |
|--------------|--------|---------------------------------------------------------------------------------------------------|
| `holiday_id` | String | FK to `holidays.csv.id`                                                                           |
| `year`       | i32    | Gregorian year of the event start. `year=2025` for Christmas even though it ends Jan 2 2026.      |
| `date_start` | String | ISO 8601: `YYYY-MM-DD`                                                                            |
| `date_end`   | String | ISO 8601: `YYYY-MM-DD`. Equal to `date_start` for single-day events. Can be in `year+1` for Dec→Jan events (e.g. Christmas 2025: `2026-01-02`) |

`month_start` and `month_end` are **not stored in the CSV** — they are derived by the loader from `date_start` and `date_end` respectively and stored in the struct. This eliminates redundancy and prevents month/date disagreement.

**Multiple occurrences per year are valid.** Some lunisolar events repeat on sub-annual cycles within a single Gregorian year. The canonical example is Bali's Galungan festival, which follows the 210-day Pawukon calendar and therefore falls **twice** in most Gregorian years. Add two rows with the same `(holiday_id, year)` and different `date_start`/`date_end` values. The loader preserves both; the API returns both under the same `occurrences` array, sorted by `date_start`.

```csv
# Two Galungan occurrences in 2025:
galungan-bali,2025,2025-04-02,2025-04-02
galungan-bali,2025,2025-10-29,2025-10-29
```

Frontend scoring must use `filter` (not `find`) when matching occurrences to a year, so both occurrences are checked for month overlap.

**Update cadence:** Once per year, add rows for the coming year. Target: update each October for the following year. For fixed-date holidays (Golden Week, Reunification Day) rows can be pre-filled years in advance.

---

## Backend changes (offpeak-api, Rust)

### New structs — `src/data/models.rs`

```rust
#[derive(Debug, Clone, Serialize)]
pub struct Holiday {
    pub id: String,
    pub name: String,
    pub crowd_impact: String,
    pub price_impact: String,
    pub closure_impact: String,
    pub notes: String,
    pub occurrences: Vec<HolidayOccurrence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HolidayOccurrence {
    pub year: i32,
    pub date_start: String,  // "YYYY-MM-DD"
    pub date_end: String,    // "YYYY-MM-DD"; may be year+1 for Dec→Jan events
    pub month_start: u8,     // derived from date_start at load time
    pub month_end: u8,       // derived from date_end at load time
}
```

Removed fields from `Holiday`: `typical_month_start`, `typical_month_end`.
Added: `id`, `occurrences`.

`month_start`/`month_end` are not in the CSV — derived by parsing the ISO date strings in the loader.

### New CSV loaders — `src/data/mod.rs`

Two loaders replace the old single holidays loader.

**`load_holidays(path)`** — loads `holidays.csv` into `HashMap<String, HolidayRef>` keyed by `id`. `HolidayRef` is an intermediate struct (not serialized) that holds all fields except `occurrences`.

```rust
struct HolidayRef {
    id: String,
    city_slug: String,
    name: String,
    crowd_impact: String,
    price_impact: String,
    closure_impact: String,
    notes: String,
}
```

Parsed by column index (consistent with existing approach):

| Index | Field           |
|-------|-----------------|
| 0     | `id`            |
| 1     | `city_slug`     |
| 2     | `name`          |
| 3     | `crowd_impact`  |
| 4     | `price_impact`  |
| 5     | `closure_impact`|
| 6     | `notes`         |

**`load_occurrences(path)`** — loads `occurrences.csv` into `Vec<OccurrenceRow>`. Parses ISO date strings to extract month numbers.

```rust
struct OccurrenceRow {
    holiday_id: String,
    year: i32,
    date_start: String,
    date_end: String,
    month_start: u8,  // parsed from date_start: "2025-01-29" → 1
    month_end: u8,    // parsed from date_end:   "2025-02-04" → 2
}
```

Column index:

| Index | Field          |
|-------|----------------|
| 0     | `holiday_id`   |
| 1     | `year`         |
| 2     | `date_start`   |
| 3     | `date_end`     |

Month extraction — split on `-`, take index 1, parse as u8:

```rust
fn month_from_iso(date: &str) -> u8 {
    date.split('-')
        .nth(1)
        .and_then(|m| m.parse().ok())
        .unwrap_or(0)
}
```

Returns 0 on malformed input; 0 is filtered out by the `(1..=12).contains` guard in scoring, consistent with existing `month_str_to_num` behavior.

**`build_cities` join logic:**

```rust
// 1. Load both files
let holiday_refs = load_holidays(holidays_path)?;
let occurrences = load_occurrences(occurrences_path)?;

// 2. Group occurrences by holiday_id.
//    Multiple rows with the same (holiday_id, year) are intentional.
let mut occ_by_id: HashMap<String, Vec<HolidayOccurrence>> = HashMap::new();
for row in &occurrences {
    occ_by_id.entry(row.holiday_id.clone())
        .or_default()
        .push(HolidayOccurrence {
            year: row.year,
            date_start: row.date_start.clone(),
            date_end: row.date_end.clone(),
            month_start: row.month_start,
            month_end: row.month_end,
        });
}

// 3. Assemble Holiday structs.
//    Sort occurrences by (year, date_start) so multi-occurrence years are
//    in chronological order (e.g. Galungan April then Galungan October).
let mut holidays_by_city: HashMap<String, Vec<Holiday>> = HashMap::new();
for (id, href) in &holiday_refs {
    let mut occs = occ_by_id.remove(id).unwrap_or_default();
    occs.sort_by(|a, b| a.year.cmp(&b.year).then(a.date_start.cmp(&b.date_start)));
    let holiday = Holiday {
        id: id.clone(),
        name: href.name.clone(),
        // ... other fields
        occurrences: occs,
    };
    holidays_by_city
        .entry(href.city_slug.clone())
        .or_default()
        .push(holiday);
}
```

### API response shape

`GET /api/v1/cities/{slug}` — `holidays` array changes shape:

```json
"holidays": [
  {
    "id": "cny-hk",
    "name": "Chinese New Year",
    "crowd_impact": "extreme",
    "price_impact": "high",
    "closure_impact": "significant",
    "notes": "Worst time for budget travel. Great for atmosphere.",
    "occurrences": [
      { "year": 2024, "date_start": "2024-02-10", "date_end": "2024-02-17", "month_start": 2, "month_end": 2 },
      { "year": 2025, "date_start": "2025-01-29", "date_end": "2025-02-04", "month_start": 1, "month_end": 2 },
      { "year": 2026, "date_start": "2026-02-17", "date_end": "2026-02-23", "month_start": 2, "month_end": 2 }
    ]
  }
]
```

No breaking change to other fields. The `typical_month_start` / `typical_month_end` fields are removed — frontend must be updated before deploying.

### Optional: year query param

`GET /api/v1/cities/{slug}?year=2026` — filter each holiday's `occurrences` array to the requested year only. Reduces payload for frontend that only needs one year. Not required for v1 of this migration; frontend can filter client-side.

### Startup validation

Add at load time:
- Warn (println) for any `holiday_id` in `occurrences.csv` that has no matching entry in `holidays.csv`.
- Warn for any holiday in `holidays.csv` that has zero occurrences (data gap, not a crash).
- Do not panic on either — partial data is better than no data.

---

## Frontend changes (offpeak-web, TypeScript/React)

### Updated types — `src/types.ts`

```typescript
export interface HolidayOccurrence {
  year: number;
  date_start: string;   // "YYYY-MM-DD"
  date_end: string;     // "YYYY-MM-DD"
  month_start: number;  // derived by API, used for month filtering
  month_end: number;
}

export interface Holiday {
  id: string;
  name: string;
  crowd_impact: 'extreme' | 'very_high' | 'high' | 'moderate' | 'low' | 'none';
  price_impact: 'high' | 'moderate' | 'low' | 'none';
  closure_impact: 'significant' | 'minimal' | 'none';
  notes: string;
  occurrences: HolidayOccurrence[];
}
```

Removed: `typical_month_start`, `typical_month_end`.

### New state — `src/App.tsx`

```typescript
const [planningYear, setPlanningYear] = useState<number>(new Date().getFullYear());
```

Default: current calendar year. When switching cities, `planningYear` is NOT reset — it's a global planning context, not per-city.

Available years for the selector: derived from the union of all occurrence years across the loaded cities. Or simpler: hardcode range `[currentYear - 1, currentYear, currentYear + 1]` and disable years with no occurrence data.

Pass `planningYear` down to `Heatmap` and `MonthDetail`.

### New component — `src/components/PlanningYearSelector.tsx`

Same pattern as `YearRangeSelector`. Single-select (not multi). Renders one button per available year.

```typescript
interface Props {
  years: number[];
  selected: number;
  onSelect: (year: number) => void;
}
```

Place in header alongside `YearRangeSelector`. Label: "Planning" or "Visiting in".

### Updated scoring — `src/lib/scoring.ts`

`getHolidaysForMonth` gains a `year` parameter:

```typescript
export function getHolidaysForMonth(
  holidays: Holiday[],
  month: number,
  year: number,
): Holiday[] {
  // Use filter (not find) so holidays with multiple occurrences in the same
  // year (e.g. Galungan twice) are each checked independently.
  return holidays.filter(h =>
    h.occurrences
      .filter(o => o.year === year)
      .some(({ month_start: s, month_end: e }) => {
        if (s <= e) return month >= s && month <= e;
        return month >= s || month <= e; // Dec→Jan wrap
      })
  );
}
```

All call sites pass `planningYear`. The `getWorstHolidayPenalty` and `computeOverallScore` functions are unchanged.

### Heatmap — `src/components/Heatmap.tsx`

Add `planningYear: number` to props. Pass through to `getHolidaysForMonth` in the `scores` useMemo and in the holiday row rendering.

### MonthDetail — `src/components/MonthDetail.tsx`

Add `planningYear: number` to props. In the Holidays section, find the occurrence for `planningYear` and display `date_start – date_end` as a formatted date range alongside the holiday name: "Jan 29 – Feb 4". Show the year in the section header: "Holidays in 2026".

If a holiday exists in the reference table but has no occurrence for `planningYear`, omit it from the list.

---

## Migration checklist

### Backend (offpeak-api) — done
- [x] Add `data/holidays_v2.csv` (reference table, new schema)
- [x] Add `data/occurrences.csv` (concrete dates, multi-occurrence per year supported)
- [x] Update Rust structs: `Holiday` (id + occurrences), add `HolidayOccurrence`
- [x] Write `parse_occurrences` loader
- [x] Rewrite `parse_holidays` loader (new column layout, keyed by id)
- [x] Update `build_cities` join logic (ref + occurrences → per-city Holiday vecs)
- [x] Remove `typical_month_start/end` from `Holiday` struct
- [x] Startup validation: warn on orphaned occurrences and holidays with zero occurrences
- [x] Sort occurrences by `(year, date_start)` — handles Galungan-style double occurrences

### Frontend (offpeak-web) — pending
- [ ] Update `src/types.ts`: `Holiday`, add `HolidayOccurrence`
- [ ] Add `planningYear` state to `App.tsx`
- [ ] Add `PlanningYearSelector` component
- [ ] Update `getHolidaysForMonth`: use `filter` not `find` for year match (multi-occurrence support)
- [ ] Update all `getHolidaysForMonth` call sites to pass `planningYear`
- [ ] Update `Heatmap` props and useMemo
- [ ] Update `MonthDetail` props and Holidays section
- [ ] Verify `christmas-hk` Dec→Jan wrap renders correctly for both months

---

## Data maintenance going forward

Each October, add next year's rows to `occurrences.csv` for all floating holidays. Fixed-date holidays (Golden Week, Reunification Day, National Day) can be pre-filled in bulk. The `notes` field in `occurrences.csv` is the place to record anchor dates ("Feb 10") for audit purposes without affecting logic.

For the Fireworks Festival: check Da Nang tourism board announcement (typically released Feb-Mar for the coming summer season). Until confirmed, use `6,7` as placeholder and note "Verify exact dates" in the `notes` field.
