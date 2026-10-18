#!/usr/bin/env bash
set -euo pipefail
# shellcheck disable=SC1091
source "${RUNFILES_DIR:-$0.runfiles}/bazel_tools/tools/bash/runfiles/runfiles.bash"
image_tar="$(rlocation "$WORKER_IMAGE_TAR")"
image_ref="$(cat "$(rlocation "$WORKER_IMAGE_REF")")"
postgres_tar="$(rlocation "$POSTGRES_IMAGE_TAR")"
postgres_ref="$(rlocation "$POSTGRES_IMAGE_REF")"
docker load --input "$image_tar"
docker run --rm --privileged --network none \
	--mount "type=bind,src=$postgres_tar,dst=/fixtures/postgres.tar,readonly" \
	--mount "type=bind,src=$postgres_ref,dst=/fixtures/postgres.ref.txt,readonly" \
	"$image_ref"
