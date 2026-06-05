package cmd

import (
	"bytes"
	"context"
	"encoding/json"
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
