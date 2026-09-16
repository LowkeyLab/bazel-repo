package testutil

import (
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"github.com/testcontainers/testcontainers-go"
)

func TestLoadImage(t *testing.T) {
	var mu sync.Mutex
	var response string
	var status int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case strings.HasSuffix(r.URL.Path, "/_ping"):
			w.Header().Set("API-Version", "1.49")
			_, _ = io.WriteString(w, "OK")
		case strings.HasSuffix(r.URL.Path, "/info"):
			w.Header().Set("Content-Type", "application/json")
			_, _ = io.WriteString(w, `{"ID":"fixture-daemon","ServerVersion":"28.1.0"}`)
		case strings.HasSuffix(r.URL.Path, "/images/load"):
			if r.Method != http.MethodPost {
				t.Errorf("import method = %s", r.Method)
			}
			body, err := io.ReadAll(r.Body)
			if err != nil || string(body) != "declared archive" {
				t.Errorf("unexpected archive: %q, %v", body, err)
			}
			w.Header().Set("Content-Type", "application/json")
			mu.Lock()
			defer mu.Unlock()
			w.WriteHeader(status)
			_, _ = io.WriteString(w, response)
		default:
			t.Errorf("unexpected Docker request: %s %s", r.Method, r.URL.Path)
			http.Error(w, "unexpected request", http.StatusNotFound)
		}
	}))
	defer server.Close()
	t.Setenv("DOCKER_HOST", server.URL)
	t.Setenv("DOCKER_TLS_VERIFY", "")
	t.Setenv("DOCKER_CERT_PATH", "")
	client, err := testcontainers.NewDockerClientWithOpts(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	defer client.Close()

	archive := filepath.Join(t.TempDir(), "image.tar")
	if err := os.WriteFile(archive, []byte("declared archive"), 0o600); err != nil {
		t.Fatal(err)
	}
	cases := []struct {
		name      string
		response  string
		status    int
		path      string
		wantError string
	}{
		{"success", "{\"stream\":\"Loading\"}\n{\"stream\":\"Loaded\"}\n", 200, archive, ""},
		{"daemon error after progress", "{\"stream\":\"Loading\"}\n{\"error\":\"no space left on device\"}\n", 200, archive, "no space left on device"},
		{"malformed response", "not json", 200, archive, "image import progress"},
		{"truncated response", "{\"stream\":", 200, archive, "unexpected EOF"},
		{"HTTP error", `{ "message": "daemon unavailable" }`, 500, archive, "daemon unavailable"},
		{"missing input", "", 200, "", "missing Bazel test input"},
		{"missing archive", "", 200, archive + ".missing", "open POSTGRES image archive"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			mu.Lock()
			response, status = tc.response, tc.status
			mu.Unlock()
			t.Setenv("POSTGRES_IMAGE_TAR", tc.path)
			err := loadImage(t.Context(), client, "POSTGRES")
			if tc.wantError == "" {
				if err != nil {
					t.Fatal(err)
				}
			} else if err == nil || !strings.Contains(err.Error(), tc.wantError) {
				t.Fatalf("loadImage error = %v; want %q", err, tc.wantError)
			}
		})
	}
}

func TestImageReference(t *testing.T) {
	path := filepath.Join(t.TempDir(), "image.ref")
	if err := os.WriteFile(path, []byte("fixture:sha256-pinned\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("POSTGRES_IMAGE_REF", path)
	got, err := imageReference("POSTGRES")
	if err != nil || got != "fixture:sha256-pinned" {
		t.Fatalf("reference = %q, %v", got, err)
	}
}
