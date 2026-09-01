package cmd

import (
	"os"
	"testing"

	"github.com/util6/assetiweave/internal/schema"
)

func TestMemoryRecentListUsesEngineMethod(t *testing.T) {
	client := &recordingClient{}
	if err := executeSkillGroupTestCommand(t, client, "memory", "recent", "list"); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != schema.MethodMemoryRecentList {
		t.Fatalf("method = %q, want %q", client.method, schema.MethodMemoryRecentList)
	}
}

func TestMemoryContextResolveUsesCurrentProject(t *testing.T) {
	wd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	client := &recordingClient{}
	if err := executeSkillGroupTestCommand(t, client, "memory", "context", "resolve", "--current-project", "--query", "why AppService?"); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != schema.MethodMemoryContextResolve {
		t.Fatalf("method = %q, want %q", client.method, schema.MethodMemoryContextResolve)
	}
	params := recordedSkillGroupParams(t, client)
	if params["project_path"] != wd || params["query"] != "why AppService?" {
		t.Fatalf("params = %#v", params)
	}
}

func TestMemoryRecallSearchRequiresQueryBeforeEngine(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "memory", "recall", "search")
	if err == nil {
		t.Fatal("expected validation error")
	}
	if client.method != "" {
		t.Fatalf("engine called with %q", client.method)
	}
}

func TestMemoryRecallSessionAndTurnUseEngineMethods(t *testing.T) {
	for _, test := range []struct {
		args   []string
		method string
	}{
		{[]string{"memory", "recall", "session", "create"}, schema.MethodMemoryRecallSessionCreate},
		{[]string{"memory", "recall", "session", "get", "session-1"}, schema.MethodMemoryRecallSessionGet},
		{[]string{"memory", "recall", "turn", "send", "session-1", "--query", "why?"}, schema.MethodMemoryRecallTurnSend},
		{[]string{"memory", "recall", "turn", "cancel", "turn-1"}, schema.MethodMemoryRecallTurnCancel},
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
