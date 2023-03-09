FROM rust:1.67.1 AS planner
WORKDIR /usr/src/app
RUN cargo install cargo-chef
COPY ./ ./
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.67.1 AS cacher
WORKDIR /usr/src/app
RUN cargo install cargo-chef
COPY --from=planner /usr/src/app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rust:1.67.1 AS builder
WORKDIR /usr/src/app
COPY ./ ./
COPY --from=cacher /usr/src/app/target target
COPY --from=cacher $CARGO_HOME $CARGO_HOME
RUN echo 'SQLX_OFFLINE=true' >> .env
RUN cargo build --release

FROM debian:bullseye-20230227-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir /app
WORKDIR /app

COPY --from=builder \
    /usr/src/app/target/release/yadokari \
    ./

ENV PORT 8080

ENTRYPOINT ["./yadokari"]