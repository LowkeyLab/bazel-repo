package research_caddy

import (
	"bufio"
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

type (
	location struct {
		Line                            int
		Column                          int
		Directive, Fingerprint, Pointer string
	}
	mapping struct {
		Source, SourceSHA256, AdaptSHA256 string
		Locations                         []location
		Unmeasured                        []int
	}
	completion struct {
		Complete      bool   `json:"complete"`
		LogSHA256     string `json:"log_sha256"`
		SourceSHA256  string `json:"source_sha256"`
		MappingSHA256 string `json:"mapping_sha256"`
	}
)

func digest(data []byte) string { return fmt.Sprintf("%x", sha256.Sum256(data)) }
func leaves(encoded []byte) (map[string]string, error) {
	var root any
	if err := json.Unmarshal(encoded, &root); err != nil {
		return nil, err
	}
	found := map[string]string{}
	var walk func(any, string) error
	walk = func(value any, pointer string) error {
		switch node := value.(type) {
		case map[string]any:
			handler, _ := node["handler"].(string)
			if handler == "headers" || handler == "static_response" {
				data, _ := json.Marshal(node)
				key := string(data)
				if _, exists := found[key]; exists {
					return fmt.Errorf("Ambiguous concrete configuration at %s", pointer)
				}
				found[key] = pointer
				return nil
			}
			for key, child := range node {
				if err := walk(child, pointer+"/"+key); err != nil {
					return err
				}
			}
		case []any:
			for index, child := range node {
				if err := walk(child, fmt.Sprintf("%s/%d", pointer, index)); err != nil {
					return err
				}
			}
		}
		return nil
	}
	return found, walk(root, "")
}

func buildMapping(t *testing.T, caddy string, original, adapted []byte, out string) []byte {
	t.Helper()
	all, err := leaves(adapted)
	if err != nil {
		t.Fatal(err)
	}
	mapped := mapping{Source: "tools/research_caddy/Caddyfile", SourceSHA256: digest(original), AdaptSHA256: digest(adapted)}
	lines := strings.Split(string(original), "\n")
	for index, line := range lines {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		directive := fields[0]
		if directive != "header" && directive != "redir" && directive != "respond" {
			continue
		}
		if strings.ContainsAny(line, "{}") {
			t.Fatal("Structural candidate is unsupported")
		}
		variant := append([]string{}, lines...)
		variant[index] = ""
		command := exec.CommandContext(t.Context(), caddy, "adapt", "--adapter", "caddyfile", "--config", "-", "--pretty")
		command.Stdin = strings.NewReader(strings.Join(variant, "\n"))
		var stderr bytes.Buffer
		command.Stderr = &stderr
		changed, err := command.Output()
		prefix := filepath.Join(out, fmt.Sprintf("deletion-%d", index+1))
		_ = os.WriteFile(prefix+".stderr", stderr.Bytes(), 0o600)
		_ = os.WriteFile(prefix+".json", changed, 0o600)
		if err != nil {
			t.Fatalf("Deletion line %d: %v", index+1, err)
		}
		remaining, err := leaves(changed)
		if err != nil {
			t.Fatal(err)
		}
		removed := []string{}
		for key := range remaining {
			if _, ok := all[key]; !ok {
				t.Fatal("Deletion changes another concrete configuration")
			}
		}
		for key := range all {
			if _, ok := remaining[key]; !ok {
				removed = append(removed, key)
			}
		}
		if len(removed) == 0 {
			mapped.Unmeasured = append(mapped.Unmeasured, index+1)
			continue
		}
		if len(removed) != 1 {
			t.Fatal("Deletion has ambiguous effects")
		}
		key := removed[0]
		mapped.Locations = append(mapped.Locations, location{index + 1, len(line) - len(strings.TrimLeft(line, " \t")) + 1, directive, key, all[key]})
	}
	if len(mapped.Locations) != len(all) || len(all) != 5 {
		t.Fatalf("Incomplete fixed-fixture mapping: %d/%d", len(mapped.Locations), len(all))
	}
	data, err := json.MarshalIndent(mapped, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(out, "mapping.json"), data, 0o600); err != nil {
		t.Fatal(err)
	}
	baseline := map[int]int{}
	for _, entry := range mapped.Locations {
		baseline[entry.Line] = 0
	}
	if err := os.WriteFile(filepath.Join(out, "baseline.info"), lcov(baseline), 0o600); err != nil {
		t.Fatal(err)
	}
	return data
}

func lcov(counts map[int]int) []byte {
	lines := []int{}
	for line := range counts {
		lines = append(lines, line)
	}
	sort.Ints(lines)
	var text strings.Builder
	text.WriteString("SF:tools/research_caddy/Caddyfile\n")
	for _, line := range lines {
		fmt.Fprintf(&text, "DA:%d,%d\n", line, counts[line])
	}
	text.WriteString("end_of_record\n")
	return []byte(text.String())
}

func collect(directory string) error {
	read := func(name string) ([]byte, error) { return os.ReadFile(filepath.Join(directory, name)) }
	source, err := read("source.txt")
	if err != nil {
		return err
	}
	mappingBytes, err := read("mapping.json")
	if err != nil {
		return err
	}
	trace, err := read("trace-runtime.jsonl")
	if err != nil {
		return err
	}
	receiptBytes, err := read("trace-completion.json")
	if err != nil {
		return err
	}
	var metadata mapping
	if err := json.Unmarshal(mappingBytes, &metadata); err != nil {
		return err
	}
	var receipt completion
	if err := json.Unmarshal(receiptBytes, &receipt); err != nil {
		return err
	}
	if !receipt.Complete || receipt.SourceSHA256 != digest(source) || metadata.SourceSHA256 != digest(source) || receipt.MappingSHA256 != digest(mappingBytes) || receipt.LogSHA256 != digest(trace) {
		return fmt.Errorf("Evidence hashes or completion do not match")
	}
	counts := map[int]int{}
	byConfig := map[string]int{}
	for _, entry := range metadata.Locations {
		if _, exists := byConfig[entry.Fingerprint]; exists {
			return fmt.Errorf("Ambiguous configuration correlation")
		}
		byConfig[entry.Fingerprint] = entry.Line
		counts[entry.Line] = 0
	}
	if len(counts) != 5 {
		return fmt.Errorf("Incomplete fixed-fixture baseline")
	}
	scanner := bufio.NewScanner(bytes.NewReader(trace))
	entries := 0
	unmapped := 0
	for scanner.Scan() {
		var event map[string]any
		if err := json.Unmarshal(scanner.Bytes(), &event); err != nil {
			return err
		}
		if event["logger"] != "http.log.trace" {
			continue
		}
		name, _ := event["msg"].(string)
		if name == "subroute" {
			unmapped++
			continue
		}
		module, ok := event["module"].(map[string]any)
		if !ok {
			return fmt.Errorf("Missing module configuration")
		}
		module["handler"] = name
		key, _ := json.Marshal(module)
		line, exists := byConfig[string(key)]
		if !exists {
			return fmt.Errorf("Unknown concrete handler configuration")
		}
		counts[line]++
		entries++
	}
	if err := scanner.Err(); err != nil {
		return err
	}
	if entries == 0 {
		return fmt.Errorf("No concrete handler events")
	}
	// A completion receipt cannot prove that every expected event reached the log.
	if err := os.WriteFile(filepath.Join(directory, "measured.info"), lcov(counts), 0o600); err != nil {
		return err
	}
	zero := map[int]int{}
	for line := range counts {
		zero[line] = 0
	}
	for _, order := range []string{"baseline-first", "measured-first"} {
		combined := map[int]int{}
		operands := []map[int]int{zero, counts}
		if order == "measured-first" {
			operands = []map[int]int{counts, zero}
		}
		for _, operand := range operands {
			for line, count := range operand {
				combined[line] += count
			}
		}
		if err := os.WriteFile(filepath.Join(directory, order+".info"), lcov(combined), 0o600); err != nil {
			return err
		}
	}
	return os.WriteFile(filepath.Join(directory, "collection.json"), []byte(fmt.Sprintf("{\"concrete_entries\":%d,\"unmapped_subroutes\":%d}\n", entries, unmapped)), 0o600)
}

func TestCollectorProcess(t *testing.T) {
	if os.Getenv("CADDY_COLLECTOR_CHILD") != "1" {
		return
	}
	if err := collect(os.Getenv("CADDY_COLLECTOR_DIR")); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	os.Exit(0)
}

func checkCollection(t *testing.T, out string, original, mappingBytes, trace []byte) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(out, "source.txt"), original, 0o600); err != nil {
		t.Fatal(err)
	}
	receipt, _ := json.Marshal(completion{true, digest(trace), digest(original), digest(mappingBytes)})
	if err := os.WriteFile(filepath.Join(out, "trace-completion.json"), receipt, 0o600); err != nil {
		t.Fatal(err)
	}
	run := func(name string, wantFailure bool) {
		t.Helper()
		command := exec.CommandContext(t.Context(), os.Args[0], "-test.run=^TestCollectorProcess$")
		command.Env = append(os.Environ(), "CADDY_COLLECTOR_CHILD=1", "CADDY_COLLECTOR_DIR="+out)
		log, err := command.CombinedOutput()
		_ = os.WriteFile(filepath.Join(out, name+".txt"), log, 0o600)
		if (err != nil) != wantFailure {
			t.Fatalf("Collector %s: %v %s", name, err, log)
		}
	}
	run("positive", false)
	expected := []byte("SF:tools/research_caddy/Caddyfile\nDA:16,1\nDA:18,1\nDA:19,1\nDA:22,0\nDA:24,0\nend_of_record\n")
	for _, name := range []string{"measured.info", "baseline-first.info", "measured-first.info"} {
		actual, err := os.ReadFile(filepath.Join(out, name))
		if err != nil || !bytes.Equal(actual, expected) {
			t.Fatalf("Unexpected %s: %s (%v)", name, actual, err)
		}
	}
	for _, name := range []string{"trace-runtime.jsonl", "mapping.json", "trace-completion.json"} {
		path := filepath.Join(out, name)
		data, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		if err := os.Remove(path); err != nil {
			t.Fatal(err)
		}
		run("missing-"+name, true)
		if err := os.WriteFile(path, data, 0o600); err != nil {
			t.Fatal(err)
		}
	}
	_ = os.WriteFile(filepath.Join(out, "trace-runtime.jsonl"), append(trace, []byte("broken JSON\n")...), 0o600)
	corruptReceipt, _ := json.Marshal(completion{true, digest(append(trace, []byte("broken JSON\n")...)), digest(original), digest(mappingBytes)})
	_ = os.WriteFile(filepath.Join(out, "trace-completion.json"), corruptReceipt, 0o600)
	run("corrupt-trace", true)
	_ = os.WriteFile(filepath.Join(out, "trace-runtime.jsonl"), trace, 0o600)
	_ = os.WriteFile(filepath.Join(out, "trace-completion.json"), receipt, 0o600)
	var ambiguous mapping
	_ = json.Unmarshal(mappingBytes, &ambiguous)
	ambiguous.Locations = append(ambiguous.Locations, ambiguous.Locations[0])
	badMap, _ := json.Marshal(ambiguous)
	_ = os.WriteFile(filepath.Join(out, "mapping.json"), badMap, 0o600)
	badReceipt, _ := json.Marshal(completion{true, digest(trace), digest(original), digest(badMap)})
	_ = os.WriteFile(filepath.Join(out, "trace-completion.json"), badReceipt, 0o600)
	run("ambiguous-map", true)
	_ = os.WriteFile(filepath.Join(out, "mapping.json"), mappingBytes, 0o600)
	_ = os.WriteFile(filepath.Join(out, "trace-completion.json"), receipt, 0o600)
	_ = os.WriteFile(filepath.Join(out, "controls.txt"), []byte("Five child collectors failed after successful functional assertions.\nExact SF/DA records match the independent fixture oracle in both merge orders.\n"), 0o600)
}
