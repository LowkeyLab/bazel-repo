package main

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"
)

type BodyLocation struct {
	Path      string `json:"path"`
	SourceSHA string `json:"source_sha256"`
	Line      int    `json:"line"`
	Kind      string `json:"kind"`
	StartByte int    `json:"start_byte"`
}
type BodyBranch struct {
	Path     string `json:"path"`
	Line     int    `json:"line"`
	ThenLine int    `json:"then_line"`
	ElseLine int    `json:"else_line"`
}
type BodyTransform struct {
	Path               string         `json:"path"`
	OriginalSQLSHA     string         `json:"original_sql_sha256"`
	InstrumentedSQL    string         `json:"instrumented_sql"`
	InstrumentedSQLSHA string         `json:"instrumented_sql_sha256"`
	Locations          []BodyLocation `json:"locations"`
	Branches           []BodyBranch   `json:"branches"`
	Executed           bool           `json:"executed"`
}

var bodyTransforms = map[string]BodyTransform{}

func walkProcedural(value any, visit func(string, map[string]any)) {
	switch value := value.(type) {
	case map[string]any:
		for key, item := range value {
			if strings.HasPrefix(key, "PLpgSQL_stmt_") {
				if node, ok := item.(map[string]any); ok {
					visit(key, node)
				}
			}
			walkProcedural(item, visit)
		}
	case []any:
		for _, item := range value {
			walkProcedural(item, visit)
		}
	}
}

func firstProceduralLine(value any) int {
	first := 0
	walkProcedural(value, func(_ string, node map[string]any) {
		if n, ok := node["lineno"].(float64); ok && n > 0 && (first == 0 || int(n) < first) {
			first = int(n)
		}
	})
	return first
}

func setupBodies() {
	tagPattern := regexp.MustCompile(`^DO\s+(\$[A-Za-z0-9_]*\$)`)
	for path, statements := range state.Sources {
		for _, statement := range statements {
			if statement.Kind != "DoStmt" {
				continue
			}
			if regexp.MustCompile(`(?i)\b(found|diagnostics|commit|rollback)\b`).MatchString(statement.SQL) {
				panic("The bounded body adapter cannot preserve status or transaction semantics")
			}
			match := tagPattern.FindStringSubmatchIndex(statement.SQL)
			if len(match) != 4 {
				panic("The anonymous-body adapter requires a literal dollar-quoted DO body")
			}
			originLine := statement.Line + strings.Count(statement.SQL[:match[1]], "\n")
			var tree any
			if err := json.Unmarshal(statement.ProceduralParse, &tree); err != nil {
				panic(err)
			}
			plan := BodyTransform{Path: path, OriginalSQLSHA: sourceHash([]byte(statement.SQL)), Executed: !strings.HasSuffix(path, "disconnected_body.sql")}
			locations := map[int]BodyLocation{}
			walkProcedural(tree, func(kind string, node map[string]any) {
				lineValue, ok := node["lineno"].(float64)
				if !ok || lineValue <= 0 {
					return
				}
				switch kind {
				case "PLpgSQL_stmt_block", "PLpgSQL_stmt_if", "PLpgSQL_stmt_execsql", "PLpgSQL_stmt_dynexecute", "PLpgSQL_stmt_perform", "PLpgSQL_stmt_raise":
				default:
					panic("The bounded body adapter found an unsupported statement: " + kind)
				}
				line := originLine + int(lineValue) - 1
				if _, exists := locations[line]; exists {
					panic("Two procedural statements share one original line")
				}
				originalLines := strings.Split(statement.SQL, "\n")
				index := line - statement.Line
				offset := statement.Start
				for n := 0; n < index; n++ {
					offset += len(originalLines[n]) + 1
				}
				offset += len(originalLines[index]) - len(strings.TrimLeft(originalLines[index], " \t"))
				locations[line] = BodyLocation{Path: path, SourceSHA: statement.SourceSHA, Line: line, Kind: kind, StartByte: offset}
				if kind == "PLpgSQL_stmt_if" {
					thenLine, elseLine := firstProceduralLine(node["then_body"]), firstProceduralLine(node["else_body"])
					if thenLine == 0 || elseLine == 0 {
						panic("The bounded branch adapter requires two nonempty arms")
					}
					plan.Branches = append(plan.Branches, BodyBranch{Path: path, Line: line, ThenLine: originLine + thenLine - 1, ElseLine: originLine + elseLine - 1})
				}
			})
			for _, location := range locations {
				plan.Locations = append(plan.Locations, location)
			}
			sort.Slice(plan.Locations, func(i, j int) bool { return plan.Locations[i].Line < plan.Locations[j].Line })
			originalLines := strings.Split(statement.SQL, "\n")
			instrumented := []string{}
			for index, line := range originalLines {
				originalLine := statement.Line + index
				location, known := locations[originalLine]
				hook := fmt.Sprintf("PERFORM public.research_body_hit('%s',%d);", strings.ReplaceAll(path, "'", "''"), originalLine)
				if known && location.Kind == "PLpgSQL_stmt_block" {
					if strings.TrimSpace(line) != "BEGIN" {
						panic("The bounded block adapter requires a plain BEGIN line")
					}
					instrumented = append(instrumented, line, hook)
				} else if known {
					instrumented = append(instrumented, hook, line)
				} else {
					instrumented = append(instrumented, line)
				}
			}
			plan.InstrumentedSQL = strings.Join(instrumented, "\n")
			plan.InstrumentedSQLSHA = sourceHash([]byte(plan.InstrumentedSQL))
			bodyTransforms[path] = plan
		}
	}
	writeJSON("body-plan.json", bodyTransforms)
}

func setupBodyDatabase(ctx context.Context) error {
	for _, query := range []string{
		"CREATE TABLE public.research_body_profile(source_path TEXT,source_sha TEXT,original_line INTEGER,kind TEXT,hits BIGINT DEFAULT 0,PRIMARY KEY(source_path,original_line))",
		"CREATE FUNCTION public.research_body_hit(TEXT,INTEGER) RETURNS void LANGUAGE SQL AS $$UPDATE public.research_body_profile SET hits=hits+1 WHERE source_path=$1 AND original_line=$2$$",
	} {
		if _, err := state.Admin.ExecContext(ctx, query); err != nil {
			return err
		}
	}
	for _, plan := range bodyTransforms {
		for _, location := range plan.Locations {
			if _, err := state.Admin.ExecContext(ctx, "INSERT INTO public.research_body_profile(source_path,source_sha,original_line,kind) VALUES($1,$2,$3,$4)", location.Path, location.SourceSHA, location.Line, location.Kind); err != nil {
				return err
			}
		}
	}
	return nil
}

func executeBody(ctx context.Context, database *sql.DB, path, name string) error {
	plan := bodyTransforms[path]
	var statement Statement
	for _, candidate := range state.Sources[path] {
		if candidate.Kind == "DoStmt" {
			statement = candidate
		}
	}
	marked := context.WithValue(ctx, sourceKey{}, sourceContext{Statement: statement, Test: name, RuntimeSQL: plan.InstrumentedSQL})
	_, err := database.ExecContext(marked, plan.InstrumentedSQL)
	return err
}

func executeMaintainedRust(t *testing.T, ctx context.Context, name string) {
	t.Helper()
	statement := state.EmbeddedStatement
	receipt := filepath.Join(state.NativeDir, name+"-rust-call.json")
	command := exec.CommandContext(ctx, runfile(os.Getenv("RUST_PROBE")))
	command.Env = append(os.Environ(), "COVERAGE_RESEARCH_DSN="+state.DSN, "COVERAGE_RESEARCH_SCHEMA="+name, "COVERAGE_RESEARCH_RECEIPT="+receipt,
		"COVERAGE_SOURCE_SHA="+statement.SourceSHA, "COVERAGE_SOURCE_START="+strconv.Itoa(statement.Start), "COVERAGE_SOURCE_END="+strconv.Itoa(statement.End), "COVERAGE_SOURCE_LINE="+strconv.Itoa(statement.Line), "COVERAGE_RUNTIME_SHA="+sourceHash([]byte(state.Embedded)))
	output, err := command.CombinedOutput()
	if err != nil {
		t.Fatalf("Maintained Rust function failed: %v: %s", err, output)
	}
	if err = os.WriteFile(filepath.Join(state.NativeDir, name+"-rust-process.log"), output, 0o600); err != nil {
		t.Fatal(err)
	}
}
