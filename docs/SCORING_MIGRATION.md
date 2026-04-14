# Scoring Migration: Backend — Completed

**Scope:** All score computation moved from frontend (`scoring.ts`) to backend (`scoring.rs`). Frontend is a pure display layer: receives pre-computed scores from the API, maps values to colors, renders cells. No scoring logic on the client.

---

## What changed on the backend

### `GET /api/v1/cities`

Previously returned an array of slug strings. Now returns:

```json
[
  { "slug": "hong-kong", "name": "Hong Kong" },
  { "slug": "da-nang",   "name": "Da Nang"   }
]
```

Frontend must not derive display names from slugs. Use `name` directly.

---

### `GET /api/v1/cities/{slug}`

Three optional query parameters:

| Param       | Type        | Default      | Description                                              |
|-------------|-------------|--------------|----------------------------------------------------------|
| `year`      | i32         | current year | Planning year — selects which holiday occurrences to use |
| `year_from` | i32 or null | all years    | Start of arrivals range for crowd_index computation      |
| `year_to`   | i32 or null | all years    | End of arrivals range for crowd_index computation        |

`year` and `year_from`/`year_to` are fully independent:
- `year` only affects `holiday_penalty` and therefore `overall`
- `year_from`/`year_to` only affect `crowd_index` and therefore `overall`

Example:
```
GET /api/v1/cities/hong-kong?year=2027&year_from=2023&year_to=2024
```

Response includes a new top-level field `monthly_scores`:

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
    },
    ...
  ]
}
```

`monthly_scores` always has exactly 12 entries, sorted month 1–12. All component values are exposed so the frontend can show breakdowns without any computation.

Results are cached server-side by `(slug, year, year_from, year_to)` — evicted only on restart.

---

## Score field reference

| Field             | Type  | Range        | Notes                          |
|-------------------|-------|--------------|--------------------------------|
| `comfort`         | i32   | 2–10         | From heat_index + rain_days    |
| `crowd_index`     | f64   | 1.0–10.0     | 1 decimal; higher = more crowd |
| `typhoon_penalty` | f64   | 0 / 0.5 / 2 / 6 | Informational; already in overall |
| `holiday_penalty` | i32   | 0–3          | Informational; already in overall |
| `overall`         | f64   | 1.0–10.0     | 1 decimal; higher = better     |

---

## What frontend must do

### `src/types.ts`

Add:

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

Update city list type:

```typescript
export interface CityListItem {
  slug: string;
  name: string;
}
```

### `src/api.ts`

`fetchCities` returns `CityListItem[]`, not `string[]`.

`fetchCity` passes all three params:

```typescript
export function fetchCity(
  slug: string,
  year: number,
  yearFrom?: number,
  yearTo?: number,
): Promise<CityData> {
  const params = new URLSearchParams({ year: String(year) });
  if (yearFrom !== undefined) params.set('year_from', String(yearFrom));
  if (yearTo   !== undefined) params.set('year_to',   String(yearTo));
  return fetch(`${API_URL}/api/v1/cities/${slug}?${params}`)
    .then(r => { if (!r.ok) throw new Error(r.statusText); return r.json(); });
}
```

Re-fetch when `year`, `yearFrom`, or `yearTo` changes. Cache key on the frontend: `${slug}:${year}:${yearFrom ?? ''}:${yearTo ?? ''}`.

### `src/lib/scoring.ts` — remove entirely

The following functions no longer exist and must be deleted:
- `computeComfortScore`
- `computeOverallScore`
- `computeOverallFromComponents`
- `getWorstHolidayPenalty`
- `typhoonRiskToScore`
- `typhoonPenalty`

`computeMonthlyIndex` and `getHolidaysForMonth` are also no longer needed — the backend handles year-range filtering and holiday resolution.

### `src/components/Heatmap.tsx`

`scores` useMemo reads directly from `city.monthly_scores`:

```typescript
const scores = city.monthly_scores; // no computation
```

If a year-range selector exists, it now drives a re-fetch rather than a client-side recomputation.

### `src/components/MonthDetail.tsx`

Read score fields directly from the matching `MonthScore` entry in `city.monthly_scores`. No local computation.

### `src/App.tsx`

- City list: iterate `CityListItem[]`, use `.name` for display, `.slug` for routing
- `planningYear` changes → re-fetch current city with new `year` param
- Year-range changes → re-fetch current city with new `year_from`/`year_to` params
- No scoring logic anywhere in the component tree
