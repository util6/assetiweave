package platform

import (
	"context"
	"testing"
)

type recordingRegistrar struct {
	observed   int
	wrapped    int
	lifecycles int
	rules      int
}

func (r *recordingRegistrar) Config() PluginConfig {
	return PluginConfig{}
}

func (r *recordingRegistrar) Observe(When, string, Selector, Observer) {
	r.observed++
}

func (r *recordingRegistrar) Wrap(string, Selector, Wrapper) {
	r.wrapped++
}

func (r *recordingRegistrar) On(LifecycleEvent, string, LifecycleHandler) {
	r.lifecycles++
}

func (r *recordingRegistrar) Restrict(*Rule) {
	r.rules++
}

func TestBuilderBuildsPluginAndInstallsActions(t *testing.T) {
	plugin, err := NewPlugin("audit", "0.1.0").
		Observer(Before, "before", All(), func(context.Context, Invocation) {}).
		Wrap("wrapper", All(), func(next Handler) Handler { return next }).
		On(Startup, "startup", func(context.Context, *LifecycleContext) error { return nil }).
		Build()

	if err != nil {
		t.Fatalf("Build() error = %v", err)
	}
	registrar := &recordingRegistrar{}
	if err := plugin.Install(registrar); err != nil {
		t.Fatalf("Install() error = %v", err)
	}
	if registrar.observed != 1 || registrar.wrapped != 1 || registrar.lifecycles != 1 {
		t.Fatalf("registrar calls = %+v, want one hook of each kind", registrar)
	}
}

func TestBuilderRestrictImpliesFailClosed(t *testing.T) {
	plugin, err := NewPlugin("policy", "0.1.0").
		Restrict(&Rule{MaxRisk: RiskRead}).
		Build()

	if err != nil {
		t.Fatalf("Build() error = %v", err)
	}
	if capabilities := plugin.Capabilities(); !capabilities.Restricts || capabilities.FailurePolicy != FailClosed {
		t.Fatalf("capabilities = %+v, want restricting FailClosed plugin", capabilities)
	}
	registrar := &recordingRegistrar{}
	if err := plugin.Install(registrar); err != nil {
		t.Fatalf("Install() error = %v", err)
	}
	if registrar.rules != 1 {
		t.Fatalf("Restrict calls = %d, want 1", registrar.rules)
	}
}

func TestBuilderRequireCLIRecordsCapability(t *testing.T) {
	plugin, err := NewPlugin("audit", "0.1.0").
		RequireCLI(">=1.2.0").
		Build()

	if err != nil {
		t.Fatalf("Build() error = %v", err)
	}
	if got := plugin.Capabilities().RequiredCLIVersion; got != ">=1.2.0" {
		t.Fatalf("RequiredCLIVersion = %q, want >=1.2.0", got)
	}
}

func TestBuilderRejectsDuplicateHookNames(t *testing.T) {
	_, err := NewPlugin("audit", "0.1.0").
		Observer(Before, "same", All(), func(context.Context, Invocation) {}).
		On(Shutdown, "same", func(context.Context, *LifecycleContext) error { return nil }).
		Build()

	if err == nil {
		t.Fatal("Build() error = nil, want duplicate hook name error")
	}
}
