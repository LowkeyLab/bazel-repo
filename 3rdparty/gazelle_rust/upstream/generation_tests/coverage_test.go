package generationtest

import (
	"fmt"
	"os"
	"testing"
)

func TestMain(m *testing.M) {
	if os.Getenv("COVERAGE_DIR") != "" {
		// Go reads GOCOVERDIR before the subprocess's rules_go exit hook runs.
		// That hook collects LCOV; the runtime's additional files can stay in
		// Bazel's per-test scratch directory.
		if err := os.Setenv("GOCOVERDIR", os.Getenv("TEST_TMPDIR")); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
	}
	os.Exit(m.Run())
}
