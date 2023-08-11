FROM rust:alpine3.18 AS builder
WORKDIR /app
COPY . .
RUN apk add --no-cache musl-dev 
RUN cargo build --release

FROM scratch
COPY --from=builder /app/target/release/task-queue .
CMD [ "./task-queue" ]
