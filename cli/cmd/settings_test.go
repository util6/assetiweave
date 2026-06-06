package cmd

import (
	"context"
	"testing"

	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/schema"
)

func TestSettingsShowCallsAppSettingsGet(t *testing.T) {
	client := &recordingClient{}
	root := Build(context.Background(), testPluginFactory(client))
	root.SetArgs([]string{"settings", "show"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != schema.MethodSettingsGet {
		t.Fatalf("method = %q, want %q", client.method, schema.MethodSettingsGet)
	}
	if params, ok := client.params.(map[string]any); !ok || len(params) != 0 {
		t.Fatalf("params = %#v, want empty object", client.params)
	}
}

func TestSettingsSaveParsesJSONObject(t *testing.T) {
	client := &recordingClient{}
	root := Build(context.Background(), testPluginFactory(client))
	root.SetArgs([]string{"settings", "save", "--json", `{"density":"compact"}`})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != schema.MethodSettingsSave {
		t.Fatalf("method = %q, want %q", client.method, schema.MethodSettingsSave)
	}
	params, ok := client.params.(map[string]any)
	if !ok {
		t.Fatalf("params = %T, want map", client.params)
	}
	settings, ok := params["settings"].(map[string]any)
	if !ok || settings["density"] != "compact" {
		t.Fatalf("settings params = %#v", params["settings"])
	}
}

func TestSettingsSaveRejectsInvalidJSONBeforeEngine(t *testing.T) {
	client := &recordingClient{}
	root := Build(context.Background(), testPluginFactory(client))
	root.SetArgs([]string{"settings", "save", "--json", `{`})

	err := root.Execute()

	assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeInvalidJSON)
	if client.method != "" {
		t.Fatalf("engine was called with %q", client.method)
	}
}

func TestSettingsShowRejectsExtraArguments(t *testing.T) {
	client := &recordingClient{}
	root := Build(context.Background(), testPluginFactory(client))
	root.SetArgs([]string{"settings", "show", "extra"})

	err := root.Execute()

	assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeInvalidArgument)
	if client.method != "" {
		t.Fatalf("engine was called with %q", client.method)
	}
}
