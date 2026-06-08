package cmd

import "testing"

func TestSkillSearchBuildsProviderParams(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"skill", "search",
		"--query", "browser testing",
		"--provider", "github",
		"--limit", "5",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.search" {
		t.Fatalf("method = %q, want skill.search", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["query"] != "browser testing" || params["provider"] != "github" || params["limit"] != 5 {
		t.Fatalf("params = %#v", params)
	}
}

func TestSkillAcquireDryRunAndConfirmation(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client,
		"skill", "acquire",
		"--url", "https://github.com/util6/util6-agents/tree/main/skills/browser",
		"--path", "skills/browser",
		"--name", "browser",
		"--dry-run",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.acquire" {
		t.Fatalf("method = %q, want skill.acquire", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if params["url"] != "https://github.com/util6/util6-agents/tree/main/skills/browser" ||
		params["path"] != "skills/browser" ||
		params["name"] != "browser" ||
		params["dry_run"] != true ||
		params["yes"] != false {
		t.Fatalf("dry-run params = %#v", params)
	}

	err = executeSkillGroupTestCommand(t, client,
		"skill", "acquire",
		"--url", "https://github.com/util6/util6-agents",
	)
	if err == nil {
		t.Fatal("Execute() error = nil, want confirmation error")
	}

	err = executeSkillGroupTestCommand(t, client,
		"skill", "acquire",
		"--url", "https://github.com/util6/util6-agents",
		"--branch", "main",
		"--yes",
	)
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params = recordedSkillGroupParams(t, client)
	if params["url"] != "https://github.com/util6/util6-agents" ||
		params["branch"] != "main" ||
		params["dry_run"] != false ||
		params["yes"] != true {
		t.Fatalf("apply params = %#v", params)
	}
}

func TestSkillRemoteCommandsBuildParams(t *testing.T) {
	client := &recordingClient{}
	err := executeSkillGroupTestCommand(t, client, "skill", "remote", "list")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.remote.list" {
		t.Fatalf("method = %q, want skill.remote.list", client.method)
	}
	params := recordedSkillGroupParams(t, client)
	if len(params) != 0 {
		t.Fatalf("list params = %#v, want empty", params)
	}

	err = executeSkillGroupTestCommand(t, client, "skill", "remote", "check")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "skill.remote.check" {
		t.Fatalf("method = %q, want skill.remote.check", client.method)
	}
	params = recordedSkillGroupParams(t, client)
	if len(params) != 0 {
		t.Fatalf("check all params = %#v, want empty", params)
	}

	err = executeSkillGroupTestCommand(t, client, "skill", "remote", "check", "asset-a")
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	params = recordedSkillGroupParams(t, client)
	if params["asset_id"] != "asset-a" {
		t.Fatalf("check asset params = %#v", params)
	}
}
