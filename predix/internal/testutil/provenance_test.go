package testutil

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"os"
	"os/exec"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/bazelbuild/rules_go/go/runfiles"
	"github.com/testcontainers/testcontainers-go"
)

// Run the public fixture in a fresh process: both Docker configuration and image
// initialization are cached globally by the fixture and Testcontainers.
func TestFixtureProcess(t *testing.T) {
	if os.Getenv("BAZEL_IMAGE_FIXTURE_CHILD") != "1" {
		t.Skip("subprocess entry point")
	}
	db := SetupTestDB(t)
	var result int
	if err := db.QueryRowContext(t.Context(), "SELECT 1").Scan(&result); err != nil || result != 1 {
		t.Fatalf("fixture database query = %d, %v", result, err)
	}
}

func TestFixturesUseDeclaredArchives(t *testing.T) {
	client, err := testcontainers.NewDockerClientWithOpts(t.Context())
	if err != nil {
		t.Fatal(err)
	}
	defer client.Close()
	// These tests run in the same Docker-enabled Bazel environment as the other
	// database tests. Forward to its Unix socket, never to an independent daemon.
	socket, ok := strings.CutPrefix(client.DaemonHost(), "unix://")
	if !ok {
		t.Fatalf("provenance test requires a Unix Docker socket, got %q", client.DaemonHost())
	}
	transport := &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", socket)
	}}
	defer transport.CloseIdleConnections()
	probe, err := runfiles.Rlocation(os.Getenv("RUST_POSTGRES_PROBE"))
	if err != nil {
		t.Fatal(err)
	}
	for _, language := range []string{"Go", "Rust"} {
		t.Run(language, func(t *testing.T) {
			prefixes := []string{"POSTGRES", "RYUK"}
			if language == "Rust" {
				prefixes = []string{"RUST_POSTGRES"}
			}
			archives := make(map[[32]byte]string)
			for _, prefix := range prefixes {
				path, err := imagePath(prefix, "TAR")
				if err != nil {
					t.Fatal(err)
				}
				archive, err := os.Open(path)
				if err != nil {
					t.Fatal(err)
				}
				hash := sha256.New()
				_, err = io.Copy(hash, archive)
				archive.Close()
				if err != nil {
					t.Fatal(err)
				}
				ref, err := imageReference(prefix)
				if err != nil {
					t.Fatal(err)
				}
				archives[[32]byte(hash.Sum(nil))] = ref
			}
			var mu sync.Mutex
			imported, created := map[string]bool{}, map[string]bool{}
			upstream, _ := url.Parse("http://docker")
			proxy := httputil.NewSingleHostReverseProxy(upstream)
			proxy.Transport = transport
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				switch {
				case strings.HasSuffix(r.URL.Path, "/images/create"):
					t.Errorf("fixture attempted a registry pull: %s", r.URL.RawQuery)
					http.Error(w, `{"message":"registry pulls disabled"}`, http.StatusForbidden)
					return
				case strings.HasSuffix(r.URL.Path, "/images/load"):
					// Hash the actual uploaded bytes, so a warm daemon cache cannot make a
					// missing or substituted Bazel input appear to work.
					archive, err := os.CreateTemp(t.TempDir(), "upload-")
					if err != nil {
						t.Error(err)
						http.Error(w, err.Error(), 500)
						return
					}
					defer archive.Close()
					hash := sha256.New()
					if _, err := io.Copy(io.MultiWriter(archive, hash), r.Body); err != nil {
						t.Error(err)
						http.Error(w, err.Error(), 500)
						return
					}
					ref, declared := archives[[32]byte(hash.Sum(nil))]
					if !declared {
						t.Error("fixture imported an undeclared archive")
						http.Error(w, "undeclared archive", 403)
						return
					}
					if _, err := archive.Seek(0, io.SeekStart); err != nil {
						t.Error(err)
						http.Error(w, err.Error(), 500)
						return
					}
					request := r.Clone(r.Context())
					request.URL.Scheme, request.URL.Host = "http", "docker"
					request.RequestURI = ""
					request.Body = io.NopCloser(archive)
					response, err := transport.RoundTrip(request)
					if err != nil {
						t.Error(err)
						http.Error(w, err.Error(), 502)
						return
					}
					defer response.Body.Close()
					body, err := io.ReadAll(response.Body)
					if err != nil {
						t.Error(err)
						http.Error(w, err.Error(), 502)
						return
					}
					if response.StatusCode == http.StatusOK && !bytes.Contains(body, []byte(`"error"`)) && !bytes.Contains(body, []byte(`"errorDetail"`)) {
						mu.Lock()
						imported[ref] = true
						mu.Unlock()
					}
					w.Header().Set("Content-Type", "application/json")
					w.WriteHeader(response.StatusCode)
					_, _ = w.Write(body)
					return
				case strings.HasSuffix(r.URL.Path, "/containers/create"):
					body, err := io.ReadAll(r.Body)
					if err != nil {
						t.Error(err)
						http.Error(w, err.Error(), 400)
						return
					}
					r.Body = io.NopCloser(bytes.NewReader(body))
					var config struct{ Image string }
					if err := json.Unmarshal(body, &config); err != nil {
						t.Error(err)
						http.Error(w, err.Error(), 400)
						return
					}
					mu.Lock()
					allowed := imported[config.Image]
					if allowed {
						created[config.Image] = true
					}
					mu.Unlock()
					if !allowed {
						t.Errorf("container requested image %q before its declared archive was imported", config.Image)
						http.Error(w, `{"message":"image not imported from declared input"}`, 403)
						return
					}
				}
				proxy.ServeHTTP(w, r)
			}))
			defer server.Close()
			ctx, cancel := context.WithTimeout(t.Context(), 2*time.Minute)
			defer cancel()
			command := exec.CommandContext(ctx, os.Args[0], "-test.run=^TestFixtureProcess$", "-test.v")
			if language == "Rust" {
				command = exec.CommandContext(ctx, probe)
			}
			command.Env = append(os.Environ(),
				"DOCKER_HOST=tcp://"+strings.TrimPrefix(server.URL, "http://"),
				"DOCKER_TLS_VERIFY=",
				"DOCKER_CERT_PATH=",
				"TESTCONTAINERS_HOST_OVERRIDE=127.0.0.1",
				"TESTCONTAINERS_DOCKER_SOCKET_OVERRIDE="+socket,
				"TESTCONTAINERS_RYUK_DISABLED=false",
				"BAZEL_IMAGE_FIXTURE_CHILD=1")
			if language == "Rust" {
				command.Env = append(command.Env,
					"POSTGRES_IMAGE_REF="+os.Getenv("RUST_POSTGRES_IMAGE_REF"),
					"POSTGRES_IMAGE_TAR="+os.Getenv("RUST_POSTGRES_IMAGE_TAR"))
			}
			if output, err := command.CombinedOutput(); err != nil {
				t.Fatalf("%s fixture: %v\n%s", language, err, output)
			}
			mu.Lock()
			defer mu.Unlock()
			for _, ref := range archives {
				if !created[ref] {
					t.Errorf("no container created from declared image %q", ref)
				}
			}
		})
	}
}
