package cmd

import (
	"context"
	"testing"

	"github.com/util6/assetiweave/internal/schema"
)

func TestTeamMemberCommandsUseEngineMethods(t *testing.T) {
	tests := []struct {
		args   []string
		method string
	}{
		{
			args:   []string{"team", "member", "turn", "team-1", "--member-id", "member-1", "--message", "hello"},
			method: schema.MethodTeamMemberTurnStart,
		},
		{
			args:   []string{"team", "member", "replay", "team-1", "--member-id", "member-1"},
			method: schema.MethodTeamMemberReplayStart,
		},
		{
			args:   []string{"team", "member", "stream", "team-1", "--member-id", "member-1", "--execution-id", "execution-1"},
			method: schema.MethodTeamMemberStreamSnapshot,
		},
		{
			args:   []string{"team", "member", "task", "get", "task-1"},
			method: schema.MethodTeamMemberTaskGet,
		},
		{
			args:   []string{"team", "member", "task", "list"},
			method: schema.MethodTeamMemberTasksList,
		},
		{
			args:   []string{"team", "member", "cancel", "team-1", "--member-id", "member-1", "--execution-id", "execution-1"},
			method: schema.MethodTeamMemberTurnCancel,
		},
	}

	for _, test := range tests {
		t.Run(test.method, func(t *testing.T) {
			client := &recordingClient{}
			root := Build(context.Background(), testPluginFactory(client))
			root.SetArgs(test.args)

			if err := root.Execute(); err != nil {
				t.Fatalf("Execute() error = %v", err)
			}
			if client.method != test.method {
				t.Fatalf("method = %q, want %q", client.method, test.method)
			}
		})
	}
}
