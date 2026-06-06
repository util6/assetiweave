package cmd

import (
	"context"
	"sync/atomic"
	"testing"

	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdutil"
)

type restrictivePlugin struct {
	wrapped atomic.Int64
	before  atomic.Int64
	after   atomic.Int64
}

func (p *restrictivePlugin) Name() string    { return "readonly" }
func (p *restrictivePlugin) Version() string { return "0.1.0" }
func (p *restrictivePlugin) Capabilities() platform.Capabilities {
	return platform.Capabilities{Restricts: true, FailurePolicy: platform.FailClosed}
}
func (p *restrictivePlugin) Install(registrar platform.Registrar) error {
	registrar.Restrict(&platform.Rule{Name: "read-only", MaxRisk: platform.RiskRead})
	registrar.Observe(platform.Before, "before", platform.All(), func(context.Context, platform.Invocation) {
		p.before.Add(1)
	})
	registrar.Observe(platform.After, "after", platform.All(), func(context.Context, platform.Invocation) {
		p.after.Add(1)
	})
	registrar.Wrap("wrapper", platform.All(), func(next platform.Handler) platform.Handler {
		return func(ctx context.Context, invocation platform.Invocation) error {
			p.wrapped.Add(1)
			return next(ctx, invocation)
		}
	})
	return nil
}

func TestPluginRestrictionDeniesWriteBeforeEngineAndBypassesWrapper(t *testing.T) {
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	plugin := &restrictivePlugin{}
	platform.Register(plugin)
	client := &recordingClient{}
	factory := testPluginFactory(client)
	root, _ := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"skill", "delete", "demo"})

	err := root.Execute()

	if client.method != "" {
		t.Fatalf("engine method = %q, want no call", client.method)
	}
	assertTypedProblem(t, err, errs.CategoryPolicy, errs.SubtypeCommandDenied)
	if plugin.before.Load() != 1 || plugin.after.Load() != 1 {
		t.Fatalf("observer calls = before:%d after:%d, want 1/1", plugin.before.Load(), plugin.after.Load())
	}
	if plugin.wrapped.Load() != 0 {
		t.Fatalf("wrapper calls = %d, want 0 for denied command", plugin.wrapped.Load())
	}
}

func TestPluginRestrictionDenialWinsOverRequiredFlags(t *testing.T) {
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	plugin := &restrictivePlugin{}
	platform.Register(plugin)
	client := &recordingClient{}
	root, _ := buildInternal(context.Background(), testPluginFactory(client))
	root.SetArgs([]string{"source", "add"})

	err := root.Execute()

	if client.method != "" {
		t.Fatalf("engine method = %q, want no call", client.method)
	}
	assertTypedProblem(t, err, errs.CategoryPolicy, errs.SubtypeCommandDenied)
}

func TestPluginRestrictionAllowsReadCommand(t *testing.T) {
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	plugin := &restrictivePlugin{}
	platform.Register(plugin)
	client := &recordingClient{}
	factory := &cmdutil.Factory{
		IOStreams: testPluginFactory(client).IOStreams,
		Client:    client,
	}
	root, _ := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if client.method != "overview.get" {
		t.Fatalf("engine method = %q, want overview.get", client.method)
	}
	if plugin.wrapped.Load() != 1 {
		t.Fatalf("wrapper calls = %d, want 1", plugin.wrapped.Load())
	}
}

type skillDenyPlugin struct{}

func (skillDenyPlugin) Name() string    { return "skill-deny" }
func (skillDenyPlugin) Version() string { return "0.1.0" }
func (skillDenyPlugin) Capabilities() platform.Capabilities {
	return platform.Capabilities{Restricts: true, FailurePolicy: platform.FailClosed}
}
func (skillDenyPlugin) Install(registrar platform.Registrar) error {
	registrar.Restrict(&platform.Rule{Name: "no-skills", Deny: []string{"skill/**"}})
	return nil
}

func TestPluginRestrictionAggregatesDeniedParentGroup(t *testing.T) {
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	platform.Register(skillDenyPlugin{})
	client := &recordingClient{}
	root, _ := buildInternal(context.Background(), testPluginFactory(client))
	root.SetArgs([]string{"skill"})

	err := root.Execute()

	if client.method != "" {
		t.Fatalf("engine method = %q, want no call", client.method)
	}
	assertTypedProblem(t, err, errs.CategoryPolicy, errs.SubtypeCommandDenied)
}
