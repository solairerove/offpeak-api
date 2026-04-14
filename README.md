# offpeak-api

JSON API for the Offpeak travel planning tool. Reads city data from CSV files at startup and serves it in memory.

**Live:** https://offpeak-api-production.up.railway.app

## Endpoints

```
GET /api/v1/cities
GET /api/v1/cities/{slug}?planning_year=2026&years=2018,2019,2024
GET /api/v1/cities/{slug}/weather
GET /api/v1/cities/{slug}/arrivals?planning_year=2026&years=2018,2019,2024
```

## Run locally

```bash
cargo run
```

Expects CSV files in `data/`. Override with `DATA_DIR` env var. Port defaults to `3000`, override with `PORT`.

## Build & run with Docker

```bash
docker build -t offpeak-api .
docker run -p 3000:3000 offpeak-api
```

## Deploy

Deployed on Railway via `Dockerfile`. Set `PORT` automatically by Railway.
