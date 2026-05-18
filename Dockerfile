
## Этап сборки
FROM rust:1.94-slim AS builder
LABEL authors="artemos"

### Установка необходимых инструментов и зависимостей
RUN --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/turn-proxy/target \
    cargo build --release && \
    cp target/release/turn-proxy-server /usr/local/bin/turn-proxy-server

COPY . .

### Этап финального образа
WORKDIR /usr/src/turn-proxy
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/turn-proxy/target \
    cargo build --release && \
    cp target/release/turn-proxy-server /usr/local/bin/turn-proxy-server


## Финальный образ
FROM gcr.io/distroless/cc-debian12:nonroot
LABEL authors="artemos"

COPY --from=builder /usr/local/bin/turn-proxy-server /usr/local/bin/turn-proxy-server

EXPOSE 56040/udp

## Здесь можно указать другой путь к конфигурационному файлу, если нужно
## (в, например, Docker-compose нужно задать volume для этого файла)
ENTRYPOINT ["turn-proxy-server"]
CMD ["--config", "/config.toml"]