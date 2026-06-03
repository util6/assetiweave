package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
)

type fakeClient struct {
	data json.RawMessage
	err  error
}

func (f fakeClient) Call(context.Context, string, any) (json.RawMessage, error) {
	return f.data, f.err
}

type recordingClient struct {
	method string
	params any
	data   json.RawMessage
}

func (r *recordingClient) Call(_ context.Context, method string, params any) (json.RawMessage, error) {
	r.method = method
	r.params = params
	if r.data != nil {
		return r.data, nil
	}
	return json.RawMessage(`{}`), nil
}

func TestOverviewWritesSuccessEnvelopeToStdout(t *testing.T) {
	stdout := &bytes.Buffer{}
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: stderr},
		Client:    fakeClient{data: json.RawMessage(`{"source_count":1}`)},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if stderr.Len() != 0 {
		t.Fatalf("stderr = %q, want empty", stderr.String())
	}
	if !strings.Contains(stdout.String(), `"ok": true`) || !strings.Contains(stdout.String(), `"source_count": 1`) {
		t.Fatalf("stdout missing success envelope: %s", stdout.String())
	}
}

func TestHandleErrorWritesErrorEnvelopeToStderr(t *testing.T) {
	stdout := &bytes.Buffer{}
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: stderr},
		Client: fakeClient{err: output.ErrWithHint(
			output.ExitValidation,
			"validation",
			"bad input",
			"fix it",
		)},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	err := root.Execute()
	if err == nil {
		t.Fatal("Execute() error = nil, want error")
	}
	code := handleError(factory, err)

	if code != output.ExitValidation {
		t.Fatalf("exit code = %d, want %d", code, output.ExitValidation)
	}
	if stdout.Len() != 0 {
		t.Fatalf("stdout = %q, want empty", stdout.String())
	}
	var envelope output.ErrorEnvelope
	if decodeErr := json.Unmarshal(stderr.Bytes(), &envelope); decodeErr != nil {
		t.Fatalf("stderr is not JSON envelope: %v\n%s", decodeErr, stderr.String())
	}
	if envelope.OK || envelope.Error.Type != "validation" || envelope.Error.Hint != "fix it" {
		t.Fatalf("unexpected error envelope: %+v", envelope)
	}
}

func TestHandleErrorWrapsUnstructuredErrors(t *testing.T) {
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: stderr},
		Client:    fakeClient{},
	}

	code := handleError(factory, errors.New("plain failure"))

	if code != 1 {
		t.Fatalf("exit code = %d, want 1", code)
	}
	if !strings.Contains(stderr.String(), `"type": "internal"`) {
		t.Fatalf("stderr missing internal envelope: %s", stderr.String())
	}
}

func TestSkillImportPassesDryRunToEngine(t *testing.T) {
	client := &recordingClient{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"skill", "import", "--from", "/tmp/skill", "--name", "demo", "--dry-run"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.import" {
		t.Fatalf("method = %q, want skill.import", client.method)
	}
	params, ok := client.params.(map[string]any)
	if !ok {
		t.Fatalf("params type = %T, want map[string]any", client.params)
	}
	if params["from"] != "/tmp/skill" || params["name"] != "demo" || params["dry_run"] != true {
		t.Fatalf("unexpected params: %#v", params)
	}
}

func TestSourceAddPassesDryRunToEngine(t *testing.T) {
	client := &recordingClient{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"source", "add", "--name", "LocalSkills", "--path", "/tmp/skills", "--dry-run"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "source.add" {
		t.Fatalf("method = %q, want source.add", client.method)
	}
	params, ok := client.params.(map[string]any)
	if !ok {
		t.Fatalf("params type = %T, want map[string]any", client.params)
	}
	if params["name"] != "LocalSkills" || params["root_path"] != "/tmp/skills" || params["dry_run"] != true {
		t.Fatalf("unexpected params: %#v", params)
	}
}

func TestAPICallPassesRawJSONParams(t *testing.T) {
	client := &recordingClient{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"api", "call", "profile.list", "--json", `{"dry_run":true}`})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "profile.list" {
		t.Fatalf("method = %q, want profile.list", client.method)
	}
	raw, ok := client.params.(json.RawMessage)
	if !ok {
		t.Fatalf("params type = %T, want json.RawMessage", client.params)
	}
	if string(raw) != `{"dry_run":true}` {
		t.Fatalf("params = %s", raw)
	}
}

func TestAPICallRejectsInvalidJSON(t *testing.T) {
	client := &recordingClient{}
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: stderr},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"api", "call", "profile.list", "--json", `{`})

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
	if !strings.Contains(stderr.String(), `"type": "invalid_json"`) {
		t.Fatalf("stderr missing invalid_json envelope: %s", stderr.String())
	}
}

func TestSkillDeleteRequiresYesBeforeCallingEngine(t *testing.T) {
	client := &recordingClient{}
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: stderr},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"skill", "delete", "demo"})

	err := root.Execute()
	if err == nil {
		t.Fatal("Execute() error = nil, want confirmation error")
	}
	code := handleError(factory, err)
	if code != output.ExitValidation {
		t.Fatalf("exit code = %d, want %d", code, output.ExitValidation)
	}
	if client.method != "" {
		t.Fatalf("engine was called with method %q", client.method)
	}
	if !strings.Contains(stderr.String(), `"type": "confirmation_required"`) {
		t.Fatalf("stderr missing confirmation error: %s", stderr.String())
	}
}
