package personalwebsite

import (
	"archive/tar"
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/bazelbuild/rules_go/go/runfiles"
)

func TestCacheHeaders(t *testing.T) {
	client, files, root := startCaddy(t)
	landingHTML, ok := files["/index.html"]
	if !ok {
		t.Fatal("deployment archive has no landing page")
	}
	t.Run("landing page revalidation", func(t *testing.T) {
		checkRevalidation(t, client, "/", landingHTML)
	})
	t.Run("same size and timestamp with changed HTML", func(t *testing.T) {
		changed := strings.Replace(landingHTML, "<title>", "<TITLE>", 1)
		if changed == landingHTML || len(changed) != len(landingHTML) {
			t.Fatal("expected a same-length HTML change")
		}
		path := filepath.Join(root, "index.html")
		info, err := os.Stat(path)
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(changed), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.Chtimes(path, info.ModTime(), info.ModTime()); err != nil {
			t.Fatal(err)
		}
		// Simulate a new deployment's content and its generated sidecar together.
		if err := os.WriteFile(path+".etag", []byte(contentETag(changed)), 0o644); err != nil {
			t.Fatal(err)
		}
		response, body := get(t, client, "/", http.Header{
			"If-None-Match":     {contentETag(landingHTML)},
			"If-Modified-Since": {"Sat, 01 Jan 2000 00:00:00 GMT"},
		})
		if response.StatusCode != http.StatusOK || body != changed {
			t.Fatalf("changed landing response = %d; want 200 with new HTML", response.StatusCode)
		}
		assertHeader(t, response, "Etag", contentETag(changed))
		checkRevalidation(t, client, "/", changed)
	})
	links := regexp.MustCompile(`(?:src|href)="(/_astro/[^"\s]+)"`).FindAllStringSubmatch(landingHTML, -1)
	var stylesheet string
	for _, link := range links {
		path := link[1]
		if strings.HasSuffix(path, ".css") {
			stylesheet = path
		}
		t.Run(path, func(t *testing.T) {
			response, body := get(t, client, path, nil)
			if response.StatusCode != http.StatusOK || body != files[path] {
				t.Fatalf("asset response = %d; want 200 with built contents", response.StatusCode)
			}
			const immutable = "public, max-age=31536000, immutable"
			assertHeader(t, response, "Cache-Control", immutable)
			etag := response.Header.Get("Etag")
			if etag == "" {
				t.Fatal("asset must retain its validator")
			}
			response, body = get(t, client, path, http.Header{"If-None-Match": {etag}})
			if response.StatusCode != http.StatusNotModified || body != "" {
				t.Fatalf("validated asset = %d; want 304 without a body", response.StatusCode)
			}
			assertHeader(t, response, "Cache-Control", immutable)
		})
	}
	if stylesheet == "" {
		t.Fatal("landing page has no generated stylesheet to check")
	}
	t.Run("unversioned favicon revalidation", func(t *testing.T) {
		checkRevalidation(t, client, "/favicon.svg", files["/favicon.svg"])
	})
	for _, path := range []string{"/_astro/missing.css", "/missing", "/missing.html", "/index.html.etag"} {
		t.Run(path, func(t *testing.T) {
			response, _ := get(t, client, path, nil)
			if response.StatusCode != http.StatusNotFound {
				t.Fatalf("missing resource status = %d; want 404", response.StatusCode)
			}
			assertHeader(t, response, "Cache-Control", "no-store")
		})
	}
	for _, path := range []string{stylesheet, "/favicon.svg"} {
		for _, tc := range []struct {
			name    string
			headers http.Header
			status  int
		}{
			{"unsatisfiable range", http.Header{"Range": {"bytes=9223372036854775807-"}}, http.StatusRequestedRangeNotSatisfiable},
			{"failed precondition", http.Header{"If-Match": {"\"not-current\""}}, http.StatusPreconditionFailed},
		} {
			t.Run(path+"/"+tc.name, func(t *testing.T) {
				response, _ := get(t, client, path, tc.headers)
				if response.StatusCode != tc.status {
					t.Fatalf("status = %d; want %d", response.StatusCode, tc.status)
				}
				assertHeader(t, response, "Cache-Control", "no-store")
			})
		}
	}
	t.Run("existing redirect", func(t *testing.T) {
		response, _ := get(t, client, "/projects", nil)
		if response.StatusCode != http.StatusMovedPermanently {
			t.Fatalf("redirect status = %d; want 301", response.StatusCode)
		}
		assertHeader(t, response, "Location", "/blog")
	})
}

func contentETag(body string) string {
	return fmt.Sprintf("\"%x\"", sha256.Sum256([]byte(body)))
}

func checkRevalidation(t *testing.T, client *http.Client, path, contents string) {
	t.Helper()
	etag := contentETag(contents)
	for _, tc := range []struct {
		name    string
		headers http.Header
		status  int
		body    string
	}{
		{"initial", nil, http.StatusOK, contents},
		{"legacy timestamp", http.Header{"If-Modified-Since": {"Sat, 01 Jan 2000 00:00:00 GMT"}}, http.StatusOK, contents},
		{"stale etag", http.Header{"If-None-Match": {"\"old-etag\""}}, http.StatusOK, contents},
		{"current etag", http.Header{"If-None-Match": {etag}}, http.StatusNotModified, ""},
	} {
		t.Run(tc.name, func(t *testing.T) {
			response, body := get(t, client, path, tc.headers)
			if response.StatusCode != tc.status || body != tc.body {
				t.Fatalf("response = %d; want %d with expected body", response.StatusCode, tc.status)
			}
			assertHeader(t, response, "Cache-Control", "no-cache")
			assertHeader(t, response, "Etag", etag)
			assertHeader(t, response, "Last-Modified", "")
		})
	}
}

func get(t *testing.T, client *http.Client, path string, headers http.Header) (*http.Response, string) {
	t.Helper()
	request, err := http.NewRequestWithContext(t.Context(), http.MethodGet, "http://localhost"+path, nil)
	if err != nil {
		t.Fatal(err)
	}
	request.Header = headers
	response, err := client.Do(request)
	if err != nil {
		t.Fatal(err)
	}
	defer response.Body.Close()
	body, err := io.ReadAll(response.Body)
	if err != nil {
		t.Fatal(err)
	}
	return response, string(body)
}

func assertHeader(t *testing.T, response *http.Response, name, want string) {
	t.Helper()
	if got := response.Header.Get(name); got != want {
		t.Errorf("%s = %q; want %q", name, got, want)
	}
}

func startCaddy(t *testing.T) (*http.Client, map[string]string, string) {
	t.Helper()
	caddy, err := runfiles.Rlocation(os.Getenv("CADDY"))
	if err != nil {
		t.Fatal(err)
	}
	caddyfile, err := runfiles.Rlocation(os.Getenv("CADDYFILE"))
	if err != nil {
		t.Fatal(err)
	}
	workDir := t.TempDir()
	root := filepath.Join(workDir, "app")
	archive, err := runfiles.Rlocation(os.Getenv("SITE_ARCHIVE"))
	if err != nil {
		t.Fatal(err)
	}
	file, err := os.Open(archive)
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	files := make(map[string]string)
	reader := tar.NewReader(file)
	for {
		header, err := reader.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		if header.Typeflag == tar.TypeDir {
			continue
		}
		if header.Typeflag != tar.TypeReg {
			t.Fatalf("unsupported site archive entry: %s", header.Name)
		}
		path, ok := strings.CutPrefix(strings.TrimPrefix(header.Name, "./"), "app/")
		if !ok || !filepath.IsLocal(path) {
			t.Fatalf("unexpected site archive path: %s", header.Name)
		}
		body, err := io.ReadAll(reader)
		if err != nil {
			t.Fatal(err)
		}
		name := filepath.Join(root, path)
		if err := os.MkdirAll(filepath.Dir(name), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(name, body, 0o644); err != nil {
			t.Fatal(err)
		}
		// Preserve the actual deployment timestamps, including pkg_tar normalization.
		if err := os.Chtimes(name, header.ModTime, header.ModTime); err != nil {
			t.Fatal(err)
		}
		files["/"+filepath.ToSlash(path)] = string(body)
	}
	source, err := os.ReadFile(caddyfile)
	if err != nil {
		t.Fatal(err)
	}
	adapt := exec.CommandContext(t.Context(), caddy, "adapt", "--adapter", "caddyfile", "--config", "-")
	adapt.Stdin = strings.NewReader(strings.ReplaceAll(string(source), "root * /app", "root * "+root))
	var diagnostics bytes.Buffer
	adapt.Stderr = &diagnostics
	adapted, err := adapt.Output()
	if err != nil {
		t.Fatalf("adapt: %v\n%s", err, diagnostics.String())
	}
	var config map[string]any
	if err := json.Unmarshal(adapted, &config); err != nil {
		t.Fatal(err)
	}
	// Use a short socket path: Bazel's test directories can exceed Unix socket limits.
	socketDir, err := os.MkdirTemp("/tmp", "caddy-cache-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(socketDir) })
	socket := filepath.Join(socketDir, "http.sock")
	servers := config["apps"].(map[string]any)["http"].(map[string]any)["servers"].(map[string]any)
	for _, server := range servers {
		server.(map[string]any)["listen"] = []string{"unix/" + socket}
	}
	config["admin"] = map[string]any{"disabled": true}
	encoded, err := json.Marshal(config)
	if err != nil {
		t.Fatal(err)
	}
	configPath := filepath.Join(workDir, "caddy.json")
	if err := os.WriteFile(configPath, encoded, 0o600); err != nil {
		t.Fatal(err)
	}
	log, err := os.Create(filepath.Join(workDir, "caddy.log"))
	if err != nil {
		t.Fatal(err)
	}
	command := exec.CommandContext(t.Context(), caddy, "run", "--config", configPath)
	command.Env = append(os.Environ(), "XDG_CONFIG_HOME="+workDir, "XDG_DATA_HOME="+workDir)
	command.Stdout, command.Stderr = log, log
	if err := command.Start(); err != nil {
		log.Close()
		t.Fatal(err)
	}
	t.Cleanup(func() {
		command.Process.Kill()
		command.Wait()
		log.Close()
		if t.Failed() {
			contents, _ := os.ReadFile(log.Name())
			t.Logf("Caddy output:\n%s", contents)
		}
	})
	transport := &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", socket)
	}}
	t.Cleanup(transport.CloseIdleConnections)
	client := &http.Client{
		Transport: transport, Timeout: 2 * time.Second,
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}
	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		request, err := http.NewRequestWithContext(t.Context(), http.MethodGet, "http://localhost/", nil)
		if err != nil {
			t.Fatal(err)
		}
		response, err := client.Do(request)
		if err == nil {
			response.Body.Close()
			return client, files, root
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatal("Caddy did not become ready")
	return nil, nil, ""
}
