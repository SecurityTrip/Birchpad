# The demo image.
FROM rust:1.98 AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:trixie-slim
ENV GREETING="Hello, world!"
COPY --from=build /src/target/release/demo /usr/local/bin/demo
EXPOSE 8080
CMD ["demo", "--port", "8080"]
