package errlint

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"
)

type Violation struct {
	Rule     string
	Symbol   string
	File     string
	Line     int
	Position string
	Message  string
}

func FindRawSubtypeUsage(path string, src []byte) ([]Violation, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, path, src, 0)
	if err != nil {
		return nil, err
	}
	var violations []Violation
	emit := func(pos token.Pos, raw string) {
		position := fset.Position(pos)
		violations = append(violations, Violation{
			Rule:     "raw_subtype",
			File:     path,
			Line:     position.Line,
			Position: fmt.Sprintf("%d:%d", position.Line, position.Column),
			Message:  fmt.Sprintf("raw subtype %q must be declared as an errs.Subtype* constant", raw),
		})
	}
	ast.Inspect(file, func(node ast.Node) bool {
		switch expr := node.(type) {
		case *ast.CallExpr:
			if !isSubtypeCast(expr.Fun) || len(expr.Args) != 1 {
				return true
			}
			if raw, ok := stringLiteral(expr.Args[0]); ok {
				emit(expr.Pos(), raw)
			}
		case *ast.KeyValueExpr:
			key, ok := expr.Key.(*ast.Ident)
			if !ok || key.Name != "Subtype" {
				return true
			}
			if raw, ok := stringLiteral(expr.Value); ok {
				emit(expr.Value.Pos(), raw)
			}
		}
		return true
	})
	return violations, nil
}

func ScanRepoForRawSubtypeUsage(root string) ([]Violation, error) {
	var violations []Violation
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() {
			if shouldSkipDir(entry.Name()) {
				return filepath.SkipDir
			}
			return nil
		}
		if !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
			return nil
		}
		src, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			rel = path
		}
		fileViolations, err := FindRawSubtypeUsage(filepath.ToSlash(rel), src)
		if err != nil {
			return err
		}
		violations = append(violations, fileViolations...)
		return nil
	})
	if err != nil {
		return nil, err
	}
	return violations, nil
}

func isSubtypeCast(fun ast.Expr) bool {
	switch expr := fun.(type) {
	case *ast.Ident:
		return expr.Name == "Subtype"
	case *ast.SelectorExpr:
		return expr.Sel != nil && expr.Sel.Name == "Subtype"
	default:
		return false
	}
}

func stringLiteral(expr ast.Expr) (string, bool) {
	literal, ok := expr.(*ast.BasicLit)
	if !ok || literal.Kind != token.STRING {
		return "", false
	}
	return strings.Trim(literal.Value, `"`), true
}

func shouldSkipDir(name string) bool {
	switch name {
	case ".git", "node_modules", "target", "dist", "build", "vendor":
		return true
	default:
		return false
	}
}
