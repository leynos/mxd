# Dockerfile for running the TLC model checker
# Uses TLA+ tools pinned at v1.7.4 for reproducibility
#
# Note: The Alpine base image is not pinned to a specific digest; this accepts
# minor drift in exchange for security patches. The critical reproducibility
# constraint is the tla2tools.jar version, which is pinned and checksum-verified.

FROM docker.io/eclipse-temurin:21-jre-alpine

# Create non-root user for security
RUN adduser -D -u 1000 tlc

# Download tla2tools.jar pinned at v1.7.4 with SHA-256 verification
# Update both version and hash when upgrading
ARG TLA_VERSION=1.7.4
ARG TLA_SHA256=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88

RUN apk add --no-cache curl && \
    curl -fsSL -o /opt/tla2tools.jar \
      "https://github.com/tlaplus/tlaplus/releases/download/v${TLA_VERSION}/tla2tools.jar" && \
    echo "${TLA_SHA256}  /opt/tla2tools.jar" | sha256sum -c - && \
    apk del curl

WORKDIR /workspace

USER tlc

ENTRYPOINT ["java", "-XX:+UseParallelGC", "-cp", "/opt/tla2tools.jar", "tlc2.TLC"]
