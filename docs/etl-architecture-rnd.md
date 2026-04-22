# ETL Architecture R&D

> Research & design document. Reference for future technical implementation docs.

---

## Current Architecture Summary

The API is a single stateless Rust service. All data lives in 6 CSV files **baked into the Docker image** at build time. A data update means editing CSVs → Docker rebuild → Railway redeploy. No database, no external data sources.

**Core problem**: data updates are coupled to deployments.

---

## What Can Actually Be Automated

| Data | Automatable? | Reason |
|---|---|---|
| `weather.csv` — climate normals | **~70%** | Temperature, humidity, rainfall, rain days → Open-Meteo free API. `typhoon_risk` and `notes` stay manual (editorial judgment). |
| `occurrences.csv` — holiday dates | **~80%** | CNY, Tet, public holidays have APIs or algorithmic derivation. Impact metadata in `holidays.csv` stays manual. |
| `arrivals.csv` — visitor counts | **No** | Tourism board data (HKTB, Vietnam MoT) is published as PDFs/Excel months late. Scraping is fragile. Manual import is right. |
| `pricing.csv` — cost index | **No** | This is editorial judgment. Could scrape Booking.com but it's legally grey and noisy. |
| `notes.csv` — tips/visa | **No** | Pure editorial content. |

---

## Proposed Architecture

Three layers, cleanly separated:

```
┌─────────────────────────────────────────────────────────────┐
│  DATA SOURCES                                               │
│                                                             │
│  Open-Meteo API          Calendarific / nager.Date API      │
│  (free, no key)          (free tier for public holidays)    │
│                                                             │
│  GSheets/CSV exports                                        │
│  (arrivals, pricing, notes)                                 │
└──────────┬───────────────────────┬──────────────────────────┘
           │                       │
           ▼                       ▼
┌─────────────────────────────────────────────────────────────┐
│  ETL LAYER  (separate services, same Railway project)       │
│                                                             │
│  weather-etl          → runs monthly via cron               │
│  holiday-etl          → runs annually (prefetch 2 yrs)      │
│  csv-importer         → manual trigger (CLI or webhook)     │
│                         for arrivals / pricing / notes      │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  PERSISTENCE LAYER                                          │
│                                                             │
│  PostgreSQL (Railway managed)                               │
│  Tables: cities, weather, arrivals, holidays,               │
│          occurrences, pricing, notes                        │
│  (maps 1:1 with current CSV schemas)                       │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  API LAYER  (current Rust service, mostly unchanged)        │
│                                                             │
│  - Replace CSV loading with DB queries (sqlx)               │
│  - Add POST /api/v1/admin/reload  (env-var API key)         │
│  - Keep scoring.rs untouched                                │
│  - Keep handlers.rs untouched                               │
│  - Remove data/ from Docker image                           │
└─────────────────────────────────────────────────────────────┘
```

---

## ETL Services Detail

**`weather-etl`** (Python or Rust CLI)
- Input: cities config (slug → lat/lng coordinates)
- Calls Open-Meteo climate normals endpoint — free, no key
- Returns: monthly avg temp, humidity, rainfall, rain days
- Derives: heat index from temp + humidity (standard formula)
- Writes to `weather` DB table, skips `typhoon_risk` and `notes` (marked as manual columns)
- Schedule: monthly cron, but practically annual is enough (climate normals don't change)

**`holiday-etl`** (Python or Rust CLI)
- Input: `holidays` table (already has impact metadata)
- Calls Calendarific or nager.Date for public holiday dates per country
- For lunisolar events (CNY, Tet): algorithmic calculation (established libraries exist for both)
- Writes to `occurrences` table for current year + next 2 years
- Schedule: annual cron in December

**`csv-importer`** (Rust CLI, extend current parsing code)
- Accepts a CSV file path + `--type arrivals|pricing|notes` flag
- Validates and upserts to DB
- Triggered manually: `./csv-importer --type arrivals --file arrivals_2024.csv`

---

## What Changes in the API

Minimal changes — the architecture is already well-suited:

1. **`src/data/mod.rs`**: swap CSV file reads for `sqlx` DB queries. `build_cities()` logic stays the same — it just receives data from DB rows instead of parsed CSV rows.
2. **`Cargo.toml`**: add `sqlx` with postgres + runtime-tokio features.
3. **`src/main.rs`**: add `DATABASE_URL` env var, initialize DB pool, pass to `load_app_data()`.
4. **`Dockerfile`**: remove `COPY data/ /app/data/` — no more baked-in data.
5. **New route**: `POST /api/v1/admin/reload` — re-runs `load_app_data()` and swaps the `Arc<AppData>`. Needed to refresh in-memory state after ETL writes without a full restart.

The scoring logic, handlers, models, and all business logic are **completely untouched**.

---

## Migration Path

**Phase 1** (foundation, ~1 day): Add Railway PostgreSQL, write a one-shot migration script that imports current CSVs into DB, update API to read from DB. Deploy. Verify data parity.

**Phase 2** (reload, ~half day): Add `POST /admin/reload` endpoint so ETL can notify the API to hot-reload after writes. Remove data from Docker image.

**Phase 3** (weather ETL, ~1 day): Write `weather-etl` with Open-Meteo, run it, verify output in DB. Set up Railway cron.

**Phase 4** (holiday ETL, ~1 day): Write `holiday-etl` for occurrence dates. Run it and verify against current `occurrences.csv` for known dates. Set up annual cron.

---

## What Stays Manual (and That's Fine)

- `holidays.csv` impact ratings — `crowd_impact`, `price_impact`, `closure_impact` are editorial judgments
- `weather` → `typhoon_risk` and `notes` — regional knowledge
- `arrivals` — tourism board data, downloaded quarterly
- `pricing` — editorial cost index
- `notes` — tips and visa info

The win is that the **automated data never needs to be touched again** once the ETL is wired up, and **manual data no longer requires a Docker rebuild** — just trigger the importer.

---

## Simplest Alternative (if PostgreSQL feels heavy)

If you want to stay file-based: store CSVs in **Cloudflare R2** (free tier, S3-compatible), API fetches from R2 at startup, ETL writes updated files to R2 and calls the reload endpoint. No DB, very low ops overhead. Trades some query flexibility for simplicity.

---

## Recommendation

Go with PostgreSQL on Railway — it's one click to provision, sqlx is excellent in Rust, and it gives you clean separation between automated and manually-curated columns, proper upsert semantics, and easy auditability (add `updated_at`, `source` columns). The current architecture maps to it almost perfectly. The ETL layer and scoring logic don't need to know about each other at all.
