// Package main records parsed Starlark statement locations without evaluation.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"sort"

	"go.starlark.net/syntax"
)

func main() {
	result := map[string][]int32{}
	for _, path := range os.Args[1:] {
		file, err := syntax.Parse(path, nil, 0)
		if err != nil {
			panic(err)
		}
		lines := map[int32]bool{}
		syntax.Walk(file, func(node syntax.Node) bool {
			if stmt, ok := node.(syntax.Stmt); ok {
				// String expression statements include documentation, not executable work.
				if expr, ok := stmt.(*syntax.ExprStmt); ok {
					if literal, ok := expr.X.(*syntax.Literal); ok && literal.Token == syntax.STRING {
						return true
					}
				}
				start, _ := stmt.Span()
				lines[start.Line] = true
			}
			return true
		})
		for line := range lines {
			result[path] = append(result[path], line)
		}
		sort.Slice(result[path], func(i, j int) bool { return result[path][i] < result[path][j] })
	}
	data, err := json.MarshalIndent(result, "", "  ")
	if err != nil {
		panic(err)
	}
	fmt.Println(string(data))
}
