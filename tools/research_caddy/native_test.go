package research_caddy

import (
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
	"syscall"
	"testing"
	"time"

	"github.com/bazelbuild/rules_go/go/runfiles"
)

func TestPackagedNative(t *testing.T) {
	caddy, err := runfiles.Rlocation(os.Getenv("CADDY"))
	if err != nil {
		t.Fatal(err)
	}
	source, err := runfiles.Rlocation(os.Getenv("CADDYFILE"))
	if err != nil {
		t.Fatal(err)
	}
	out := os.Getenv("TEST_UNDECLARED_OUTPUTS_DIR")
	save := func(name string, value []byte) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(out, name), value, 0o600); err != nil {
			t.Fatal(err)
		}
	}
	for _, command := range []string{"version", "build-info", "list-modules"} {
		data, err := exec.CommandContext(t.Context(), caddy, command).CombinedOutput()
		save(command+".txt", data)
		if err != nil {
			t.Fatal(err)
		}
	}
	binary, err := os.ReadFile(caddy)
	if err != nil {
		t.Fatal(err)
	}
	original, err := os.ReadFile(source)
	if err != nil {
		t.Fatal(err)
	}
	save("input-hashes.json", []byte(fmt.Sprintf("{\"caddy_sha256\":\"%x\",\"source_sha256\":\"%x\"}\n", sha256.Sum256(binary), sha256.Sum256(original))))
	adapt := exec.CommandContext(t.Context(), caddy, "adapt", "--adapter", "caddyfile", "--config", source, "--pretty")
	var diagnostics bytes.Buffer
	adapt.Stderr = &diagnostics
	encoded, err := adapt.Output()
	save("native-adapt.stderr", diagnostics.Bytes())
	if err != nil {
		t.Fatal(err)
	}
	save("native-adapt.json", encoded)
	mappingBytes := buildMapping(t, caddy, original, encoded, out)
	var config map[string]any
	if err := json.Unmarshal(encoded, &config); err != nil {
		t.Fatal(err)
	}
	response, log := runRequest(t, caddy, config, out, "native")
	save("native-runtime.jsonl", log)
	data, err := json.MarshalIndent(response, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	save("native-response.json", data)
	if response.Status != 307 || response.Location != "/done" || response.Stage != "before" || response.Order != "first" {
		t.Fatalf("Unexpected response: %+v", response)
	}
	save("functional.txt", []byte("Packaged native request assertions passed.\n"))
	servers := config["apps"].(map[string]any)["http"].(map[string]any)["servers"].(map[string]any)
	for _, server := range servers {
		server.(map[string]any)["logs"].(map[string]any)["trace"] = true
	}
	traced, traceLog := runRequest(t, caddy, config, out, "trace")
	save("trace-runtime.jsonl", traceLog)
	if traced != response {
		t.Fatalf("Trace changes functional response: native=%+v trace=%+v", response, traced)
	}
	save("trace-functional.txt", []byte("Traced response equals native response.\n"))
	receipt, err := json.MarshalIndent(map[string]any{"complete": true, "log_sha256": fmt.Sprintf("%x", sha256.Sum256(traceLog)), "source_sha256": fmt.Sprintf("%x", sha256.Sum256(original))}, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	save("trace-completion.json", receipt)
	checkCollection(t, out, original, mappingBytes, traceLog)
}

type result struct {
	Status                       int
	Location, Stage, Order, Body string
}

func runRequest(t *testing.T, caddy string, config map[string]any, out, name string) (result, []byte) {
	t.Helper()
	socketDir, err := os.MkdirTemp("/tmp", "caddy-coverage-")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(socketDir)
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
	configPath := filepath.Join(out, name+"-run.json")
	if err := os.WriteFile(configPath, encoded, 0o600); err != nil {
		t.Fatal(err)
	}
	logPath := filepath.Join(out, name+"-runtime.log")
	logFile, err := os.Create(logPath)
	if err != nil {
		t.Fatal(err)
	}
	command := exec.CommandContext(t.Context(), caddy, "run", "--config", configPath)
	command.Env = append(os.Environ(), "XDG_CONFIG_HOME="+out, "XDG_DATA_HOME="+out, "GOMAXPROCS=2", "GOMEMLIMIT=1536MiB")
	command.Stdout = logFile
	command.Stderr = logFile
	if err := command.Start(); err != nil {
		t.Fatal(err)
	}
	stopped := false
	stop := func() {
		if stopped {
			return
		}
		stopped = true
		_ = command.Process.Signal(syscall.SIGINT)
		_ = command.Wait()
		_ = logFile.Close()
	}
	defer stop()
	deadline := time.Now().Add(10 * time.Second)
	for {
		if _, err := os.Stat(socket); err == nil {
			break
		}
		if time.Now().After(deadline) {
			stop()
			body, _ := os.ReadFile(logPath)
			t.Fatalf("Caddy readiness failed: %s", body)
		}
		time.Sleep(10 * time.Millisecond)
	}
	transport := &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "unix", socket)
	}}
	defer transport.CloseIdleConnections()
	client := &http.Client{Transport: transport, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }, Timeout: 5 * time.Second}
	request, err := http.NewRequestWithContext(t.Context(), http.MethodGet, "http://fixture/chosen", nil)
	if err != nil {
		t.Fatal(err)
	}
	request.Header.Set("X-Mode", "yes")
	response, err := client.Do(request)
	if err != nil {
		t.Fatal(err)
	}
	body, err := io.ReadAll(response.Body)
	_ = response.Body.Close()
	if err != nil {
		t.Fatal(err)
	}
	actual := result{response.StatusCode, response.Header.Get("Location"), response.Header.Get("X-Stage"), response.Header.Get("X-Order"), string(body)}
	stop()
	log, err := os.ReadFile(logPath)
	if err != nil {
		t.Fatal(err)
	}
	return actual, log
}
