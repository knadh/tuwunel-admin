FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY . .
RUN cargo build --release

FROM alpine:3.21
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/tuwunel-admin /usr/local/bin/tuwunel-admin
WORKDIR /app
EXPOSE 8009
ENTRYPOINT ["tuwunel-admin"]