FROM ubuntu:24.04@sha256:ab18ecc70c55c703eae968948f9f1dbbe662aa8d9badac7c982e6a6237b9d74c

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

