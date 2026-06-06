package errlint

import (
	"reflect"
	"testing"
)

func TestFindLegacyExitErrorUsageDetectsHelpersAndLiterals(t *testing.T) {
	src := []byte(`package sample

import "github.com/util6/assetiweave/internal/output"

var _ = output.Errorf(output.ExitValidation, "validation", "bad")
var _ = output.ErrWithHint(output.ExitEngine, "engine", "bad", "fix")
var _ = &output.ExitError{}
var _ = output.ExitError{}
`)

	violations, err := FindLegacyExitErrorUsage("sample.go", src)
	if err != nil {
		t.Fatalf("FindLegacyExitErrorUsage() error = %v", err)
	}
	got := summarizeBySymbol(violations)
	want := map[string]int{
		"output.ErrWithHint": 1,
		"output.Errorf":      1,
		"output.ExitError":   2,
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("summary = %#v, want %#v", got, want)
	}
}

func TestRepoLegacyExitErrorUsageDoesNotExceedBaseline(t *testing.T) {
	root := repoRoot(t)
	violations, err := ScanRepoForLegacyExitErrorUsage(root)
	if err != nil {
		t.Fatalf("ScanRepoForLegacyExitErrorUsage() error = %v", err)
	}
	got := summarizeLegacyByFileAndSymbol(violations)
	for file, symbols := range got {
		allowedSymbols, ok := legacyExitErrorBaseline[file]
		if !ok {
			t.Fatalf("new legacy output error usage in %s:\n%s", file, formatViolations(filterViolationsByFile(violations, file)))
		}
		for symbol, count := range symbols {
			allowed := allowedSymbols[symbol]
			if count > allowed {
				t.Fatalf("%s %s count = %d, allowed %d\n%s", file, symbol, count, allowed, formatViolations(filterViolationsByFile(violations, file)))
			}
		}
	}
}

var legacyExitErrorBaseline = map[string]map[string]int{}

func summarizeBySymbol(violations []Violation) map[string]int {
	counts := map[string]int{}
	for _, violation := range violations {
		counts[violation.Symbol]++
	}
	return counts
}

func summarizeLegacyByFileAndSymbol(violations []Violation) map[string]map[string]int {
	counts := map[string]map[string]int{}
	for _, violation := range violations {
		if counts[violation.File] == nil {
			counts[violation.File] = map[string]int{}
		}
		counts[violation.File][violation.Symbol]++
	}
	return counts
}

func filterViolationsByFile(violations []Violation, file string) []Violation {
	var filtered []Violation
	for _, violation := range violations {
		if violation.File == file {
			filtered = append(filtered, violation)
		}
	}
	return filtered
}
