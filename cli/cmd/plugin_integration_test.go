package cmd

import (
	"bytes"
	"context"
	"errors"
	"sync/atomic"
	"testing"

	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdutil"
	"github.com/util6/assetiweave/internal/hook"
)

type integrationPlugin struct {
	before   atomic.Int64
	after    atomic.Int64
	wrapped  atomic.Int64
	startup  atomic.Int64
	shutdown atomic.Int64
}

func (p *integrationPlugin) Name() string    { return "integration" }
func (p *integrationPlugin) Version() string { return "0.1.0" }
func (p *integrationPlugin) Capabilities() platform.Capabilities {
	return platform.Capabilities{FailurePolicy: platform.FailOpen}
}

func (p *integrationPlugin) Install(registrar platform.Registrar) error {
	registrar.Observe(platform.Before, "before", platform.All(), func(context.Context, platform.Invocation) {
		p.before.Add(1)
	})
	registrar.Observe(platform.After, "after", platform.All(), func(context.Context, platform.Invocation) {
		p.after.Add(1)
	})
	registrar.Wrap("wrapped", platform.All(), func(next platform.Handler) platform.Handler {
		return func(ctx context.Context, invocation platform.Invocation) error {
			p.wrapped.Add(1)
			return next(ctx, invocation)
		}
	})
	registrar.On(platform.Startup, "startup", func(context.Context, *platform.LifecycleContext) error {
		p.startup.Add(1)
		return nil
	})
	registrar.On(platform.Shutdown, "shutdown", func(context.Context, *platform.LifecycleContext) error {
		p.shutdown.Add(1)
		return nil
	})
	return nil
}

func TestPluginPipelineWiresLifecycleAndCommandHooks(t *testing.T) {
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	plugin := &integrationPlugin{}
	platform.Register(plugin)

	factory := &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{
			In:     &bytes.Buffer{},
			Out:    &bytes.Buffer{},
			ErrOut: &bytes.Buffer{},
		},
		Client: &recordingClient{},
	}
	root, registry := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	if plugin.startup.Load() != 1 {
		t.Fatalf("startup calls = %d, want 1", plugin.startup.Load())
	}
	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if plugin.before.Load() != 1 || plugin.after.Load() != 1 || plugin.wrapped.Load() != 1 {
		t.Fatalf(
			"command hook calls = before:%d after:%d wrapped:%d",
			plugin.before.Load(),
			plugin.after.Load(),
			plugin.wrapped.Load(),
		)
	}
	if err := hook.Emit(context.Background(), registry, platform.Shutdown, nil, factory.IOStreams.ErrOut); err != nil {
		t.Fatalf("shutdown emit error = %v", err)
	}
	if plugin.shutdown.Load() != 1 {
		t.Fatalf("shutdown calls = %d, want 1", plugin.shutdown.Load())
	}
}

type failingPlugin struct {
	policy  platform.FailurePolicy
	startup bool
}

func (p failingPlugin) Name() string    { return "failing" }
func (p failingPlugin) Version() string { return "0.1.0" }
func (p failingPlugin) Capabilities() platform.Capabilities {
	return platform.Capabilities{FailurePolicy: p.policy}
}
func (p failingPlugin) Install(registrar platform.Registrar) error {
	if !p.startup {
		return errors.New("install failed")
	}
	registrar.On(platform.Startup, "startup", func(context.Context, *platform.LifecycleContext) error {
		return errors.New("startup failed")
	})
	return nil
}

func TestFailClosedPluginInstallBlocksCommandExecution(t *testing.T) {
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	platform.Register(failingPlugin{policy: platform.FailClosed})
	client := &recordingClient{}
	factory := testPluginFactory(client)
	root, registry := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	err := root.Execute()

	if registry != nil {
		t.Fatal("registry = non-nil after FailClosed install failure")
	}
	if client.method != "" {
		t.Fatalf("engine method = %q, want no call", client.method)
	}
	assertTypedProblem(t, err, errs.CategoryConfig, errs.SubtypePluginInstallFailed)
}

func TestStartupFailureBlocksCommandExecution(t *testing.T) {
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	platform.Register(failingPlugin{policy: platform.FailClosed, startup: true})
	client := &recordingClient{}
	factory := testPluginFactory(client)
	root, registry := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	err := root.Execute()

	if registry != nil {
		t.Fatal("registry = non-nil after Startup failure")
	}
	if client.method != "" {
		t.Fatalf("engine method = %q, want no call", client.method)
	}
	assertTypedProblem(t, err, errs.CategoryConfig, errs.SubtypePluginLifecycleFailed)
}

type policyPlugin struct {
	name string
}

func (p policyPlugin) Name() string    { return p.name }
func (p policyPlugin) Version() string { return "0.1.0" }
func (p policyPlugin) Capabilities() platform.Capabilities {
	return platform.Capabilities{Restricts: true, FailurePolicy: platform.FailClosed}
}
func (p policyPlugin) Install(registrar platform.Registrar) error {
	registrar.Restrict(&platform.Rule{Name: p.name, MaxRisk: platform.RiskRead})
	return nil
}

func TestPluginPolicyConflictBlocksCommandWithTypedPolicyError(t *testing.T) {
	platform.ResetForTesting()
	t.Cleanup(platform.ResetForTesting)
	platform.Register(policyPlugin{name: "one"})
	platform.Register(policyPlugin{name: "two"})
	client := &recordingClient{}
	factory := testPluginFactory(client)
	root, registry := buildInternal(context.Background(), factory)
	root.SetArgs([]string{"overview"})

	err := root.Execute()

	if registry != nil {
		t.Fatal("registry = non-nil after plugin policy conflict")
	}
	if client.method != "" {
		t.Fatalf("engine method = %q, want no call", client.method)
	}
	assertTypedProblem(t, err, errs.CategoryPolicy, errs.SubtypePluginPolicyConflict)
}

func testPluginFactory(client *recordingClient) *cmdutil.Factory {
	return &cmdutil.Factory{
		IOStreams: &cmdutil.IOStreams{
			In:     &bytes.Buffer{},
			Out:    &bytes.Buffer{},
			ErrOut: &bytes.Buffer{},
		},
		Client: client,
	}
}
