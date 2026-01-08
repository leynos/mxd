# Dockerfile for running TLC model checker
# Uses the latest TLA+ tools from GitHub releases

FROM docker.io/eclipse-temurin:21-jre-alpine

# Download the latest tla2tools.jar
RUN apk add --no-cache curl && \
    curl -fsSL -o /opt/tla2tools.jar \
      https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar && \
    apk del curl

WORKDIR /workspace

ENTRYPOINT ["java", "-XX:+UseParallelGC", "-cp", "/opt/tla2tools.jar", "tlc2.TLC"]
