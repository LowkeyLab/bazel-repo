FROM ubuntu:24.04@sha256:d78ab76437b1afc5f01e223d6bf0172763f404bb166441328845adbef44518cb

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

