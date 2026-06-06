package errs

import (
	"go/ast"
	"go/parser"
	"go/token"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestDeclaredSubtypesIncludesEverySubtypeConstant(t *testing.T) {
	constants := loadSubtypeConstants(t)
	declared := map[Subtype]bool{}
	for _, subtype := range DeclaredSubtypes() {
		if declared[subtype] {
			t.Fatalf("DeclaredSubtypes contains duplicate %q", subtype)
		}
		declared[subtype] = true
	}

	for name, value := range constants {
		if !declared[value] {
			t.Fatalf("%s = %q is missing from DeclaredSubtypes", name, value)
		}
	}
}

func TestTypedConstructorsRejectUndeclaredSubtype(t *testing.T) {
	assertPanic(t, func() {
		_ = NewValidationError(Subtype("not_declared"), "bad subtype")
	})
	assertPanic(t, func() {
		_ = NewConfigError(Subtype("not_declared"), "bad subtype")
	})
	assertPanic(t, func() {
		_ = NewEngineError(Subtype("not_declared"), "bad subtype")
	})
	assertPanic(t, func() {
		_ = NewPolicyError(Subtype("not_declared"), "bad subtype")
	})
}

func loadSubtypeConstants(t *testing.T) map[string]Subtype {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("resolve test file")
	}
	subtypesFile := filepath.Join(filepath.Dir(file), "subtypes.go")
	fset := token.NewFileSet()
	parsed, err := parser.ParseFile(fset, subtypesFile, nil, 0)
	if err != nil {
		t.Fatalf("parse subtypes.go: %v", err)
	}

	constants := map[string]Subtype{}
	ast.Inspect(parsed, func(node ast.Node) bool {
		decl, ok := node.(*ast.ValueSpec)
		if !ok || len(decl.Names) == 0 {
			return true
		}
		if !isSubtypeValueSpec(decl) {
			return true
		}
		for i, name := range decl.Names {
			if !strings.HasPrefix(name.Name, "Subtype") || name.Name == "Subtype" {
				continue
			}
			if i >= len(decl.Values) {
				t.Fatalf("%s must explicitly declare its subtype value", name.Name)
			}
			literal, ok := decl.Values[i].(*ast.BasicLit)
			if !ok || literal.Kind != token.STRING {
				t.Fatalf("%s must be a string literal", name.Name)
			}
			constants[name.Name] = Subtype(strings.Trim(literal.Value, `"`))
		}
		return true
	})
	if len(constants) == 0 {
		t.Fatal("no Subtype constants found")
	}
	return constants
}

func isSubtypeValueSpec(spec *ast.ValueSpec) bool {
	ident, ok := spec.Type.(*ast.Ident)
	return ok && ident.Name == "Subtype"
}

func assertPanic(t *testing.T, fn func()) {
	t.Helper()
	defer func() {
		if recovered := recover(); recovered == nil {
			t.Fatal("function did not panic")
		}
	}()
	fn()
}
