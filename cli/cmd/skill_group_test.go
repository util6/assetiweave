package cmd

import (
	"bytes"
	"context"
	"reflect"
	"testing"

	"github.com/util6/assetiweave/internal/cmdutil"
)

func TestSkillGroupCreateBuildsInputFromFlags(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"skill", "group", "create",
		"--name", "Frontend",
		"--id", "frontend",
		"--description", "UI skills",
		"--color", "#0ea5e9",
		"--source", "source-a",
		"--path-glob", "frontend/**",
		"--name-contains", "ui",
		"--disabled",
		"--sort-order", "20",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.group.create" {
		t.Fatalf("method = %q, want skill.group.create", client.method)
	}

	params := recordedSkillGroupParams(t, client)
	input := recordedNestedMap(t, params, "input")
	if input["id"] != "frontend" ||
		input["name"] != "Frontend" ||
		input["description"] != "UI skills" ||
		input["color"] != "#0ea5e9" ||
		input["enabled"] != false ||
		input["sort_order"] != 20 {
		t.Fatalf("input = %#v", input)
	}
	rules := recordedNestedMap(t, input, "rules")
	if !reflect.DeepEqual(rules["source_ids"], []string{"source-a"}) ||
		!reflect.DeepEqual(rules["relative_path_globs"], []string{"frontend/**"}) ||
		rules["name_contains"] != "ui" {
		t.Fatalf("rules = %#v", rules)
	}
}

func TestSkillGroupUpdateReadsJSONAndInjectsPathID(t *testing.T) {
	client := &recordingClient{}
	groupJSON := `{"name":"Frontend","asset_kind":"skill","description":null,"color":"#10b981","enabled":true,"sort_order":0,"rules":{"source_ids":[],"relative_path_globs":[],"name_contains":null},"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}`
	err := executeSkillGroupTestCommand(t, client,
		"skill", "group", "update", "frontend",
		"--json", groupJSON,
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.group.update" {
		t.Fatalf("method = %q, want skill.group.update", client.method)
	}

	params := recordedSkillGroupParams(t, client)
	group := recordedNestedMap(t, params, "group")
	if group["id"] != "frontend" || group["name"] != "Frontend" {
		t.Fatalf("group = %#v", group)
	}
}

func TestSkillGroupDeleteRequiresYesBeforeCallingEngine(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "skill", "group", "delete", "frontend")
	if err == nil {
		t.Fatal("Execute() error = nil, want confirmation error")
	}
	if client.method != "" {
		t.Fatalf("engine was called with method %q", client.method)
	}

	err = executeSkillGroupTestCommand(t, client, "skill", "group", "delete", "frontend", "--yes")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.group.delete" {
		t.Fatalf("method = %q, want skill.group.delete", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["group_id"] != "frontend" || params["yes"] != true {
		t.Fatalf("params = %#v", params)
	}
}

func TestSkillGroupMembersSetAndClearBuildParams(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"skill", "group", "members", "set", "frontend",
		"--asset", "skill-a",
		"--asset", "skill-b",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.group.members.set" {
		t.Fatalf("method = %q, want skill.group.members.set", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["group_id"] != "frontend" || !reflect.DeepEqual(params["asset_ids"], []string{"skill-a", "skill-b"}) {
		t.Fatalf("params = %#v", params)
	}

	err = executeSkillGroupTestCommand(t, client,
		"skill", "group", "members", "set", "frontend",
		"--clear",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params = recordedSkillGroupParams(t, client)
	if params["group_id"] != "frontend" || !reflect.DeepEqual(params["asset_ids"], []string{}) {
		t.Fatalf("clear params = %#v", params)
	}

	err = executeSkillGroupTestCommand(t, client,
		"skill", "group", "members", "set", "frontend",
		"--asset", "skill-a",
		"--clear",
	)
	if err == nil {
		t.Fatal("Execute() error = nil, want validation error")
	}
}

func TestSkillGroupExclusivePreviewAndApplyBuildInput(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"skill", "group", "exclusive", "preview",
		"--group", "frontend",
		"--group", "browser",
		"--profile", "codex",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.group.exclusive.preview" {
		t.Fatalf("method = %q, want skill.group.exclusive.preview", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	input := recordedNestedMap(t, params, "input")
	if input["profile_id"] != "codex" ||
		input["mount_selected"] != true ||
		input["dry_run"] != true ||
		!reflect.DeepEqual(input["group_ids"], []string{"frontend", "browser"}) {
		t.Fatalf("preview input = %#v", input)
	}

	err = executeSkillGroupTestCommand(t, client,
		"skill", "group", "exclusive", "apply",
		"--group", "frontend",
		"--profile", "codex",
	)
	if err == nil {
		t.Fatal("Execute() error = nil, want confirmation error")
	}

	err = executeSkillGroupTestCommand(t, client,
		"skill", "group", "exclusive", "apply",
		"--group", "frontend",
		"--profile", "codex",
		"--yes",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.group.exclusive.apply" {
		t.Fatalf("method = %q, want skill.group.exclusive.apply", client.method)
	}
	params = recordedSkillGroupParams(t, client)
	input = recordedNestedMap(t, params, "input")
	if input["profile_id"] != "codex" ||
		input["mount_selected"] != true ||
		input["dry_run"] != false ||
		!reflect.DeepEqual(input["group_ids"], []string{"frontend"}) ||
		params["yes"] != true {
		t.Fatalf("apply params = %#v", params)
	}
}

func executeSkillGroupTestCommand(t *testing.T, client *recordingClient, args ...string) error {
	t.Helper()
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs(args)
	return root.Execute()
}

func recordedSkillGroupParams(t *testing.T, client *recordingClient) map[string]any {
	t.Helper()
	params, ok := client.params.(map[string]any)
	if !ok {
		t.Fatalf("params type = %T, want map[string]any", client.params)
	}
	return params
}

func recordedNestedMap(t *testing.T, parent map[string]any, key string) map[string]any {
	t.Helper()
	value, ok := parent[key].(map[string]any)
	if !ok {
		t.Fatalf("%s type = %T, want map[string]any", key, parent[key])
	}
	return value
}
