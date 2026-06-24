package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"

	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
	"github.com/util6/assetiweave/internal/protocol"
	"github.com/util6/assetiweave/internal/update"
)

func TestUpdateCheckReportsAppManagedRelease(t *testing.T) {
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
		data["action"] != "app_update_required" ||
		data["release_url"] == "" ||
		data["package_url"] != nil ||
		data["checksum_url"] != nil {
		t.Fatalf("update check result = %#v", data)
	}
}

func TestUpdateWithoutCheckReportsAppManagedReleaseWithoutReplacingTools(t *testing.T) {
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
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	var envelope output.Envelope
	if err := json.Unmarshal(stdout.Bytes(), &envelope); err != nil {
		t.Fatalf("stdout is not JSON: %v\n%s", err, stdout.String())
	}
	data, ok := envelope.Data.(map[string]any)
	if !ok || data["action"] != "app_update_required" {
		t.Fatalf("update data = %#v", envelope.Data)
	}
}
