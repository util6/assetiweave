package errlint

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestFindRawSubtypeUsageDetectsCastsAndLiterals(t *testing.T) {
	src := []byte(`package sample

import "github.com/util6/assetiweave/errs"

var _ = errs.NewEngineError(errs.Subtype("raw_engine"), "x")
var _ = errs.NewConfigError(Subtype("raw_config"), "x")
var _ = errs.Problem{Subtype: "raw_problem", Message: "x"}
`)

	violations, err := FindRawSubtypeUsage("sample.go", src)
	if err != nil {
		t.Fatalf("FindRawSubtypeUsage() error = %v", err)
	}
	if len(violations) != 3 {
		t.Fatalf("violations = %+v, want 3", violations)
	}
	for _, violation := range violations {
		if violation.Rule != "raw_subtype" || violation.Line == 0 || !strings.Contains(violation.Message, "raw") {
			t.Fatalf("unexpected violation: %+v", violation)
		}
	}
}

func TestFindRawSubtypeUsageAcceptsDeclaredConstants(t *testing.T) {
	src := []byte(`package sample

import "github.com/util6/assetiweave/errs"

var _ = errs.NewEngineError(errs.SubtypeEngineProtocol, "x")
var _ = errs.Problem{Subtype: errs.SubtypeEngineProtocol, Message: "x"}
`)

	violations, err := FindRawSubtypeUsage("sample.go", src)
	if err != nil {
		t.Fatalf("FindRawSubtypeUsage() error = %v", err)
	}
	if len(violations) != 0 {
		t.Fatalf("violations = %+v, want none", violations)
	}
}

func TestRepoHasNoRawSubtypeUsageInProductionGo(t *testing.T) {
	root := repoRoot(t)
	violations, err := ScanRepoForRawSubtypeUsage(root)
	if err != nil {
		t.Fatalf("ScanRepoForRawSubtypeUsage() error = %v", err)
	}
	if len(violations) != 0 {
		t.Fatalf("raw subtype usage found:\n%s", formatViolations(violations))
	}
}

func repoRoot(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("resolve test file")
	}
	return filepath.Clean(filepath.Join(filepath.Dir(file), "..", ".."))
}

func formatViolations(violations []Violation) string {
	var builder strings.Builder
	for _, violation := range violations {
		builder.WriteString(violation.File)
		builder.WriteString(":")
		builder.WriteString(violation.Position)
		builder.WriteString(": ")
		builder.WriteString(violation.Message)
		builder.WriteString("\n")
	}
	return builder.String()
}

func TestScanRepoSkipsTestFiles(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "raw_test.go"), []byte(`package sample
var _ = Subtype("test_fixture")
`), 0o600); err != nil {
		t.Fatalf("write test file: %v", err)
	}

	violations, err := ScanRepoForRawSubtypeUsage(root)
	if err != nil {
		t.Fatalf("ScanRepoForRawSubtypeUsage() error = %v", err)
	}
	if len(violations) != 0 {
		t.Fatalf("violations = %+v, want none", violations)
	}
}
