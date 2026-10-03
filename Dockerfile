FROM ubuntu:24.04@sha256:a853f94d226358a79c740cfc7bce0c289748f3fe3488d921d038ccd752c61b60

# Install dependencies
RUN apt update && apt install -y ca-certificates curl
RUN curl -fsSL https://get.docker.com -o get-docker.sh
RUN sh ./get-docker.sh

RUN apt update && apt install -y \
    curl \
    gnupg \
    lsb-release \
    build-essential \
    python3 \
    python3-pip \
    git

