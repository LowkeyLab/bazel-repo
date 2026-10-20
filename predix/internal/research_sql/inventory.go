// Package main records PostgreSQL parse positions without database execution.
package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"strings"

	pgquery "github.com/pganalyze/pg_query_go/v6"
)

type Statement struct {
	Path            string          `json:"path"`
	Start           int             `json:"start"`
	End             int             `json:"end"`
	Line            int             `json:"line"`
	EndLine         int             `json:"end_line"`
	Kind            string          `json:"kind"`
	SQL             string          `json:"sql"`
	SourceSHA       string          `json:"source_sha256"`
	Parse           json.RawMessage `json:"parse"`
	ProceduralParse json.RawMessage `json:"procedural_parse,omitempty"`
	ProceduralError string          `json:"procedural_error,omitempty"`
}

type Inventory struct {
	Version    int         `json:"parser_version"`
	Statements []Statement `json:"statements"`
}

func inventoryPaths(paths []string) Inventory {
	result := Inventory{}
	for _, path := range paths {
		data, err := os.ReadFile(path)
		if err != nil {
			panic(err)
		}
		text := string(data)
		digest := sha256.Sum256(data)
		parsed, err := pgquery.ParseToJSON(text)
		if err != nil {
			panic(err)
		}
		var tree struct {
			Version int `json:"version"`
			Stmts   []struct {
				Stmt   json.RawMessage `json:"stmt"`
				Start  int             `json:"stmt_location"`
				Length int             `json:"stmt_len"`
			} `json:"stmts"`
		}
		if err := json.Unmarshal([]byte(parsed), &tree); err != nil {
			panic(err)
		}
		result.Version = tree.Version
		scanned, err := pgquery.Scan(text)
		if err != nil {
			panic(err)
		}
		for _, raw := range tree.Stmts {
			start, end := raw.Start, raw.Start+raw.Length
			if raw.Length == 0 {
				end = len(data)
			}
			for _, token := range scanned.GetTokens() {
				offset := int(token.GetStart())
				if offset < start || offset >= end {
					continue
				}
				value := strings.TrimSpace(text[offset:int(token.GetEnd())])
				if strings.HasPrefix(value, "--") || strings.HasPrefix(value, "/*") {
					continue
				}
				start = offset
				break
			}
			var node map[string]json.RawMessage
			if err := json.Unmarshal(raw.Stmt, &node); err != nil {
				panic(err)
			}
			kind := ""
			for key := range node {
				kind = key
			}
			item := Statement{Path: path, Start: start, End: end, Line: 1 + strings.Count(text[:start], "\n"), EndLine: 1 + strings.Count(text[:end], "\n"), Kind: kind, SQL: text[start:end], SourceSHA: hex.EncodeToString(digest[:]), Parse: raw.Stmt}
			if kind == "DoStmt" || kind == "CreateFunctionStmt" {
				value, err := pgquery.ParsePlPgSqlToJSON(item.SQL)
				if err != nil {
					item.ProceduralError = err.Error()
				} else {
					item.ProceduralParse = json.RawMessage(value)
				}
			}
			result.Statements = append(result.Statements, item)
		}
	}
	return result
}

func main() {
	result := inventoryPaths(os.Args[1:])
	data, err := json.MarshalIndent(result, "", "  ")
	if err != nil {
		panic(err)
	}
	fmt.Println(string(data))
}
