FROM rust:1.95-bookworm AS chef
WORKDIR /app
RUN cargo install cargo-chef --locked

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --bin ozon-cloud-api --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin ozon-cloud-api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/ozon-cloud-api /usr/local/bin/ozon-cloud-api
ENV OZON_SUITE_BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["ozon-cloud-api"]

