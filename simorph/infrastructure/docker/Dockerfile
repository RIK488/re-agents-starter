# SIMORPH — image multi-étapes (API web + CLI)
FROM rust:1.83-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p simorph-web -p simorph-cli

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends python3 python3-numpy ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 10001 simorph
COPY --from=build /src/target/release/simorph-web /usr/local/bin/simorph-web
COPY --from=build /src/target/release/simorph-cli /usr/local/bin/simorph
COPY --from=build /src/adapters/python /opt/simorph/adapters/python
COPY --from=build /src/apps/web/static /opt/simorph/static
ENV SIMORPH_DATA=/data SIMORPH_STATIC=/opt/simorph/static SIMORPH_PYTHON_ADAPTER=/opt/simorph/adapters/python
VOLUME ["/data"]
USER simorph
EXPOSE 8080
ENTRYPOINT ["simorph-web"]
