package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
)

func TestHandleErrorWritesTypedErrorEnvelope(t *testing.T) {
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: stderr},
		Client: fakeClient{
			err: errs.NewConfigError(errs.SubtypeInvalidConfig, "invalid local config").
				WithCode("invalid_config").
				WithHint("fix the config file"),
		},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	err := root.Execute()
	if err == nil {
		t.Fatal("Execute() error = nil, want typed error")
	}
	code := handleError(factory, err)

	if code != output.ExitValidation {
		t.Fatalf("exit code = %d, want %d", code, output.ExitValidation)
	}
	var envelope output.ErrorEnvelope
	if decodeErr := json.Unmarshal(stderr.Bytes(), &envelope); decodeErr != nil {
		t.Fatalf("stderr is not JSON envelope: %v\n%s", decodeErr, stderr.String())
	}
	if envelope.Error.Type != "config" ||
		envelope.Error.Subtype != "invalid_config" ||
		envelope.Error.Code != "invalid_config" ||
		envelope.Error.Hint != "fix the config file" {
		t.Fatalf("unexpected typed envelope: %+v", envelope)
	}
}

func TestRequireArgReturnsTypedValidationError(t *testing.T) {
	_, err := requireArg(nil, "asset-id")
	if err == nil {
		t.Fatal("requireArg() error = nil")
	}
	var validationErr *errs.ValidationError
	if !errors.As(err, &validationErr) {
		t.Fatalf("error = %T, want *errs.ValidationError", err)
	}
	problem, ok := errs.ProblemOf(err)
	if !ok ||
		problem.Category != errs.CategoryValidation ||
		problem.Subtype != errs.SubtypeInvalidArgument ||
		problem.Code != "validation" {
		t.Fatalf("problem = %+v", problem)
	}
}

func TestCompletionRejectsUnknownShellWithTypedValidation(t *testing.T) {
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: stderr},
		Client:    &recordingClient{},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"completion", "unknown"})

	err := root.Execute()
	if err == nil {
		t.Fatal("Execute() error = nil, want validation error")
	}
	code := handleError(factory, err)
	if code != output.ExitValidation {
		t.Fatalf("exit code = %d, want %d", code, output.ExitValidation)
	}
	var envelope output.ErrorEnvelope
	if decodeErr := json.Unmarshal(stderr.Bytes(), &envelope); decodeErr != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", decodeErr, stderr.String())
	}
	if envelope.Error.Type != "validation" ||
		envelope.Error.Subtype != "invalid_argument" ||
		envelope.Error.Code != "validation" {
		t.Fatalf("unexpected error envelope: %+v", envelope.Error)
	}
}

func assertTypedProblem(t *testing.T, err error, category errs.Category, subtype errs.Subtype) *errs.Problem {
	t.Helper()
	if err == nil {
		t.Fatalf("error = nil, want %s.%s", category, subtype)
	}
	problem, ok := errs.ProblemOf(err)
	if !ok {
		t.Fatalf("error = %T, want typed problem", err)
	}
	if problem.Category != category || problem.Subtype != subtype {
		t.Fatalf("problem = %+v, want %s.%s", problem, category, subtype)
	}
	return problem
}
