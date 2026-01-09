# Dockerfile for running the TLC model checker
# Uses TLA+ tools pinned at v1.7.4 for reproducibility

FROM docker.io/eclipse-temurin:21-jre-alpine

# Create non-root user for security
RUN adduser -D -u 1000 tlc

# Download tla2tools.jar pinned at v1.7.4 (update version here when upgrading)
RUN apk add --no-cache curl && \
    curl -fsSL -o /opt/tla2tools.jar \
      https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar && \
    apk del curl

WORKDIR /workspace

USER tlc

ENTRYPOINT ["java", "-XX:+UseParallelGC", "-cp", "/opt/tla2tools.jar", "tlc2.TLC"]
