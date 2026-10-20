package main

import (
	"archive/tar"
	"bytes"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"database/sql"
	"database/sql/driver"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/bazelbuild/rules_go/go/runfiles"
	"github.com/jackc/pgx/v5/stdlib"
	db "github.com/lowkeylab/bazel-repo/predix/internal/db"
	testimages "github.com/lowkeylab/bazel-repo/test_images/go"
	"github.com/moby/moby/api/types/container"
	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/modules/postgres"
	"github.com/testcontainers/testcontainers-go/wait"
)

type (
	sourceKey     struct{}
	sourceContext struct {
		Statement  Statement
		Test       string
		RuntimeSQL string
	}
	trackedDriver struct{ inner driver.Driver }
	trackedConn   struct{ driver.Conn }
	trackedRows   struct {
		driver.Rows
		conn   *trackedConn
		ctx    context.Context
		meta   sourceContext
		query  string
		closed bool
	}
)

var state struct {
	DSN               string
	Admin             *sql.DB
	Inventory         Inventory
	Sources           map[string][]Statement
	NativeDir         string
	Generated         string
	Embedded          string
	EmbeddedStatement Statement
	Errors            []string
	Lock              sync.Mutex
}

func writeJSON(name string, value any) {
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		panic(err)
	}
	if err = os.WriteFile(filepath.Join(state.NativeDir, name), data, 0o600); err != nil {
		panic(err)
	}
}

func sourceHash(data []byte) string {
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:])
}

func runfile(name string) string {
	path, err := runfiles.Rlocation(name)
	if err != nil {
		panic(err)
	}
	return path
}

func input(name string) []byte {
	data, err := os.ReadFile(runfile(os.Getenv(name)))
	if err != nil {
		panic(err)
	}
	return data
}

func (d *trackedDriver) Open(name string) (driver.Conn, error) {
	conn, err := d.inner.Open(name)
	if err != nil {
		return nil, err
	}
	return &trackedConn{conn}, nil
}

func (c *trackedConn) CheckNamedValue(value *driver.NamedValue) error {
	if checker, ok := c.Conn.(driver.NamedValueChecker); ok {
		return checker.CheckNamedValue(value)
	}
	return driver.ErrSkip
}

func (c *trackedConn) record(ctx context.Context, meta sourceContext, query string) {
	if meta.RuntimeSQL != query {
		state.Lock.Lock()
		state.Errors = append(state.Errors, "Runtime SQL does not match its declared source mapping")
		state.Lock.Unlock()
		return
	}
	statement := meta.Statement
	args := []driver.NamedValue{{Ordinal: 1, Value: meta.Test}, {Ordinal: 2, Value: statement.Path}, {Ordinal: 3, Value: statement.SourceSHA}, {Ordinal: 4, Value: int64(statement.Start)}, {Ordinal: 5, Value: int64(statement.End)}, {Ordinal: 6, Value: int64(statement.Line)}, {Ordinal: 7, Value: sourceHash([]byte(query))}}
	_, err := c.Conn.(driver.ExecerContext).ExecContext(ctx, "INSERT INTO public.research_receipts(test_name,source_path,source_sha,start_byte,end_byte,start_line,runtime_sql_sha,backend_pid) VALUES($1,$2,$3,$4,$5,$6,$7,pg_backend_pid())", args)
	if err != nil {
		state.Lock.Lock()
		state.Errors = append(state.Errors, err.Error())
		state.Lock.Unlock()
	}
}

func (c *trackedConn) ExecContext(ctx context.Context, query string, args []driver.NamedValue) (driver.Result, error) {
	result, err := c.Conn.(driver.ExecerContext).ExecContext(ctx, query, args)
	if meta, ok := ctx.Value(sourceKey{}).(sourceContext); ok && err == nil {
		c.record(ctx, meta, query)
	}
	return result, err
}

func (c *trackedConn) QueryContext(ctx context.Context, query string, args []driver.NamedValue) (driver.Rows, error) {
	rows, err := c.Conn.(driver.QueryerContext).QueryContext(ctx, query, args)
	if err != nil {
		return nil, err
	}
	if meta, ok := ctx.Value(sourceKey{}).(sourceContext); ok {
		return &trackedRows{Rows: rows, conn: c, ctx: ctx, meta: meta, query: query}, nil
	}
	return rows, nil
}

func (r *trackedRows) Close() error {
	if r.closed {
		return nil
	}
	r.closed = true
	err := r.Rows.Close()
	if err == nil {
		r.conn.record(r.ctx, r.meta, r.query)
	}
	return err
}

func queryJSON(ctx context.Context, query string) any {
	rows, err := state.Admin.QueryContext(ctx, query)
	if err != nil {
		return map[string]any{"error": err.Error(), "query": query}
	}
	defer rows.Close()
	columns, err := rows.Columns()
	if err != nil {
		panic(err)
	}
	output := []map[string]any{}
	for rows.Next() {
		values := make([]any, len(columns))
		pointers := make([]any, len(columns))
		for i := range values {
			pointers[i] = &values[i]
		}
		if err = rows.Scan(pointers...); err != nil {
			panic(err)
		}
		record := map[string]any{}
		for i, column := range columns {
			if value, ok := values[i].([]byte); ok {
				record[column] = string(value)
			} else {
				record[column] = values[i]
			}
		}
		output = append(output, record)
	}
	if err = rows.Err(); err != nil {
		panic(err)
	}
	return output
}

func profilerFiles(data []byte) []testcontainers.ContainerFile {
	files := []testcontainers.ContainerFile{}
	archive := tar.NewReader(bytes.NewReader(data))
	for {
		header, err := archive.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			panic(err)
		}
		content, err := io.ReadAll(archive)
		if err != nil {
			panic(err)
		}
		files = append(files, testcontainers.ContainerFile{Reader: bytes.NewReader(content), ContainerFilePath: "/" + header.Name, FileMode: 0o644})
	}
	return files
}

func collect(ctx context.Context, name string) {
	writeJSON(name+"-receipts.json", queryJSON(ctx, "SELECT * FROM public.research_receipts ORDER BY id"))
	writeJSON(name+"-statements.json", queryJSON(ctx, "SELECT queryid::text,toplevel,query,calls,rows FROM pg_stat_statements WHERE query ILIKE '%research_values%' OR query ILIKE '%FROM users%' OR query ILIKE '%FROM servers%' ORDER BY queryid"))
	writeJSON(name+"-profiler-lines.json", queryJSON(ctx, "SELECT * FROM plpgsql_profiler_function_tb('research_branch(boolean)'::regprocedure)"))
	writeJSON(name+"-profiler-statements.json", queryJSON(ctx, "SELECT * FROM plpgsql_profiler_function_statements_tb('research_branch(boolean)'::regprocedure)"))
	writeJSON(name+"-profiler-functions.json", queryJSON(ctx, "SELECT * FROM plpgsql_profiler_functions_all()"))
	writeJSON(name+"-profiler-coverage.json", queryJSON(ctx, "SELECT plpgsql_coverage_statements('research_branch(boolean)'::regprocedure) AS statements,plpgsql_coverage_branches('research_branch(boolean)'::regprocedure) AS branches"))
	writeJSON(name+"-adapter-errors.json", state.Errors)
}

func setupSources() {
	var provision any
	if err := json.Unmarshal(input("PROFILER_MANIFEST"), &provision); err != nil {
		panic(err)
	}
	writeJSON("provision.json", provision)
	state.Sources = map[string][]Statement{}
	for _, item := range []struct{ Env, Path string }{{"SQL_FIXTURE", "predix/internal/research_sql/statements.sql"}, {"SQL_DISCONNECTED", "predix/internal/research_sql/disconnected.sql"}, {"PROFILE_CONTROL", "predix/internal/research_sql/profile_control.sql"}, {"ORIGINAL_QUERIES", "predix/internal/sql/query.sql"}} {
		inventory := inventoryPaths([]string{runfile(os.Getenv(item.Env))})
		state.Inventory.Version = inventory.Version
		for _, statement := range inventory.Statements {
			statement.Path = item.Path
			state.Sources[item.Path] = append(state.Sources[item.Path], statement)
			state.Inventory.Statements = append(state.Inventory.Statements, statement)
		}
	}
	generated := input("GENERATED_SNAPSHOT")
	file, err := parser.ParseFile(token.NewFileSet(), "query.sql.go", generated, 0)
	if err != nil {
		panic(err)
	}
	ast.Inspect(file, func(node ast.Node) bool {
		value, ok := node.(*ast.ValueSpec)
		if !ok {
			return true
		}
		for i, name := range value.Names {
			if name.Name == "getUserByUsername" {
				literal := value.Values[i].(*ast.BasicLit)
				state.Generated, err = strconv.Unquote(literal.Value)
				if err != nil {
					panic(err)
				}
			}
		}
		return true
	})
	if state.Generated == "" {
		panic("The generated query constant is missing")
	}
	host := input("HOST_SNAPSHOT")
	state.Embedded = "SELECT COUNT(*) FROM servers"
	offset := bytes.Index(host, []byte(state.Embedded))
	if bytes.Count(host, []byte(state.Embedded)) != 1 || offset < 0 {
		panic("The maintained embedded SQL is missing")
	}
	state.EmbeddedStatement = Statement{Path: "nicknamer2/src/discord_server/repo/lib.rs", Start: offset, End: offset + len(state.Embedded), Line: 1 + bytes.Count(host[:offset], []byte("\n")), EndLine: 1 + bytes.Count(host[:offset], []byte("\n")), Kind: "SelectStmt", SQL: state.Embedded, SourceSHA: sourceHash(host)}
	literalPath := filepath.Join(os.Getenv("TEST_TMPDIR"), "embedded-original.sql")
	if err = os.WriteFile(literalPath, []byte(state.Embedded), 0o600); err != nil {
		panic(err)
	}
	parsedLiteral := inventoryPaths([]string{literalPath})
	if len(parsedLiteral.Statements) != 1 {
		panic("The embedded SQL does not parse as one statement")
	}
	state.EmbeddedStatement.Parse = parsedLiteral.Statements[0].Parse
	state.Inventory.Statements = append(state.Inventory.Statements, state.EmbeddedStatement)
	writeJSON("inventory.json", state.Inventory)
	writeJSON("source-mappings.json", map[string]any{"generated_snapshot_sha256": sourceHash(generated), "generated_runtime_sql": state.Generated, "generated_operation": "GetUserByUsername", "generated_origin": "predix/internal/sql/query.sql", "host_snapshot_sha256": sourceHash(host), "host_origin": state.EmbeddedStatement, "host_runtime_sql": state.Embedded, "host_runtime_limit": "The harness executes extracted SQL. It does not execute the Rust repository function."})
}

func runTests(m *testing.M) int {
	state.NativeDir = os.Getenv("TEST_UNDECLARED_OUTPUTS_DIR")
	if state.NativeDir == "" {
		panic("The evidence output directory is missing")
	}
	setupSources()
	ctx, cancel := context.WithTimeout(context.Background(), 150*time.Second)
	defer cancel()
	secret := make([]byte, 24)
	if _, err := rand.Read(secret); err != nil {
		panic(err)
	}
	pg, err := testimages.Postgres(ctx, postgres.WithDatabase("research"), postgres.WithUsername("research"), postgres.WithPassword(hex.EncodeToString(secret)), testcontainers.WithFiles(profilerFiles(input("PROFILER_FILES"))...), testcontainers.WithHostConfigModifier(func(config *container.HostConfig) {
		config.Resources.Memory = 384 * 1024 * 1024
		config.Resources.NanoCPUs = 1_000_000_000
	}), testcontainers.WithCmd("postgres", "-c", "shared_preload_libraries=pg_stat_statements,plpgsql_check", "-c", "shared_buffers=32MB", "-c", "max_connections=12", "-c", "compute_query_id=on", "-c", "pg_stat_statements.track=all", "-c", "plpgsql_check.profiler=on", "-c", "jit=off"), testcontainers.WithWaitStrategyAndDeadline(90*time.Second, wait.ForLog("database system is ready to accept connections").WithOccurrence(2).WithStartupTimeout(60*time.Second)))
	if err != nil {
		writeJSON("startup-error.json", map[string]any{"error": err.Error()})
		fmt.Fprintln(os.Stderr, "Database startup failed:", err)
		return 1
	}
	defer func() {
		logs, err := pg.Logs(context.Background())
		if err == nil {
			data, _ := io.ReadAll(logs)
			_ = logs.Close()
			_ = os.WriteFile(filepath.Join(state.NativeDir, "database.log"), data, 0o600)
		}
		stopCtx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		defer cancel()
		err = pg.Terminate(stopCtx)
		writeJSON("database-stop.json", map[string]any{"stopped": err == nil, "error": fmt.Sprint(err)})
	}()
	state.DSN, err = pg.ConnectionString(ctx, "sslmode=disable", "connect_timeout=10")
	if err != nil {
		panic(err)
	}
	state.Admin, err = sql.Open("pgx", state.DSN)
	if err != nil {
		panic(err)
	}
	defer state.Admin.Close()
	for _, query := range []string{"CREATE EXTENSION pg_stat_statements", "CREATE EXTENSION plpgsql_check", "CREATE TABLE public.research_receipts(id BIGSERIAL PRIMARY KEY,test_name TEXT,source_path TEXT,source_sha TEXT,start_byte INTEGER,end_byte INTEGER,start_line INTEGER,runtime_sql_sha TEXT,backend_pid INTEGER)", string(input("PROFILE_CONTROL"))} {
		if _, err = state.Admin.ExecContext(ctx, query); err != nil {
			writeJSON("setup-error.json", map[string]any{"query": query, "error": err.Error()})
			return 1
		}
	}
	writeJSON("server.json", queryJSON(ctx, "SELECT version(),current_setting('server_version_num') AS server_version_num,current_setting('shared_preload_libraries') AS preload"))
	writeJSON("extensions.json", queryJSON(ctx, "SELECT extname,extversion FROM pg_extension ORDER BY extname"))
	collect(ctx, "before")
	if _, err = state.Admin.ExecContext(ctx, "SELECT pg_stat_statements_reset()"); err != nil {
		panic(err)
	}
	sql.Register("coverage-postgres", &trackedDriver{inner: stdlib.GetDefaultDriver()})
	code := m.Run()
	collect(ctx, "after")
	writeJSON("functional.json", map[string]any{"exit": code, "applications": 2, "database_memory_bytes": 384 * 1024 * 1024, "database_nano_cpus": 1_000_000_000})
	return code
}

func TestMain(m *testing.M) { os.Exit(runTests(m)) }

func executeApplication(t *testing.T, name string) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 45*time.Second)
	defer cancel()
	database, err := sql.Open("coverage-postgres", state.DSN)
	if err != nil {
		t.Fatal(err)
	}
	defer database.Close()
	database.SetMaxOpenConns(1)
	for _, query := range []string{"CREATE SCHEMA " + name, "SET search_path TO " + name + ",public", "CREATE TABLE users(id SERIAL,username TEXT,password_hash TEXT,role TEXT,uuid UUID)", "INSERT INTO users(username,password_hash,role,uuid) VALUES('coverage_user','synthetic_hash','member','00000000-0000-0000-0000-000000000001')", "CREATE TABLE servers(id INTEGER)", "INSERT INTO servers VALUES(1)"} {
		if _, err = database.ExecContext(ctx, query); err != nil {
			t.Fatal(err)
		}
	}
	statements := state.Sources["predix/internal/research_sql/statements.sql"]
	for _, index := range []int{0, 1, 3} {
		statement := statements[index]
		marked := context.WithValue(ctx, sourceKey{}, sourceContext{Statement: statement, Test: name, RuntimeSQL: statement.SQL})
		if _, err = database.ExecContext(marked, statement.SQL); err != nil {
			t.Fatal(err)
		}
	}
	statement := statements[2]
	marked := context.WithValue(ctx, sourceKey{}, sourceContext{Statement: statement, Test: name, RuntimeSQL: statement.SQL})
	var ordinary int
	if err = database.QueryRowContext(marked, statement.SQL).Scan(&ordinary); err != nil || ordinary != 7 {
		t.Fatalf("Ordinary query: value=%d error=%v", ordinary, err)
	}
	var authored Statement
	authoredCount := 0
	for _, candidate := range state.Sources["predix/internal/sql/query.sql"] {
		if strings.Contains(candidate.SQL, "WHERE username = $1 LIMIT 1") {
			authored = candidate
			authoredCount++
		}
	}
	if authored.Path == "" || authoredCount != 1 {
		t.Fatal("The authored query source is missing")
	}
	marked = context.WithValue(ctx, sourceKey{}, sourceContext{Statement: authored, Test: name, RuntimeSQL: state.Generated})
	user, err := db.New(database).GetUserByUsername(marked, "coverage_user")
	if err != nil || user.Username != "coverage_user" {
		t.Fatalf("Generated application query: user=%v error=%v", user, err)
	}
	marked = context.WithValue(ctx, sourceKey{}, sourceContext{Statement: state.EmbeddedStatement, Test: name, RuntimeSQL: state.Embedded})
	var count int
	if err = database.QueryRowContext(marked, state.Embedded).Scan(&count); err != nil || count != 1 {
		t.Fatalf("Embedded SQL: count=%d error=%v", count, err)
	}
	var result int
	if err = database.QueryRowContext(ctx, "SELECT public.research_branch(true)").Scan(&result); err != nil || result != 23 {
		t.Fatalf("Procedural control: result=%d error=%v", result, err)
	}
	if err = database.QueryRowContext(ctx, "SELECT count(*) FROM research_values WHERE origin='anonymous'").Scan(&count); err != nil || count != 1 {
		t.Fatalf("Anonymous block: count=%d error=%v", count, err)
	}
	if err = database.QueryRowContext(ctx, "SELECT count(*) FROM research_values WHERE origin='untaken'").Scan(&count); err != nil || count != 0 {
		t.Fatalf("Untaken arm: count=%d error=%v", count, err)
	}
	collect(ctx, name)
	t.Log("FUNCTIONAL_APPLICATION_PASS", name)
}
func TestApplicationA(t *testing.T) { executeApplication(t, "application_a") }
func TestApplicationB(t *testing.T) { executeApplication(t, "application_b") }
