# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Development
cargo run                        # Start server (default: port 3000, data dir: ./data)
PORT=8080 DATA_DIR=./data cargo run  # Override defaults

# Build
cargo build                      # Debug build
cargo build --release            # Optimized release build

# Test
cargo test                       # Run all tests
cargo test scoring::tests        # Run only scoring module tests
cargo test -- --nocapture        # Run with stdout

# Lint & Format
cargo fmt                        # Format code
cargo fmt --check                # Check formatting without modifying
cargo clippy --all-targets       # Run linter

# Docker
docker build -t offpeak-api .
docker run -p 3000:3000 offpeak-api
```

## Architecture

The API is a stateless Axum/Tokio HTTP server. All CSV data is loaded into an in-memory `HashMap<String, CityData>` at startup (keyed by city slug) and served via an `Arc<AppData>` shared across async handlers. There is no database.

```
src/
├── main.rs          # Entry point: env config, data loading, router wiring, server startup
├── api/
│   ├── mod.rs       # Router definition, CORS layer (all origins/methods/headers)
│   └── handlers.rs  # HTTP handlers — extract State<Arc<AppData>>, return Json or 404
├── data/
│   ├── mod.rs       # CSV parsing: reads Weather.csv, Arrivals.csv, Holidays.csv, Notes.csv
│   └── models.rs    # AppData, CityData, WeatherMonth, ArrivalsData, Holiday, Note structs
└── scoring.rs       # Business logic: normalizes monthly arrival counts to a 1.0–10.0 index
```

**Data flow:** `main.rs` calls `data::load_app_data()` → parses 4 CSV files from `DATA_DIR` → builds `HashMap<slug, CityData>` → wraps in `Arc<AppData>` → passed as Axum state to all handlers. Handlers call `scoring::compute_monthly_index()` when serving the arrivals endpoint.

## API Routes

All routes under `/api/v1/`:

| Method | Path | Notes |
|--------|------|-------|
| GET | `/cities` | Sorted list of all city slugs |
| GET | `/cities/{slug}` | Full city data |
| GET | `/cities/{slug}/weather` | 12 `WeatherMonth` objects |
| GET | `/cities/{slug}/arrivals` | `ArrivalsData`; optional `?year_from=&year_to=` query params |

Health check for Railway: `GET /api/v1/cities`

## Configuration

Environment variables only — no config files or `.env`:
- `PORT` — HTTP listen port (default: `3000`)
- `DATA_DIR` — Path to directory containing CSV files (default: `"data"`)

Server panics on startup if CSV files are missing or port binding fails.

## Deployment

Deployed on Railway via multi-stage Docker build (`rust:1.85-alpine` builder → `alpine:3.19` runtime). Configuration in `railway.toml`. Railway injects `PORT` automatically.
