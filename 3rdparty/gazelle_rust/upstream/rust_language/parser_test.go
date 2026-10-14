package rust_language

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/bazel-contrib/bazel-gazelle/v2/cmd/gazelle/update"
	"github.com/bazel-contrib/bazel-gazelle/v2/compat"
	languagev2 "github.com/bazel-contrib/bazel-gazelle/v2/language"
	"github.com/bazelbuild/bazel-gazelle/config"
	"github.com/bazelbuild/bazel-gazelle/language"

	pb "github.com/calsign/gazelle_rust/proto"
	"google.golang.org/protobuf/proto"
)

func realParser(t *testing.T) *Parser {
	t.Helper()
	p := NewParser()
	t.Cleanup(func() {
		_ = p.cleanup()
	})
	return p
}

func parserFixture(t *testing.T, dir, name, content string) string {
	t.Helper()
	path := filepath.Join(dir, name)
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestRealParserTypedExchanges(t *testing.T) {
	p := realParser(t)
	dir := t.TempDir()
	source := parserFixture(t, dir, "src/lib.rs", "use serde::Serialize;\npub fn library() {}\n")
	imports, err := p.Parse(&pb.RustImportsRequest{AbsolutePath: source, RelativePath: "src/lib.rs"})
	if err != nil {
		t.Fatal(err)
	}
	if !imports.Success || len(imports.Imports) != 1 || imports.Imports[0] != "serde" {
		t.Fatalf("source imports: %v", imports)
	}

	manifest := parserFixture(t, dir, "Cargo.toml", "[package]\nname = \"fixture-library\"\nversion = \"0.1.0\"\nedition = \"2024\"\n")
	cargo, err := p.ParseCargoToml(&pb.CargoTomlRequest{FilePath: manifest})
	if err != nil {
		t.Fatal(err)
	}
	if !cargo.Success || cargo.Name != "fixture-library" || cargo.Edition != "2024" || cargo.GetLibrary().GetName() != "fixture_library" || len(cargo.GetLibrary().GetSrcs()) != 1 || cargo.Library.Srcs[0] != "src/lib.rs" {
		t.Fatalf("manifest: %v", cargo)
	}

	lock := parserFixture(t, dir, "Cargo.lock", "version = 3\n\n[[package]]\nname = \"fixture-library\"\nversion = \"0.1.0\"\n")
	crates, err := p.GetLockfileCrates(&pb.LockfileCratesRequest{Lockfile: &pb.LockfileCratesRequest_CargoLockfilePath{CargoLockfilePath: lock}})
	if err != nil {
		t.Fatal(err)
	}
	want := &pb.LockfileCratesResponse{Crates: []*pb.Package{{Name: "fixture-library", CrateName: "fixture_library", Version: "0.1.0", WorkspaceMember: true}}}
	if !proto.Equal(crates, want) {
		t.Fatalf("lockfile: got %v, want %v", crates, want)
	}

	// A subsequent response of a different type must not alter earlier results.
	if !imports.Success || imports.Imports[0] != "serde" || cargo.Name != "fixture-library" {
		t.Fatalf("earlier results changed: source=%v manifest=%v", imports, cargo)
	}
}

func TestRealParserIndependentInstances(t *testing.T) {
	first, second := realParser(t), realParser(t)
	dir := t.TempDir()
	firstPath := parserFixture(t, dir, "first.rs", "use first_crate::Thing;\npub fn first() {}\n")
	secondPath := parserFixture(t, dir, "second.rs", "use second_crate::Thing;\nfn main() {}\n")
	firstRequest := &pb.RustImportsRequest{AbsolutePath: firstPath, RelativePath: "first.rs"}
	firstResult, err := first.Parse(firstRequest)
	if err != nil {
		t.Fatal(err)
	}
	secondResult, err := second.Parse(&pb.RustImportsRequest{AbsolutePath: secondPath, RelativePath: "second.rs"})
	if err != nil {
		t.Fatal(err)
	}
	if !firstResult.Success || len(firstResult.Imports) != 1 || firstResult.Imports[0] != "first_crate" || firstResult.GetHints().GetHasMain() {
		t.Fatalf("first parser result: %v", firstResult)
	}
	if !secondResult.Success || len(secondResult.Imports) != 1 || secondResult.Imports[0] != "second_crate" || !secondResult.GetHints().GetHasMain() {
		t.Fatalf("second parser result: %v", secondResult)
	}
	again, err := first.Parse(firstRequest)
	if err != nil {
		t.Fatal(err)
	}
	if !proto.Equal(again, firstResult) {
		t.Fatalf("first parser successive result: got %v, want %v", again, firstResult)
	}
}

func TestRealParserUnsuccessfulSourceResponseKeepsSessionUsable(t *testing.T) {
	p := realParser(t)
	dir := t.TempDir()
	bad := parserFixture(t, dir, "bad.rs", "pub fn broken(\n")
	response, err := p.Parse(&pb.RustImportsRequest{AbsolutePath: bad, RelativePath: "bad.rs"})
	if err != nil {
		t.Fatalf("ordinary parse diagnostic became transport error: %v", err)
	}
	if response.Success || response.ErrorMsg == "" {
		t.Fatalf("unsuccessful source response: %v", response)
	}
	good := parserFixture(t, dir, "good.rs", "fn main() {}\n")
	next, err := p.Parse(&pb.RustImportsRequest{AbsolutePath: good, RelativePath: "good.rs"})
	if err != nil {
		t.Fatal(err)
	}
	if !next.Success || !next.GetHints().GetHasMain() {
		t.Fatalf("successful response after parse diagnostic: %v", next)
	}
}

func TestRealParserEOFShutdownIsIdempotent(t *testing.T) {
	p := NewParser()
	if err := p.cleanup(); err != nil {
		t.Fatal(err)
	}
	if p.cmd.ProcessState == nil || !p.cmd.ProcessState.Success() {
		t.Fatalf("parser did not exit successfully after EOF: %v", p.cmd.ProcessState)
	}
	if err := p.cleanup(); err != nil {
		t.Fatalf("repeated cleanup: %v", err)
	}
}

func TestGenerationEntrypointReapsParser(t *testing.T) {
	l := NewLanguage().(*rustLang)
	t.Cleanup(func() { _ = l.Parser.cleanup() })
	dir := t.TempDir()
	parserFixture(t, dir, "WORKSPACE", "")
	parserFixture(t, dir, "BUILD.bazel", "")
	parserFixture(t, dir, "lib.rs", "pub fn library() {}\n")
	if err := update.Run(context.Background(), []languagev2.Language{compat.LanguageV2(l)}, dir, nil); err != nil {
		t.Fatal(err)
	}
	if l.Parser.cmd.ProcessState == nil || !l.Parser.cmd.ProcessState.Success() {
		t.Fatalf("generation left parser running or unsuccessful: %v", l.Parser.cmd.ProcessState)
	}
	generated, err := os.ReadFile(filepath.Join(dir, "BUILD.bazel"))
	if err != nil {
		t.Fatal(err)
	}
	if len(generated) == 0 {
		t.Fatal("generation did not produce BUILD rules")
	}
}

// The child is a test fixture, selected only inside the test executable. The
// production launcher and five-second grace period have no test-only modes.
func TestParserLifecycleChild(t *testing.T) {
	switch os.Getenv("GAZELLE_PARSER_LIFECYCLE_CHILD") {
	case "startup":
		NewParser()
		os.Exit(0)
	case "truncated-response":
		// The fixture remains alive until production cleanup sends EOF. A separate
		// watchdog bounds the child even if a fatal parent bypasses its context.
		go func() { time.Sleep(10 * time.Second); os.Exit(97) }()
		_, _ = os.Stdout.Write([]byte{2, 0})
		_ = os.Stdout.Close()
		_, _ = io.Copy(io.Discard, os.Stdin)
		os.Exit(0)
	case "fatal-exchange":
		executable, err := os.Executable()
		if err != nil {
			t.Fatal(err)
		}
		ctx, cancel := context.WithTimeout(context.Background(), 12*time.Second)
		defer cancel()
		cmd := exec.CommandContext(ctx, executable, "-test.run=^TestParserLifecycleChild$")
		cmd.Env = append(os.Environ(), "GAZELLE_PARSER_LIFECYCLE_CHILD=truncated-response")
		p, err := startParser(cmd)
		if err != nil {
			t.Fatal(err)
		}
		log.SetOutput(terminationDiagnostic{pid: cmd.Process.Pid})
		l := &rustLang{Parser: p}
		l.parseFile(config.New(), "lib.rs", nil, &language.GenerateArgs{Dir: os.TempDir()}, nil, nil, nil, nil)
		t.Fatal("terminal exchange returned from fatal path")
	case "unresponsive":
		fmt.Fprint(os.Stdout, "ready")
		for {
			time.Sleep(time.Hour)
		}
	}
}

func TestParserStartupFailureIsFatalWithLaunchContext(t *testing.T) {
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	dir := t.TempDir()
	// Runfile resolution succeeds, but this non-executable fixture cannot launch.
	parserFixture(t, dir, "3rdparty/gazelle_rust/upstream/rust_parser/rust_parser", "not executable")
	ctx, cancel := context.WithTimeout(context.Background(), 12*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, executable, "-test.run=^TestParserLifecycleChild$")
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "GAZELLE_PARSER_LIFECYCLE_CHILD=startup")
	output, err := cmd.CombinedOutput()
	if err == nil || ctx.Err() != nil {
		t.Fatalf("startup did not fail promptly: %v (%v), output=%s", err, ctx.Err(), output)
	}
	if !strings.Contains(string(output), "start parser") || !strings.Contains(string(output), "rust_parser") {
		t.Fatalf("missing launch context in diagnostic: %s", output)
	}
}

func TestParserKillsAndReapsUnresponsiveChildAfterGrace(t *testing.T) {
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	// This independent outer deadline also terminates the child if cleanup loses
	// its timeout, so a regression cannot strand the harness or its child.
	ctx, cancel := context.WithTimeout(context.Background(), 12*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, executable, "-test.run=^TestParserLifecycleChild$")
	cmd.Env = append(os.Environ(), "GAZELLE_PARSER_LIFECYCLE_CHILD=unresponsive")
	p, err := startParser(cmd)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = p.cleanup() })
	ready := make([]byte, 5)
	if _, err := io.ReadFull(p.stdout, ready); err != nil || string(ready) != "ready" {
		t.Fatalf("child readiness: %q, %v", ready, err)
	}
	started := time.Now()
	err = p.cleanup()
	var exitError *exec.ExitError
	if !errors.As(err, &exitError) || !strings.Contains(err.Error(), "shutdown timeout") {
		t.Fatalf("shutdown did not report forced termination: %v", err)
	}
	if elapsed := time.Since(started); elapsed < 5*time.Second || ctx.Err() != nil {
		t.Fatalf("shutdown grace: %v, outer deadline: %v", elapsed, ctx.Err())
	}
	if err := cmd.Process.Signal(os.Interrupt); !errors.Is(err, os.ErrProcessDone) {
		t.Fatalf("child was not reaped: %v", err)
	}
	if again := p.cleanup(); again == nil || again.Error() != err.Error() {
		t.Fatalf("repeated forced cleanup: first %v, again %v", err, again)
	}
}

// Stream collaborators are the private standard-I/O boundary; requests still
// exercise the public typed parsing operations and production wire protocol.
type parserWriter struct {
	bytes.Buffer
	limit    int
	writeErr error
	closeErr error
}

func (w *parserWriter) Write(b []byte) (int, error) {
	if w.writeErr != nil {
		return 0, w.writeErr
	}
	if w.limit >= 0 && len(b) > w.limit {
		b = b[:w.limit]
	}
	return w.Buffer.Write(b)
}
func (w *parserWriter) Close() error { return w.closeErr }
func streamParser(w *parserWriter, response []byte) *Parser {
	return &Parser{stdin: w, stdout: io.NopCloser(bytes.NewReader(response)), marshalOptions: &proto.MarshalOptions{}, responseBuffer: make([]byte, 1024)}
}

func TestParserFailedWriteIsTerminal(t *testing.T) {
	failure := errors.New("broken transport")
	p := streamParser(&parserWriter{limit: -1, writeErr: failure, closeErr: errors.New("cleanup failure")}, nil)
	_, err := p.Parse(&pb.RustImportsRequest{RelativePath: "lib.rs"})
	if !errors.Is(err, failure) || !strings.Contains(err.Error(), "write") || !strings.Contains(err.Error(), "cleanup failure") {
		t.Fatalf("write failure lost context or cleanup: %v", err)
	}
	_, err = p.ParseCargoToml(&pb.CargoTomlRequest{})
	if err == nil || !strings.Contains(err.Error(), "closed") {
		t.Fatalf("failed session resumed: %v", err)
	}
}

func TestParserCompletesShortWrites(t *testing.T) {
	w := &parserWriter{limit: 1}
	// Literal wire frame: RustImportsResponse.success = true.
	p := streamParser(w, []byte{2, 0, 0, 0, 56, 1})
	response, err := p.Parse(&pb.RustImportsRequest{RelativePath: "lib.rs"})
	if err != nil || !response.GetSuccess() {
		t.Fatalf("short write exchange: %v, %v", response, err)
	}
	// Request.kind.rust_imports.relative_path = "lib.rs".
	if want := []byte{10, 0, 0, 0, 10, 8, 18, 6, 'l', 'i', 'b', '.', 'r', 's'}; !bytes.Equal(w.Bytes(), want) {
		t.Fatalf("incomplete request: %x", w.Bytes())
	}
}

func TestParserZeroProgressWriteFails(t *testing.T) {
	p := streamParser(&parserWriter{limit: 0}, nil)
	_, err := p.GetLockfileCrates(&pb.LockfileCratesRequest{})
	if !errors.Is(err, io.ErrShortWrite) || !strings.Contains(err.Error(), "write") {
		t.Fatalf("zero-progress write: %v", err)
	}
}

func TestParserTruncatedPrefixHasContext(t *testing.T) {
	p := streamParser(&parserWriter{limit: -1}, []byte{2, 0})
	_, err := p.ParseCargoToml(&pb.CargoTomlRequest{})
	if !errors.Is(err, io.ErrUnexpectedEOF) || !strings.Contains(err.Error(), "prefix") {
		t.Fatalf("truncated prefix: %v", err)
	}
	_, err = p.GetLockfileCrates(&pb.LockfileCratesRequest{})
	if err == nil || !strings.Contains(err.Error(), "closed") {
		t.Fatalf("session resumed after truncated prefix: %v", err)
	}
}

func TestParserResponseFailuresHaveContext(t *testing.T) {
	for _, tc := range []struct {
		name    string
		frame   []byte
		context string
		cause   error
	}{
		{"body", []byte{2, 0, 0, 0, 56}, "body", io.ErrUnexpectedEOF},
		{"protobuf", []byte{1, 0, 0, 0, 255}, "decode", nil},
	} {
		t.Run(tc.name, func(t *testing.T) {
			p := streamParser(&parserWriter{limit: -1}, tc.frame)
			_, err := p.GetLockfileCrates(&pb.LockfileCratesRequest{})
			if err == nil || !strings.Contains(err.Error(), tc.context) || (tc.cause != nil && !errors.Is(err, tc.cause)) {
				t.Fatalf("response error lacks context/cause: %v", err)
			}
			_, err = p.Parse(&pb.RustImportsRequest{})
			if err == nil || !strings.Contains(err.Error(), "closed") {
				t.Fatalf("session resumed after response failure: %v", err)
			}
		})
	}
}

// Observe the OS process boundary at the instant the production fatal logger
// emits its diagnostic. This does not depend on defers after log.Fatal.
type terminationDiagnostic struct{ pid int }

func (d terminationDiagnostic) Write(b []byte) (int, error) {
	if errors.Is(syscall.Kill(d.pid, 0), syscall.ESRCH) {
		fmt.Fprintln(os.Stderr, "child terminated before fatal diagnostic")
	} else {
		fmt.Fprintln(os.Stderr, "child still alive at fatal diagnostic")
	}
	return os.Stderr.Write(b)
}

func TestParserTerminatesChildBeforeFatalDiagnostic(t *testing.T) {
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, executable, "-test.run=^TestParserLifecycleChild$")
	cmd.Env = append(os.Environ(), "GAZELLE_PARSER_LIFECYCLE_CHILD=fatal-exchange")
	output, err := cmd.CombinedOutput()
	if err == nil || ctx.Err() != nil {
		t.Fatalf("fatal exchange did not exit promptly: %v (%v), output=%s", err, ctx.Err(), output)
	}
	if !strings.Contains(string(output), "child terminated before fatal diagnostic") || !strings.Contains(string(output), "failed to parse lib.rs") || !strings.Contains(string(output), "prefix") {
		t.Fatalf("missing terminated child or exchange diagnostic: %s", output)
	}
}
