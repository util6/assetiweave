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

func TestConversationSearchBuildsUnifiedKindSearchParams(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"conversation", "search",
		"--query", "backend architecture",
		"--record-kind", "session",
		"--adapter", "codex",
		"--source", "codex-live",
		"--project", "/Users/util6/code-space/assetiweave",
		"--kind", "question",
		"--kind", "answer",
		"--kind", "claude-code.reasoning",
		"--kind", "custom.trace",
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
	if !reflect.DeepEqual(params["content_types"], []string{"question"}) {
		t.Fatalf("content_types = %#v", params["content_types"])
	}
	if !reflect.DeepEqual(params["card_kinds"], []string{"claude-code.reasoning", "custom.trace"}) {
		t.Fatalf("card_kinds = %#v", params["card_kinds"])
	}
	if !reflect.DeepEqual(params["semantic_roles"], []string{"answer"}) {
		t.Fatalf("semantic_roles = %#v", params["semantic_roles"])
	}
}

func TestConversationAdapterUpgradeUsesDefaultWorkspace(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "c", "ad", "upgrade")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "conversation.adapter_package.upgrade_workspace" {
		t.Fatalf("method = %q, want conversation.adapter_package.upgrade_workspace", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["package_dir"] != nil || params["developer"] != false || params["dry_run"] != false || params["yes"] != true {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationAdapterUpgradeDryRunDoesNotConfirmPromotion(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "c", "ad", "upgrade", "--dry-run")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params := recordedSkillGroupParams(t, client)
	if params["dry_run"] != true || params["yes"] != false {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationAdapterUpgradeSupportsDeveloperWorkspace(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "c", "ad", "upgrade", "-d")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params := recordedSkillGroupParams(t, client)
	if params["package_dir"] != nil || params["developer"] != true {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationAdapterUpgradeSupportsExplicitDirectory(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "c", "ad", "upgrade", "/tmp/custom/codex")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params := recordedSkillGroupParams(t, client)
	if params["package_dir"] != "/tmp/custom/codex" || params["developer"] != false {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationAdapterUpgradeRejectsDeveloperFlagWithExplicitDirectory(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "c", "ad", "upgrade", "-d", "/tmp/custom/codex")
	if err == nil || !strings.Contains(err.Error(), "cannot be combined") {
		t.Fatalf("error = %v, want mutually exclusive input error", err)
	}
}

func TestConversationSearchRejectsUnqualifiedUnknownKind(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"conversation", "search",
		"--query", "memory",
		"--kind", "custom",
	)
	if err == nil || !strings.Contains(err.Error(), "unqualified --kind") {
		t.Fatalf("error = %v, want unqualified --kind validation error", err)
	}
}

func TestConversationSearchHidesLegacyCardFilterFlags(t *testing.T) {
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	})
	command, _, err := root.Find([]string{"conversation", "search"})
	if err != nil {
		t.Fatalf("Find() error = %v", err)
	}
	for _, name := range []string{"type", "card-type", "card-kind", "semantic-role"} {
		flag := command.Flags().Lookup(name)
		if flag == nil || !flag.Hidden {
			t.Fatalf("legacy flag %q = %#v, want hidden", name, flag)
		}
	}
}

func TestConversationSearchRetainsLegacyCardFilterAliases(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"conversation", "search",
		"--query", "memory",
		"--type", "question",
		"--semantic-role", "answer",
		"--card-kind", "adapter.decision",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params := recordedSkillGroupParams(t, client)
	if !reflect.DeepEqual(params["content_types"], []string{"question"}) ||
		!reflect.DeepEqual(params["semantic_roles"], []string{"answer"}) ||
		!reflect.DeepEqual(params["card_kinds"], []string{"adapter.decision"}) {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationSearchIndexCommandsUseEngineLifecycleMethods(t *testing.T) {
	for _, test := range []struct {
		name   string
		method string
	}{
		{name: "status", method: "conversation.search.index.status"},
		{name: "rebuild", method: "conversation.search.index.rebuild"},
	} {
		t.Run(test.name, func(t *testing.T) {
			client := &recordingClient{}
			err := executeSkillGroupTestCommand(
				t,
				client,
				"conversation",
				"search",
				"index",
				test.name,
			)
			if err != nil {
				t.Fatalf("Execute() error = %v", err)
			}
			if client.method != test.method {
				t.Fatalf("method = %q, want %q", client.method, test.method)
			}
		})
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

func TestConversationSyncBuildsRecordKindParams(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"conversation", "sync",
		"--adapter", "qwen-web",
		"--record-kind", "web",
		"--mode", "full",
		"--dry-run",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "conversation.sync" {
		t.Fatalf("method = %q, want conversation.sync", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["adapter_id"] != "qwen-web" ||
		params["record_kind"] != "web" ||
		params["mode"] != "full" ||
		params["dry_run"] != true {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationDataAuditBuildsScopeParams(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"conversation", "data", "audit",
		"--source", "codex-live",
		"--record-kind", "session",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "conversation.data.audit" {
		t.Fatalf("method = %q, want conversation.data.audit", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["source_id"] != "codex-live" || params["record_kind"] != "session" || params["include_resolved"] != false {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationDataRepairRequiresConfirmationUnlessDryRun(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "conversation", "data", "repair")
	if err == nil || !strings.Contains(err.Error(), "--yes") {
		t.Fatalf("error = %v, want confirmation error", err)
	}
	if client.method != "" {
		t.Fatalf("method = %q, want no Engine call", client.method)
	}

	err = executeSkillGroupTestCommand(t, client,
		"conversation", "data", "repair",
		"--source", "codex-live",
		"--dry-run",
		"--resync",
	)
	if err != nil {
		t.Fatalf("dry-run Execute() error = %v", err)
	}
	if client.method != "conversation.data.repair" {
		t.Fatalf("method = %q, want conversation.data.repair", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["source_id"] != "codex-live" || params["dry_run"] != true || params["resync"] != true || params["yes"] != false {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationDataRollbackRequiresBackupAndConfirmation(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "conversation", "data", "rollback")
	if err == nil || !strings.Contains(err.Error(), "--backup-path") {
		t.Fatalf("error = %v, want backup path error", err)
	}

	err = executeSkillGroupTestCommand(t, client,
		"conversation", "data", "rollback",
		"--backup-path", "/tmp/conversation.sqlite",
		"--dry-run",
	)
	if err != nil {
		t.Fatalf("dry-run Execute() error = %v", err)
	}
	if client.method != "conversation.data.rollback" {
		t.Fatalf("method = %q, want conversation.data.rollback", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["backup_path"] != "/tmp/conversation.sqlite" || params["dry_run"] != true || params["yes"] != false {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationIncrementalSearchBuildsRecentRunParams(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"conversation", "search", "incremental",
		"--query", "recent memory",
		"--record-kind", "session",
		"--recent-runs", "5",
		"--kind", "answer",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "conversation.search.incremental" {
		t.Fatalf("method = %q, want conversation.search.incremental", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["query"] != "recent memory" ||
		params["record_kind"] != "session" ||
		params["recent_runs"] != 5 {
		t.Fatalf("params = %#v", params)
	}
	semanticRoles, ok := params["semantic_roles"].([]string)
	if !ok || len(semanticRoles) != 1 || semanticRoles[0] != "answer" {
		t.Fatalf("semantic_roles = %#v", params["semantic_roles"])
	}
}

func TestConversationBlockCommandsUseOnlyExactLocators(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"conversation", "block", "list", "conversation-question-1234567890",
	)
	if err != nil {
		t.Fatalf("list Execute() error = %v", err)
	}
	if client.method != "conversation.block.list" {
		t.Fatalf("list method = %q", client.method)
	}
	if params := recordedSkillGroupParams(t, client); params["question_id"] != "conversation-question-1234567890" {
		t.Fatalf("list params = %#v", params)
	}

	client = &recordingClient{}
	err = executeSkillGroupTestCommand(t, client,
		"conversation", "block", "get", "conversation-part-1234567890",
	)
	if err != nil {
		t.Fatalf("get Execute() error = %v", err)
	}
	if client.method != "conversation.block.get" {
		t.Fatalf("get method = %q", client.method)
	}
	if params := recordedSkillGroupParams(t, client); params["block_id"] != "conversation-part-1234567890" {
		t.Fatalf("get params = %#v", params)
	}
}

func TestConversationAdapterCommandExposesFocusedRuntimeCommands(t *testing.T) {
	root := Build(context.Background(), &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
		Client:    &recordingClient{},
	})
	command, _, err := root.Find([]string{"conversation", "adapter"})
	if err != nil {
		t.Fatalf("Find() error = %v", err)
	}
	names := make([]string, 0, len(command.Commands()))
	for _, child := range command.Commands() {
		names = append(names, child.Name())
	}
	if !reflect.DeepEqual(names, []string{"inspect", "list", "upgrade"}) {
		t.Fatalf("conversation adapter commands = %#v, want inspect/list/upgrade", names)
	}
}

func TestConversationAdapterInspectUsesSharedPackageInspection(t *testing.T) {
	client := &recordingClient{data: json.RawMessage(`{
		"origin":"managed_release",
		"package":{"package_id":"io.github.util6.codex-session","adapter_id":"codex","version":"1.0.1","latest_version":"1.0.1","runtime_gate_status":"ready","install_dir":"/tmp/package","installed_content_hash":"content","trusted_package_hash":"trusted","error_message":null},
		"adapter":{"id":"codex","name":"Codex","version":"1.0.1"},
		"affected_sources":[]
	}`)}
	err := executeSkillGroupTestCommand(t, client,
		"conversation", "adapter", "inspect", "io.github.util6.codex-session", "--format", "json",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "conversation.adapter_package.inspect" {
		t.Fatalf("method = %q, want conversation.adapter_package.inspect", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["package_id"] != "io.github.util6.codex-session" || params["adapter_id"] != "io.github.util6.codex-session" {
		t.Fatalf("params = %#v", params)
	}
}

func TestConversationAdapterInspectionTableIncludesRuntimeDiagnostics(t *testing.T) {
	latest := "1.1.0"
	contentHash := "content-hash"
	trustedHash := "trusted-hash"
	buffer := &bytes.Buffer{}
	writeConversationAdapterInspection(buffer, conversationAdapterInspection{
		Origin: "managed_release",
		Package: &conversationAdapterPackage{
			PackageID: "io.github.util6.codex-session", AdapterID: "codex", Version: "1.0.1",
			LatestVersion: &latest, RuntimeGateStatus: "hash_mismatch", InstallDir: "/tmp/package",
			InstalledContentHash: &contentHash, TrustedPackageHash: &trustedHash,
		},
		AffectedSources: []conversationAdapterSource{{ID: "codex-live", Name: "Codex", Enabled: true}},
	})
	output := buffer.String()
	for _, expected := range []string{"io.github.util6.codex-session", "hash_mismatch", "/tmp/package", "content-hash", "codex-live"} {
		if !strings.Contains(output, expected) {
			t.Fatalf("output = %q, want %q", output, expected)
		}
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
		"Card kinds: `claude-code.reasoning`",
		"Semantic roles: `reasoning`",
		"Card kind facets: `claude-code.reasoning=1`",
		"Semantic role facets: `reasoning=1`",
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
	scope, ok := data["scope"].(map[string]any)
	if !ok || !reflect.DeepEqual(scope["card_kinds"], []any{"claude-code.reasoning"}) || !reflect.DeepEqual(scope["semantic_roles"], []any{"reasoning"}) {
		t.Fatalf("compact scope = %#v", data["scope"])
	}
	if !reflect.DeepEqual(data["content_type_counts"], map[string]any{"claude-code.reasoning": float64(1)}) || !reflect.DeepEqual(data["semantic_role_counts"], map[string]any{"reasoning": float64(1)}) {
		t.Fatalf("compact facets = %#v / %#v", data["content_type_counts"], data["semantic_role_counts"])
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
			"card_kinds": ["claude-code.reasoning"],
			"semantic_roles": ["reasoning"],
			"include_questions": true,
			"include_cards": true,
			"since": null,
			"until": null,
			"timeline": false,
			"limit": 50,
			"offset": 0
		},
		"total_count": 1,
		"content_type_counts": {"claude-code.reasoning": 1},
		"semantic_role_counts": {"reasoning": 1},
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
