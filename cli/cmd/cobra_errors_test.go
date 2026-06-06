package cmd

import (
	"context"
	"strings"
	"testing"

	"github.com/util6/assetiweave/errs"
)

func TestUnknownNestedCommandReturnsTypedValidation(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	root.SetArgs([]string{"source", "lst"})

	err := root.Execute()

	problem := assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeUnknownCommand)
	details := problem.Details.(map[string]any)
	if details["unknown"] != "lst" || details["command_path"] != "assetiweave-cli source" {
		t.Fatalf("details = %#v", details)
	}
	if !containsString(details["suggestions"], "list") {
		t.Fatalf("suggestions = %#v, want list", details["suggestions"])
	}
}

func TestUnknownRootCommandReturnsTypedValidation(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	root.SetArgs([]string{"sorce", "list"})

	err := root.Execute()

	problem := assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeUnknownCommand)
	details := problem.Details.(map[string]any)
	if details["unknown"] != "sorce" || !containsString(details["suggestions"], "source") {
		t.Fatalf("details = %#v", details)
	}
}

func TestUnknownFlagReturnsTypedSuggestion(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	root.SetArgs([]string{"source", "add", "--name", "demo", "--path", "/tmp", "--dry-rnu"})

	err := root.Execute()

	problem := assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeUnknownFlag)
	details := problem.Details.(map[string]any)
	if details["unknown"] != "--dry-rnu" || !containsString(details["suggestions"], "--dry-run") {
		t.Fatalf("details = %#v", details)
	}
}

func TestInvalidFlagValueReturnsTypedValidation(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	root.SetArgs([]string{"source", "add", "--name", "demo", "--path", "/tmp", "--priority", "many"})

	err := root.Execute()

	problem := assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeFlagError)
	if !strings.Contains(problem.Message, "invalid argument") ||
		!strings.Contains(problem.Hint, "--help") {
		t.Fatalf("problem = %#v", problem)
	}
}

func TestSubcommandFlagOnBareGroupReturnsMissingSubcommand(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	root.SetArgs([]string{"source", "--dry-run"})

	err := root.Execute()

	problem := assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeMissingSubcommand)
	details := problem.Details.(map[string]any)
	if details["flag"] != "--dry-run" || details["command_path"] != "assetiweave-cli source" {
		t.Fatalf("details = %#v", details)
	}
	if !containsString(details["subcommands"], "add") ||
		!containsString(details["subcommands"], "remove") ||
		!containsString(details["subcommands"], "scan") {
		t.Fatalf("subcommands = %#v", details["subcommands"])
	}
}

func TestMissingRequiredFlagReturnsTypedValidation(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	root.SetArgs([]string{"source", "add", "--name", "demo"})

	err := root.Execute()

	problem := assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeMissingRequiredFlag)
	details := problem.Details.(map[string]any)
	if !containsString(details["flags"], "path") {
		t.Fatalf("details = %#v", details)
	}
}

func TestPositionalArgumentFailureReturnsTypedValidation(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	root.SetArgs([]string{"source", "remove"})

	err := root.Execute()

	problem := assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeInvalidArgument)
	details := problem.Details.(map[string]any)
	if details["command_path"] != "assetiweave-cli source remove" ||
		!strings.Contains(problem.Hint, "--help") {
		t.Fatalf("problem = %#v", problem)
	}
}

func TestBareCommandGroupStillShowsHelp(t *testing.T) {
	root := Build(context.Background(), testPluginFactory(&recordingClient{}))
	root.SetArgs([]string{"source"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
}

func containsString(value any, want string) bool {
	values, ok := value.([]string)
	if !ok {
		return false
	}
	for _, current := range values {
		if current == want {
			return true
		}
	}
	return false
}
