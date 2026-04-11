FROM rust:1.85-alpine AS builder
RUN apk add musl-dev
WORKDIR /app
COPY . .
RUN cargo build --release

FROM alpine:3.19
COPY --from=builder /app/target/release/offpeak-api /usr/local/bin/offpeak-api
COPY data/ /app/data/
WORKDIR /app
EXPOSE 3000
CMD ["offpeak-api"]
