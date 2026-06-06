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

const outputImportPath = "github.com/util6/assetiweave/internal/output"

func FindLegacyExitErrorUsage(path string, src []byte) ([]Violation, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, path, src, 0)
	if err != nil {
		return nil, err
	}
	names, dotImported := resolveOutputNames(file)
	var violations []Violation
	emit := func(pos token.Pos, symbol string) {
		position := fset.Position(pos)
		violations = append(violations, Violation{
			Rule:     "legacy_exit_error",
			Symbol:   symbol,
			File:     path,
			Line:     position.Line,
			Position: fmt.Sprintf("%d:%d", position.Line, position.Column),
			Message:  fmt.Sprintf("%s is a legacy error constructor; use typed errs.* errors for new code", symbol),
		})
	}
	ast.Inspect(file, func(node ast.Node) bool {
		switch expr := node.(type) {
		case *ast.CallExpr:
			if symbol, ok := legacyOutputHelper(expr.Fun, names, dotImported); ok {
				emit(expr.Pos(), symbol)
			}
		case *ast.CompositeLit:
			if symbol, ok := legacyExitErrorLiteral(expr.Type, names, dotImported); ok {
				emit(expr.Pos(), symbol)
			}
		}
		return true
	})
	return violations, nil
}

func ScanRepoForLegacyExitErrorUsage(root string) ([]Violation, error) {
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
		fileViolations, err := FindLegacyExitErrorUsage(filepath.ToSlash(rel), src)
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

func resolveOutputNames(file *ast.File) (map[string]struct{}, bool) {
	names := map[string]struct{}{}
	dotImported := false
	for _, imp := range file.Imports {
		if imp.Path == nil {
			continue
		}
		path := strings.Trim(imp.Path.Value, "`\"")
		if path != outputImportPath {
			continue
		}
		switch {
		case imp.Name == nil:
			names["output"] = struct{}{}
		case imp.Name.Name == ".":
			dotImported = true
		case imp.Name.Name == "_":
		default:
			names[imp.Name.Name] = struct{}{}
		}
	}
	return names, dotImported
}

func legacyOutputHelper(fun ast.Expr, outputNames map[string]struct{}, dotImported bool) (string, bool) {
	switch expr := fun.(type) {
	case *ast.SelectorExpr:
		x, ok := expr.X.(*ast.Ident)
		if !ok || expr.Sel == nil {
			return "", false
		}
		if _, ok := outputNames[x.Name]; !ok {
			return "", false
		}
		switch expr.Sel.Name {
		case "Errorf", "ErrWithHint":
			return "output." + expr.Sel.Name, true
		default:
			return "", false
		}
	case *ast.Ident:
		if !dotImported {
			return "", false
		}
		switch expr.Name {
		case "Errorf", "ErrWithHint":
			return "output." + expr.Name, true
		default:
			return "", false
		}
	default:
		return "", false
	}
}

func legacyExitErrorLiteral(expr ast.Expr, outputNames map[string]struct{}, dotImported bool) (string, bool) {
	switch typ := expr.(type) {
	case *ast.SelectorExpr:
		x, ok := typ.X.(*ast.Ident)
		if !ok || typ.Sel == nil {
			return "", false
		}
		if _, ok := outputNames[x.Name]; !ok {
			return "", false
		}
		if typ.Sel.Name == "ExitError" {
			return "output.ExitError", true
		}
	case *ast.Ident:
		if dotImported && typ.Name == "ExitError" {
			return "output.ExitError", true
		}
	}
	return "", false
}
