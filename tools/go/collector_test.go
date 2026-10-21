package go_test

import (
	"flag"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/bazelbuild/rules_go/go/tools/bzltestutil"
)

func TestCollectorChild(t *testing.T) {
	if os.Getenv("GO_COLLECTOR_CHILD") != "1" {
		return
	}
	if os.Getenv("GO_COLLECTOR_KIND") == "file" {
		if err := flag.Set("test.coverprofile", filepath.Join(os.Getenv("COVERAGE_DIR"), "missing.profile")); err != nil {
			t.Fatal(err)
		}
		if err := bzltestutil.ConvertCoverToLcov(); err != nil {
			t.Fatal(err)
		}
		return
	}
	if err := bzltestutil.ConvertCoverFromReaderToLcov(strings.NewReader(os.Getenv("GO_COLLECTOR_INPUT"))); err != nil {
		t.Fatal(err)
	}
}

func TestCollectorValidatesRequiredOutput(t *testing.T) {
	cases := []struct {
		name, input, kind string
		fails             bool
	}{
		{name: "empty", fails: true},
		{name: "invalid_mode", input: "mode: unknown\n", fails: true},
		{name: "corrupt", input: "mode: set\ninvalid\n", fails: true},
		{name: "truncated", input: "mode: set\n" + strings.Repeat("x", 70000), fails: true},
		{name: "missing", kind: "file", fails: true},
		{name: "no_executable_lines", input: "mode: set\n"},
		{name: "measured_lines", input: "mode: set\nfixture.go:3.1,5.2 1 7\n"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			directory := t.TempDir()
			child := exec.Command(os.Args[0], "-test.run=^TestCollectorChild$")
			for _, entry := range os.Environ() {
				if !strings.HasPrefix(entry, "COVERAGE_OUTPUT_FILE=") && !strings.HasPrefix(entry, "COVERAGE_DIR=") {
					child.Env = append(child.Env, entry)
				}
			}
			child.Env = append(child.Env, "GO_COLLECTOR_CHILD=1", "GO_COLLECTOR_INPUT="+tc.input, "GO_COLLECTOR_KIND="+tc.kind, "COVERAGE_DIR="+directory)
			output, err := child.CombinedOutput()
			if (err != nil) != tc.fails {
				t.Fatalf("collector error = %v, expected failure = %v: %s", err, tc.fails, output)
			}
			if tc.name == "measured_lines" {
				files, err := filepath.Glob(filepath.Join(directory, "go_coverage.*.dat"))
				if err != nil || len(files) != 1 {
					t.Fatalf("collector reports = %v: %v", files, err)
				}
				report, err := os.ReadFile(files[0])
				if err != nil {
					t.Fatal(err)
				}
				for _, record := range []string{"SF:fixture.go\n", "DA:3,7\n", "DA:4,7\n", "DA:5,7\n"} {
					if !strings.Contains(string(report), record) {
						t.Fatalf("missing %q in %s", record, report)
					}
				}
			}
		})
	}
}
