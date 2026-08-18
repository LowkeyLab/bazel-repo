FROM ubuntu:24.04@sha256:1428a953896eef9e62fc6ef60cad05bbf98769f6ea5f8c278e519b9dd168ab26

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

