package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"testing"

	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/schema"
)

func TestGeneratedAppCommandTreeMatchesContract(t *testing.T) {
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	}
	app := newCmdApp(factory)

	if len(app.Commands()) != len(schema.AppCommands()) {
		t.Fatalf("generated app commands = %d, contract app commands = %d", len(app.Commands()), len(schema.AppCommands()))
	}
	for _, command := range schema.AppCommands() {
		if found, _, err := app.Find([]string{generatedCommandName(command.Method)}); err != nil || found == app {
			t.Fatalf("generated command missing for %q: %v", command.Method, err)
		}
	}
}

func TestGeneratedAppCommandParsesObjectParams(t *testing.T) {
	client := &recordingClient{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"app", "create-profile", "--input", `{"id":"demo","name":"Demo"}`})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "create_profile" {
		t.Fatalf("method = %q, want create_profile", client.method)
	}
	params, ok := client.params.(map[string]any)
	if !ok {
		t.Fatalf("params type = %T, want map[string]any", client.params)
	}
	input, ok := params["input"].(map[string]any)
	if !ok || input["id"] != "demo" {
		t.Fatalf("input params = %#v", params["input"])
	}
}

func TestGeneratedHighRiskAppCommandRequiresYes(t *testing.T) {
	client := &recordingClient{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"app", "delete-source", "--id", "source-id"})

	if err := root.Execute(); err == nil {
		t.Fatal("Execute() error = nil, want confirmation error")
	}
	if client.method != "" {
		t.Fatalf("engine was called with method %q", client.method)
	}
}

func TestGeneratedHighRiskAppCommandPassesYes(t *testing.T) {
	client := &recordingClient{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"app", "delete-source", "--id", "source-id", "--yes"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params, ok := client.params.(map[string]any)
	if !ok || params["id"] != "source-id" || params["yes"] != true {
		t.Fatalf("params = %#v", client.params)
	}
}

func TestGeneratedAppCommandRejectsInvalidJSONWithTypedValidation(t *testing.T) {
	client := &recordingClient{}
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: stderr},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"app", "create-profile", "--input", `{`})

	err := root.Execute()
	if err == nil {
		t.Fatal("Execute() error = nil, want validation error")
	}
	code := handleError(factory, err)
	if code != output.ExitValidation {
		t.Fatalf("exit code = %d, want %d", code, output.ExitValidation)
	}
	if client.method != "" {
		t.Fatalf("engine was called with method %q", client.method)
	}
	var envelope output.ErrorEnvelope
	if decodeErr := json.Unmarshal(stderr.Bytes(), &envelope); decodeErr != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", decodeErr, stderr.String())
	}
	if envelope.Error.Type != "validation" ||
		envelope.Error.Subtype != "invalid_json" ||
		envelope.Error.Code != "invalid_json" {
		t.Fatalf("unexpected error envelope: %+v", envelope.Error)
	}
}
