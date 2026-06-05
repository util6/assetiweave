package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"

	engineclient "github.com/util6/assetiweave/internal/client"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
)

type fakeClient struct {
	data json.RawMessage
	meta *protocol.EngineMeta
	err  error
}

func (f fakeClient) Call(context.Context, string, any) (engineclient.CallResult, error) {
	return engineclient.CallResult{Data: f.data, Meta: f.meta}, f.err
}

type recordingClient struct {
	method string
	params any
	data   json.RawMessage
	meta   *protocol.EngineMeta
}

func (r *recordingClient) Call(_ context.Context, method string, params any) (engineclient.CallResult, error) {
	r.method = method
	r.params = params
	if r.data != nil {
		return engineclient.CallResult{Data: r.data, Meta: r.meta}, nil
	}
	return engineclient.CallResult{Data: json.RawMessage(`{}`), Meta: r.meta}, nil
}

func TestOverviewWritesSuccessEnvelopeToStdout(t *testing.T) {
	stdout := &bytes.Buffer{}
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: stderr},
		Client: fakeClient{
			data: json.RawMessage(`{"source_count":1}`),
			meta: &protocol.EngineMeta{
				ProtocolVersion: 1,
				ContractVersion: 2,
				EngineVersion:   "0.1.1",
				Invocation: &protocol.InvocationMeta{
					Method:     "overview.get",
					Outcome:    "success",
					DurationMS: 1,
				},
			},
		},
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
	if !strings.Contains(stdout.String(), `"method": "overview.get"`) {
		t.Fatalf("stdout missing Engine invocation meta: %s", stdout.String())
	}
}

func TestVersionReportsCLIAndEngineCompatibility(t *testing.T) {
	stdout := &bytes.Buffer{}
	client := &recordingClient{
		data: json.RawMessage(`{
			"product": "AssetIWeave",
			"engine_version": "0.1.1",
			"protocol_version": 1,
			"contract_version": 2,
			"capabilities": ["command-contract-v1"]
		}`),
	}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"version"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "system.version" {
		t.Fatalf("method = %q, want system.version", client.method)
	}
	if !strings.Contains(stdout.String(), `"compatible": true`) ||
		!strings.Contains(stdout.String(), `"cli_version"`) ||
		!strings.Contains(stdout.String(), `"engine_version": "0.1.1"`) {
		t.Fatalf("stdout missing version compatibility: %s", stdout.String())
	}
}

func TestVersionReportsIncompatibleEngineWithoutBlockingDiagnostics(t *testing.T) {
	stdout := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client: fakeClient{data: json.RawMessage(`{
			"product": "AssetIWeave",
			"engine_version": "99.0.0",
			"protocol_version": 99,
			"contract_version": 2,
			"capabilities": []
		}`)},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"version"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if !strings.Contains(stdout.String(), `"compatible": false`) ||
		!strings.Contains(stdout.String(), `"engine_protocol_version": 99`) {
		t.Fatalf("stdout missing incompatibility diagnostics: %s", stdout.String())
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
	params, ok := client.params.(map[string]any)
	if !ok {
		t.Fatalf("params type = %T, want map[string]any", client.params)
	}
	if params["dry_run"] != true {
		t.Fatalf("params = %#v", params)
	}
}

func TestAPICallYesAddsExplicitConfirmation(t *testing.T) {
	client := &recordingClient{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"api", "call", "delete_source", "--json", `{"id":"source-id"}`, "--yes"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params, ok := client.params.(map[string]any)
	if !ok {
		t.Fatalf("params type = %T, want map[string]any", client.params)
	}
	if params["id"] != "source-id" || params["yes"] != true {
		t.Fatalf("params = %#v", params)
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
	if !strings.Contains(stderr.String(), `"type": "validation"`) ||
		!strings.Contains(stderr.String(), `"subtype": "invalid_json"`) ||
		!strings.Contains(stderr.String(), `"code": "invalid_json"`) {
		t.Fatalf("stderr missing typed invalid_json envelope: %s", stderr.String())
	}
}

func TestAPICallRejectsNonObjectJSON(t *testing.T) {
	client := &recordingClient{}
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: stderr},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"api", "call", "profile.list", "--json", `[]`})

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
	if !strings.Contains(stderr.String(), `"type": "validation"`) ||
		!strings.Contains(stderr.String(), `"subtype": "invalid_json"`) ||
		!strings.Contains(stderr.String(), `"code": "invalid_json"`) {
		t.Fatalf("stderr missing typed invalid_json envelope: %s", stderr.String())
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
	if code != output.ExitConfirmationRequired {
		t.Fatalf("exit code = %d, want %d", code, output.ExitConfirmationRequired)
	}
	if client.method != "" {
		t.Fatalf("engine was called with method %q", client.method)
	}
	if !strings.Contains(stderr.String(), `"type": "confirmation_required"`) {
		t.Fatalf("stderr missing confirmation error: %s", stderr.String())
	}
}
