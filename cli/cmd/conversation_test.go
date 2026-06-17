package cmd

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"reflect"
	"strings"
	"testing"

	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/output"
)

func TestConversationSearchBuildsMemorySearchParams(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"conversation", "search",
		"--query", "backend architecture",
		"--record-kind", "session",
		"--adapter", "codex",
		"--source", "codex-live",
		"--project", "/Users/util6/code-space/assetiweave",
		"--type", "question",
		"--card-type", "answer",
		"--since", "2026-01-01",
		"--until", "2026-06-01T00:00:00Z",
		"--timeline",
		"--limit", "25",
		"--offset", "10",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "conversation.search" {
		t.Fatalf("method = %q, want conversation.search", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["query"] != "backend architecture" ||
		params["record_kind"] != "session" ||
		params["adapter_id"] != "codex" ||
		params["source_id"] != "codex-live" ||
		params["project_path"] != "/Users/util6/code-space/assetiweave" ||
		params["since"] != "2026-01-01" ||
		params["until"] != "2026-06-01T00:00:00Z" ||
		params["timeline"] != true ||
		params["limit"] != 25 ||
		params["offset"] != 10 {
		t.Fatalf("params = %#v", params)
	}
	if !reflect.DeepEqual(params["content_types"], []string{"question", "answer"}) {
		t.Fatalf("content_types = %#v", params["content_types"])
	}
}

func TestConversationSearchCanUseCurrentProject(t *testing.T) {
	wd, err := os.Getwd()
	if err != nil {
		t.Fatalf("Getwd() error = %v", err)
	}

	client := &recordingClient{}
	err = executeSkillGroupTestCommand(t, client,
		"conversation", "search",
		"--query", "frontend changes",
		"--current-project",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params := recordedSkillGroupParams(t, client)
	if params["project_path"] != wd {
		t.Fatalf("project_path = %#v, want %q", params["project_path"], wd)
	}
}

func TestConversationSearchWritesMarkdownForAIContext(t *testing.T) {
	stdout, client := executeConversationSearchOutputCommand(t,
		conversationSearchFixtureData(),
		"conversation", "search",
		"--query", "frontend",
		"--project", "/Users/util6/code-space/assetiweave",
		"--format", "markdown",
	)

	if client.method != "conversation.search" {
		t.Fatalf("method = %q, want conversation.search", client.method)
	}
	output := stdout.String()
	for _, want := range []string{
		"# Conversation Search Evidence",
		"## Search Scope",
		"/Users/util6/code-space/assetiweave",
		"p-1-answer",
		"frontend style preference",
	} {
		if !strings.Contains(output, want) {
			t.Fatalf("markdown output missing %q:\n%s", want, output)
		}
	}
}

func TestConversationSearchWritesPromptForAIContext(t *testing.T) {
	stdout, _ := executeConversationSearchOutputCommand(t,
		conversationSearchFixtureData(),
		"conversation", "search",
		"--query", "frontend",
		"--format", "prompt",
	)

	output := stdout.String()
	for _, want := range []string{
		"# Prompt",
		"Use only the search evidence below",
		"Infer topics, preferences, and constraints yourself",
		"# Conversation Search Evidence",
	} {
		if !strings.Contains(output, want) {
			t.Fatalf("prompt output missing %q:\n%s", want, output)
		}
	}
}

func TestConversationSearchWritesCompactJSONForAIContext(t *testing.T) {
	stdout, _ := executeConversationSearchOutputCommand(t,
		conversationSearchFixtureData(),
		"conversation", "search",
		"--query", "frontend",
		"--format", "compact-json",
	)

	var envelope output.Envelope
	if err := json.Unmarshal(stdout.Bytes(), &envelope); err != nil {
		t.Fatalf("stdout is not JSON: %v\n%s", err, stdout.String())
	}
	data, ok := envelope.Data.(map[string]any)
	if !ok {
		t.Fatalf("data = %#v, want object", envelope.Data)
	}
	hits, ok := data["hits"].([]any)
	if !ok || len(hits) != 1 {
		t.Fatalf("hits = %#v, want one compact hit", data["hits"])
	}
	hit, ok := hits[0].(map[string]any)
	if !ok || hit["session_id"] != "session-1" || hit["block_id"] != "p-1-answer" {
		t.Fatalf("compact hit = %#v", hits[0])
	}
}

func executeConversationSearchOutputCommand(t *testing.T, data json.RawMessage, args ...string) (*bytes.Buffer, *recordingClient) {
	t.Helper()
	stdout := &bytes.Buffer{}
	client := &recordingClient{data: data}
	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: stdout, ErrOut: &bytes.Buffer{}},
		Client:    client,
	}
	root := Build(context.Background(), factory)
	root.SetArgs(args)
	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	return stdout, client
}

func conversationSearchFixtureData() json.RawMessage {
	return json.RawMessage(`{
		"query": "frontend",
		"record_kind": "session",
		"scope": {
			"record_kind": "session",
			"adapter_id": null,
			"source_id": null,
			"project_path": "/Users/util6/code-space/assetiweave",
			"query": "frontend",
			"content_types": ["answer"],
			"since": null,
			"until": null,
			"timeline": false,
			"limit": 50,
			"offset": 0
		},
		"total_count": 1,
		"hits": [
			{
				"session": {
					"question_count": 1,
					"turn_count": 1,
					"id": "session-1",
					"source_id": "codex-live",
					"adapter_id": "codex",
					"external_id": "external-session-1",
					"title": "Frontend style notes",
					"project_path": "/Users/util6/code-space/assetiweave",
					"started_at": "2026-06-01T10:00:00Z",
					"updated_at": "2026-06-01T10:30:00Z",
					"source_locator": null,
					"source_fingerprint": null,
					"missing": false,
					"created_at": "2026-06-01T10:31:00Z",
					"imported_at": "2026-06-01T10:31:00Z"
				},
				"question_id": "question-1",
				"question_index": 0,
				"question_title": "UI preference",
				"turn_id": "turn-1",
				"part_id": "p-1",
				"block_id": "p-1-answer",
				"card_type": "answer",
				"snippet": "The user described a frontend style preference.",
				"score": 100
			}
		]
	}`)
}
