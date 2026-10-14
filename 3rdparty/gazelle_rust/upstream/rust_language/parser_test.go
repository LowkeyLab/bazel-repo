package rust_language

import (
	"os"
	"path/filepath"
	"testing"

	pb "github.com/calsign/gazelle_rust/proto"
	"google.golang.org/protobuf/proto"
)

func realParser(t *testing.T) *Parser {
	t.Helper()
	p := NewParser()
	t.Cleanup(func() {
		_ = p.stdin.Close()
		_ = p.cmd.Wait()
		_ = p.stdout.Close()
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
