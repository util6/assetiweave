package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"

	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
	"github.com/util6/assetiweave/internal/update"
)

func TestUpdateCheckReportsLatestReleasePackage(t *testing.T) {
	stdout := &bytes.Buffer{}
	previousVersion := protocol.CLIVersion
	protocol.CLIVersion = "0.1.1"
	t.Cleanup(func() { protocol.CLIVersion = previousVersion })
	t.Setenv(update.UpdateStatePathEnv, filepath.Join(t.TempDir(), "update-state.json"))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"version":"99.0.0"}`))
	}))
	t.Cleanup(server.Close)
	t.Setenv(update.ManifestURLEnv, server.URL)
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"update", "--check"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}

	var envelope output.Envelope
	if err := json.Unmarshal(stdout.Bytes(), &envelope); err != nil {
		t.Fatalf("stdout is not JSON: %v\n%s", err, stdout.String())
	}
	data, ok := envelope.Data.(map[string]any)
	if !ok {
		t.Fatalf("update data = %#v, want object", envelope.Data)
	}
	if data["checked"] != true ||
		data["update_available"] != true ||
		data["latest"] != "99.0.0" ||
		data["action"] != "update_available" ||
		data["release_url"] == "" ||
		data["package_url"] == "" ||
		data["checksum_url"] == "" {
		t.Fatalf("update check result = %#v", data)
	}
	if !strings.Contains(data["package_asset"].(string), "assetiweave-tools-v99.0.0-") {
		t.Fatalf("package_asset = %#v", data["package_asset"])
	}
	if !strings.HasSuffix(data["checksum_asset"].(string), ".sha256") {
		t.Fatalf("checksum_asset = %#v", data["checksum_asset"])
	}
}

func TestUpdateRequiresYesBeforeReplacingTools(t *testing.T) {
	stdout := &bytes.Buffer{}
	stderr := &bytes.Buffer{}
	previousVersion := protocol.CLIVersion
	protocol.CLIVersion = "0.1.1"
	t.Cleanup(func() { protocol.CLIVersion = previousVersion })
	t.Setenv(update.UpdateStatePathEnv, filepath.Join(t.TempDir(), "update-state.json"))
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"version":"99.0.0"}`))
	}))
	t.Cleanup(server.Close)
	t.Setenv(update.ManifestURLEnv, server.URL)
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: stderr},
		Client:    &recordingClient{},
	}
	root := Build(context.Background(), factory)
	root.SetArgs([]string{"update"})

	err := root.Execute()
	if err == nil {
		t.Fatal("Execute() error = nil, want confirmation error")
	}
	code := handleError(factory, err)

	if code != output.ExitConfirmationRequired {
		t.Fatalf("exit code = %d, want %d", code, output.ExitConfirmationRequired)
	}
	var envelope output.ErrorEnvelope
	if err := json.Unmarshal(stderr.Bytes(), &envelope); err != nil {
		t.Fatalf("stderr is not JSON: %v\n%s", err, stderr.String())
	}
	if envelope.Error.Subtype != "confirmation_required" ||
		envelope.Error.Code != "confirmation_required" {
		t.Fatalf("error = %+v", envelope.Error)
	}
}
