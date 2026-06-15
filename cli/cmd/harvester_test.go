package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
)

func TestHarvesterTemplateListCommand(t *testing.T) {
	stdout := &bytes.Buffer{}
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	})
	root.SetArgs([]string{"harvester", "template", "list", "--root", t.TempDir()})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	var envelope output.Envelope
	if err := json.Unmarshal(stdout.Bytes(), &envelope); err != nil {
		t.Fatalf("stdout is not JSON: %v\n%s", err, stdout.String())
	}
	data, ok := envelope.Data.(map[string]any)
	if !ok {
		t.Fatalf("data = %#v", envelope.Data)
	}
	templates, ok := data["templates"].([]any)
	if !ok || len(templates) < 2 {
		t.Fatalf("templates = %#v", data["templates"])
	}
}

func TestHarvesterInstallCommandWritesToRoot(t *testing.T) {
	stdout := &bytes.Buffer{}
	targetRoot := t.TempDir()
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	})
	root.SetArgs([]string{"harvester", "install", "qwen-web", "--root", targetRoot})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if _, err := os.Stat(filepath.Join(targetRoot, "qwen-web", "harvester.json")); err != nil {
		t.Fatalf("installed harvester manifest missing: %v", err)
	}
}

func TestHarvesterInstallFromDirectoryCommand(t *testing.T) {
	stdout := &bytes.Buffer{}
	targetRoot := t.TempDir()
	packageDir := writeCLIHarvesterPackage(t, t.TempDir(), "community-web")
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	})
	root.SetArgs([]string{"harvester", "install", "community-web", "--from", packageDir, "--root", targetRoot})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if _, err := os.Stat(filepath.Join(targetRoot, "community-web", "harvester.json")); err != nil {
		t.Fatalf("installed community package missing: %v", err)
	}
}

func TestHarvesterUpdateAllCommandWritesOfficialTemplates(t *testing.T) {
	stdout := &bytes.Buffer{}
	targetRoot := t.TempDir()
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	})
	root.SetArgs([]string{"harvester", "update", "--all", "--root", targetRoot})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	for _, id := range []string{"qwen-web", "gemini-web"} {
		if _, err := os.Stat(filepath.Join(targetRoot, id, "harvester.json")); err != nil {
			t.Fatalf("updated official template %s missing: %v", id, err)
		}
	}
}

func TestHarvesterUpdateAllRejectsExternalPackageSource(t *testing.T) {
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	})
	root.SetArgs([]string{"harvester", "update", "--all", "--from", t.TempDir()})

	if err := root.Execute(); err == nil {
		t.Fatal("Execute() error = nil, want --all and --from validation error")
	}
}

func TestHarvesterRunRequiresYes(t *testing.T) {
	stdout := &bytes.Buffer{}
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	})
	root.SetArgs([]string{"harvester", "run", "qwen-web", "--root", t.TempDir()})

	if err := root.Execute(); err == nil {
		t.Fatal("Execute() error = nil, want confirmation error")
	}
}

func writeCLIHarvesterPackage(t *testing.T, parent, id string) string {
	t.Helper()
	dir := filepath.Join(parent, id)
	if err := os.MkdirAll(filepath.Join(dir, "scripts"), 0o700); err != nil {
		t.Fatalf("mkdir package: %v", err)
	}
	manifest := `{
  "schema_version": 1,
  "id": "` + id + `",
  "name": "` + id + `",
  "version": "0.1.0",
  "origin": "community",
  "entrypoint": ["scripts/harvest.sh"],
  "output": {"normalized_dir": "output/normalized", "sessions_file": "sessions.json"},
  "adapter": {"manifest": "conversation-adapter.json"},
  "source": {"id": "` + id + `-export", "kind": "directory", "name": "` + id + ` Export"},
  "update": {"channel": "community"}
}`
	if err := os.WriteFile(filepath.Join(dir, "harvester.json"), []byte(manifest), 0o600); err != nil {
		t.Fatalf("write manifest: %v", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "scripts", "harvest.sh"), []byte("#!/bin/sh\n"), 0o700); err != nil {
		t.Fatalf("write script: %v", err)
	}
	return dir
}
