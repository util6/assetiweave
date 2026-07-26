package cmd

import (
	"os"
	"testing"

	"github.com/util6/assetiweave/internal/schema"
)

func TestMemoryRecallDefaultsToEvidenceOnlyAndUsesCurrentProject(t *testing.T) {
	wd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	client := &recordingClient{}
	err = executeSkillGroupTestCommand(t, client, "memory", "recall", "run", "--query", "why AppService?", "--current-project", "--format", "compact-json")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != schema.MethodMemoryRecallRun {
		t.Fatalf("method = %q", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["mode"] != "exact" || params["synthesize"] != false {
		t.Fatalf("params = %#v", params)
	}
	scope := params["scope"].(map[string]any)
	if scope["project_path"] != wd {
		t.Fatalf("project_path = %#v", scope["project_path"])
	}
}

func TestMemoryFullOrganizeEnablesAIOnlyWithFlag(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "memory", "recall", "run", "--full", "--project", "/tmp/project", "--ai", "--include-unavailable")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params := recordedSkillGroupParams(t, client)
	if params["mode"] != "full" || params["synthesize"] != true || params["include_unavailable"] != true {
		t.Fatalf("params = %#v", params)
	}
}

func TestMemoryRecallPreviewRejectsMissingExactQueryBeforeEngine(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "memory", "recall", "preview")
	if err == nil {
		t.Fatal("expected validation error")
	}
	if client.method != "" {
		t.Fatalf("engine called with %q", client.method)
	}
}

func TestMemoryItemAndCandidateCommandsUseEngineMethods(t *testing.T) {
	for _, test := range []struct {
		args   []string
		method string
	}{
		{[]string{"memory", "item", "list", "--status", "candidate"}, schema.MethodMemoryItemList},
		{[]string{"memory", "item", "get", "item-1"}, schema.MethodMemoryItemGet},
		{[]string{"memory", "candidate", "accept", "item-1"}, schema.MethodMemoryCandidateAccept},
		{[]string{"memory", "candidate", "reject", "item-1"}, schema.MethodMemoryCandidateReject},
		{[]string{"memory", "verify", "--item", "item-1"}, schema.MethodMemoryVerify},
	} {
		client := &recordingClient{}
		if err := executeSkillGroupTestCommand(t, client, test.args...); err != nil {
			t.Fatalf("%v: %v", test.args, err)
		}
		if client.method != test.method {
			t.Fatalf("%v method = %q, want %q", test.args, client.method, test.method)
		}
	}
}
