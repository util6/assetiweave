package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	engineclient "github.com/util6/assetiweave/internal/client"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
	"github.com/util6/assetiweave/internal/update"
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
	var envelope output.Envelope
	if err := json.Unmarshal(stdout.Bytes(), &envelope); err != nil {
		t.Fatalf("stdout is not JSON: %v\n%s", err, stdout.String())
	}
	data, ok := envelope.Data.(map[string]any)
	if !ok {
		t.Fatalf("version data = %#v, want object", envelope.Data)
	}
	release, ok := data["cli_release"].(map[string]any)
	if !ok ||
		release["version"] != data["cli_version"] ||
		release["source"] == "" ||
		release["channel"] == "" {
		t.Fatalf("cli_release is incomplete: %#v", release)
	}
}

func TestVersionCanCheckRemoteUpdatesWithoutBlockingCompatibilityReport(t *testing.T) {
	stdout := &bytes.Buffer{}
	previousVersion := protocol.CLIVersion
	protocol.CLIVersion = "0.1.1"
	t.Cleanup(func() { protocol.CLIVersion = previousVersion })
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"version":"99.0.0"}`))
	}))
	t.Cleanup(server.Close)
	t.Setenv("ASSETIWEAVE_UPDATE_MANIFEST_URL", server.URL)
	t.Setenv("ASSETIWEAVE_UPDATE_STATE_PATH", t.TempDir()+"/update-state.json")
	client := &recordingClient{
		data: json.RawMessage(`{
			"product": "AssetIWeave",
			"engine_version": "0.1.1",
			"protocol_version": 1,
			"contract_version": 2,
			"capabilities": []
		}`),
	}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"version", "--check-updates"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}

	var envelope output.Envelope
	if err := json.Unmarshal(stdout.Bytes(), &envelope); err != nil {
		t.Fatalf("stdout is not JSON: %v\n%s", err, stdout.String())
	}
	data, ok := envelope.Data.(map[string]any)
	if !ok {
		t.Fatalf("version data = %#v, want object", envelope.Data)
	}
	update, ok := data["update"].(map[string]any)
	if !ok ||
		update["checked"] != true ||
		update["available"] != true ||
		update["latest"] != "99.0.0" {
		t.Fatalf("update diagnostics = %#v", update)
	}
}

func TestComposePendingNoticeIncludesCachedUpdate(t *testing.T) {
	update.SetPending(&update.Info{Current: "0.1.1", Latest: "0.2.0"})
	t.Cleanup(func() { update.SetPending(nil) })

	notice := composePendingNotice()

	updateNotice, ok := notice["update"].(map[string]any)
	if !ok {
		t.Fatalf("notice missing update: %#v", notice)
	}
	if updateNotice["current"] != "0.1.1" ||
		updateNotice["latest"] != "0.2.0" ||
		updateNotice["message"] == "" ||
		updateNotice["command"] == "" {
		t.Fatalf("update notice = %#v", updateNotice)
	}
}

func TestCompletionCommandDetectionIncludesCobraHiddenRequests(t *testing.T) {
	if !isCompletionCommandArgs([]string{"__completeNoDesc", "source", ""}) {
		t.Fatal("__completeNoDesc was not detected as completion")
	}
	if !isCompletionCommandArgs([]string{"completion", "zsh"}) {
		t.Fatal("completion command was not detected")
	}
	if isCompletionCommandArgs([]string{"profile", "list"}) {
		t.Fatal("normal command detected as completion")
	}
}

func TestProfileVisibilityEnv(t *testing.T) {
	t.Setenv(hideProfilesEnv, "")
	if shouldHideProfiles() {
		t.Fatal("profile command should be visible without env override")
	}
	t.Setenv(hideProfilesEnv, "1")
	if !shouldHideProfiles() {
		t.Fatal("profile command should be hidden when env override is set")
	}
}

func TestGlobalEngineFlagStoresBootstrapOverride(t *testing.T) {
	stdout := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client: &recordingClient{
			data: json.RawMessage(`{
				"product": "AssetIWeave",
				"engine_version": "0.1.1",
				"protocol_version": 1,
				"contract_version": 2,
				"capabilities": []
			}`),
		},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"--engine", "/tmp/custom-engine", "version"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if factory.EnginePath != "/tmp/custom-engine" {
		t.Fatalf("EnginePath = %q, want global flag override", factory.EnginePath)
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

func TestVersionRejectsInvalidDataWithTypedEngineError(t *testing.T) {
	stderr := &bytes.Buffer{}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: stderr},
		Client: fakeClient{
			data: json.RawMessage(`{`),
			meta: &protocol.EngineMeta{
				ProtocolVersion: protocol.Version,
				ContractVersion: protocol.ContractVersion,
				EngineVersion:   "0.1.1",
			},
		},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"version"})

	err := root.Execute()
	if err == nil {
		t.Fatal("Execute() error = nil, want engine protocol error")
	}
	code := handleError(factory, err)

	if code != output.ExitEngine {
		t.Fatalf("exit code = %d, want %d", code, output.ExitEngine)
	}
	var envelope output.ErrorEnvelope
	if decodeErr := json.Unmarshal(stderr.Bytes(), &envelope); decodeErr != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", decodeErr, stderr.String())
	}
	if envelope.Error.Type != "engine" ||
		envelope.Error.Subtype != "engine_protocol" ||
		envelope.Error.Code != "engine_protocol" ||
		envelope.Meta == nil {
		t.Fatalf("unexpected error envelope: %+v", envelope)
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

	if code != output.ExitInternal {
		t.Fatalf("exit code = %d, want %d", code, output.ExitInternal)
	}
	var envelope output.ErrorEnvelope
	if err := json.Unmarshal(stderr.Bytes(), &envelope); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr.String())
	}
	if envelope.Error.Type != "internal" ||
		envelope.Error.Subtype != "unknown" ||
		envelope.Error.Code != "internal" {
		t.Fatalf("unexpected internal envelope: %+v", envelope.Error)
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
	if !strings.Contains(stderr.String(), `"type": "confirmation"`) ||
		!strings.Contains(stderr.String(), `"subtype": "confirmation_required"`) {
		t.Fatalf("stderr missing typed confirmation error: %s", stderr.String())
	}
}
