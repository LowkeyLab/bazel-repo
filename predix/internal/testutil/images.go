package testutil

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"
	"sync"

	"github.com/bazelbuild/rules_go/go/runfiles"
	"github.com/testcontainers/testcontainers-go"
)

var imageSetup struct {
	sync.Once
	postgres string
	err      error
}

// prepareImages imports declared archives before any containers are started.
func prepareImages(ctx context.Context) (string, error) {
	imageSetup.Do(func() {
		imageSetup.postgres, imageSetup.err = importImages(ctx)
	})
	return imageSetup.postgres, imageSetup.err
}

func importImages(ctx context.Context) (string, error) {
	ryuk, err := imageReference("RYUK")
	if err != nil {
		return "", err
	}
	prefix, _, ok := strings.Cut(ryuk, "/testcontainers/ryuk:")
	if !ok {
		return "", fmt.Errorf("invalid Bazel Ryuk image reference %q", ryuk)
	}
	// Set this before creating the client: Testcontainers caches its config.
	// The prefix selects our digest-specific Ryuk archive and preserves cleanup.
	if err := os.Setenv("TESTCONTAINERS_HUB_IMAGE_NAME_PREFIX", prefix); err != nil {
		return "", err
	}
	client, err := testcontainers.NewDockerClientWithOpts(ctx)
	if err != nil {
		return "", fmt.Errorf("connect to test Docker daemon: %w", err)
	}
	defer client.Close()
	for _, image := range []string{"RYUK", "POSTGRES"} {
		if err := loadImage(ctx, client, image); err != nil {
			return "", err
		}
	}
	return imageReference("POSTGRES")
}

func imagePath(prefix, suffix string) (string, error) {
	key := prefix + "_IMAGE_" + suffix
	value := os.Getenv(key)
	if value == "" {
		return "", fmt.Errorf("missing Bazel test input %s", key)
	}
	path, err := runfiles.Rlocation(value)
	if err != nil {
		return "", fmt.Errorf("resolve %s: %w", key, err)
	}
	return path, nil
}

func imageReference(prefix string) (string, error) {
	path, err := imagePath(prefix, "REF")
	if err != nil {
		return "", err
	}
	ref, err := os.ReadFile(path)
	if err != nil {
		return "", fmt.Errorf("read %s image reference: %w", prefix, err)
	}
	return strings.TrimSpace(string(ref)), nil
}

func loadImage(ctx context.Context, client *testcontainers.DockerClient, prefix string) error {
	path, err := imagePath(prefix, "TAR")
	if err != nil {
		return err
	}
	archive, err := os.Open(path)
	if err != nil {
		return fmt.Errorf("open %s image archive: %w", prefix, err)
	}
	defer archive.Close()
	progress, err := client.ImageLoad(ctx, archive)
	if err != nil {
		return fmt.Errorf("import Bazel %s image: %w", prefix, err)
	}
	defer progress.Close()
	// Docker reports import failures in the JSON stream even with HTTP 200.
	decoder := json.NewDecoder(progress)
	for {
		var message struct {
			Error string `json:"error"`
		}
		if err := decoder.Decode(&message); err == io.EOF {
			return nil
		} else if err != nil {
			return fmt.Errorf("read %s image import progress: %w", prefix, err)
		}
		if message.Error != "" {
			return fmt.Errorf("import Bazel %s image: %s", prefix, message.Error)
		}
	}
}
