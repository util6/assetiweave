package cmd

import (
	"bytes"
	"context"
	"testing"

	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/schema"
)

func TestTenantCommandsCallEngineMethods(t *testing.T) {
	tests := []struct {
		name       string
		args       []string
		wantMethod string
		wantParams map[string]any
	}{
		{
			name:       "list",
			args:       []string{"tenant", "list"},
			wantMethod: schema.MethodTenantList,
			wantParams: map[string]any{},
		},
		{
			name:       "active",
			args:       []string{"tenant", "active"},
			wantMethod: schema.MethodTenantActive,
			wantParams: map[string]any{},
		},
		{
			name:       "create",
			args:       []string{"tenant", "create", "Client A", "--slug", "client-a"},
			wantMethod: schema.MethodTenantCreate,
			wantParams: map[string]any{"name": "Client A", "slug": "client-a", "set_active": true},
		},
		{
			name:       "create without switching",
			args:       []string{"tenant", "create", "Client B", "--set-active=false"},
			wantMethod: schema.MethodTenantCreate,
			wantParams: map[string]any{"name": "Client B", "slug": nil, "set_active": false},
		},
		{
			name:       "switch",
			args:       []string{"tenant", "switch", "client-a"},
			wantMethod: schema.MethodTenantSwitch,
			wantParams: map[string]any{"id": "client-a"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			client := &recordingClient{}
			factory := &cmdutil.Factory{
				IOStreams: &cmdutil.IOStreams{In: &bytes.Buffer{}, Out: &bytes.Buffer{}, ErrOut: &bytes.Buffer{}},
				Client:    client,
			}
			root := Build(context.Background(), factory)
			root.SetArgs(tt.args)

			if err := root.Execute(); err != nil {
				t.Fatalf("Execute() error = %v", err)
			}
			if client.method != tt.wantMethod {
				t.Fatalf("method = %q, want %q", client.method, tt.wantMethod)
			}
			got, ok := client.params.(map[string]any)
			if !ok {
				t.Fatalf("params = %#v, want map", client.params)
			}
			for key, want := range tt.wantParams {
				if got[key] != want {
					t.Fatalf("params[%q] = %#v, want %#v in %#v", key, got[key], want, got)
				}
			}
		})
	}
}
