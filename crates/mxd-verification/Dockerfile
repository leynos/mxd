# Dockerfile for running the TLC model checker
# Uses TLA+ tools pinned at v1.8.0 for reproducibility

FROM docker.io/eclipse-temurin:21-jre-alpine

# Download tla2tools.jar pinned at v1.8.0 (update version here when upgrading)
RUN apk add --no-cache curl && \
    curl -fsSL -o /opt/tla2tools.jar \
      https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar && \
    apk del curl

WORKDIR /workspace

ENTRYPOINT ["java", "-XX:+UseParallelGC", "-cp", "/opt/tla2tools.jar", "tlc2.TLC"]
