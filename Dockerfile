## Этап подготовОЧКИ
FROM rust:1.94-slim AS planner
WORKDIR /usr/src/turn-proxy

RUN cargo install cargo-chef --locked
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

## Этап сборки зависимостей
FROM rust:1.94-slim AS builder
WORKDIR /usr/src/turn-proxy
RUN cargo install cargo-chef --locked

COPY --from=planner /usr/src/turn-proxy/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json

## Этап сборки приложения
COPY . .
RUN cargo build --release

## Этап сборки образа
FROM gcr.io/distroless/cc-debian13:nonroot
LABEL authors="artemos"

COPY --from=builder /usr/src/turn-proxy/target/release/turn-proxy-server /bin/turn-proxy-server

EXPOSE 56040/udp

## Здесь можно указать другой путь к конфигурационному файлу, если нужно
## (в, например, Docker-compose нужно задать volume для этого файла)
ENTRYPOINT ["turn-proxy-server"]
CMD ["--config", "/config.toml"]
