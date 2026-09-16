// Package testimages provides PostgreSQL containers using Bazel-supplied images.
package testimages

import (
	"context"
	"fmt"
	"time"

	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

// Postgres imports the declared PostgreSQL and Ryuk archives and starts a container.
// Callers must declare image_data and image_env for postgres_18 and ryuk in their
// Bazel test target and terminate the returned container when finished.
func Postgres(ctx context.Context, opts ...testcontainers.ContainerCustomizer) (*postgres.PostgresContainer, error) {
	image, err := prepareImages(ctx)
	if err != nil {
		return nil, fmt.Errorf("prepare Bazel test images: %w", err)
	}
	defaults := []testcontainers.ContainerCustomizer{
		testcontainers.WithWaitStrategy(
			wait.ForLog("database system is ready to accept connections").
				WithOccurrence(2).
				WithStartupTimeout(5 * time.Second),
		),
	}
	return postgres.Run(ctx, image, append(defaults, opts...)...)
}
