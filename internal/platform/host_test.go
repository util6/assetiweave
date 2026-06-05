package internalplatform

import (
	"bytes"
	"context"
	"errors"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/util6/assetiweave/extension/platform"
)

type testPlugin struct {
	name         string
	capabilities platform.Capabilities
	install      func(platform.Registrar) error
	installCalls *atomic.Int64
}

func (p testPlugin) Name() string    { return p.name }
func (p testPlugin) Version() string { return "1.0.0" }
func (p testPlugin) Capabilities() platform.Capabilities {
	return p.capabilities
}
func (p testPlugin) Install(registrar platform.Registrar) error {
	if p.installCalls != nil {
		p.installCalls.Add(1)
	}
	if p.install == nil {
		return nil
	}
	return p.install(registrar)
}

type testCommandView struct{}

func (testCommandView) Path() string                     { return "overview" }
func (testCommandView) Domain() string                   { return "overview" }
func (testCommandView) Risk() (platform.Risk, bool)      { return platform.RiskRead, true }
func (testCommandView) Identities() []platform.Identity  { return nil }
func (testCommandView) Annotation(string) (string, bool) { return "", false }

func TestInstallAllFailOpenDiscardsStagedHooks(t *testing.T) {
	staged := testPlugin{
		name:         "broken",
		capabilities: platform.Capabilities{FailurePolicy: platform.FailOpen},
		install: func(registrar platform.Registrar) error {
			registrar.Observe(platform.Before, "partial", platform.All(), func(context.Context, platform.Invocation) {})
			return errors.New("unavailable")
		},
	}
	healthy := testPlugin{
		name:         "healthy",
		capabilities: platform.Capabilities{FailurePolicy: platform.FailOpen},
		install: func(registrar platform.Registrar) error {
			registrar.Observe(platform.Before, "audit", platform.All(), func(context.Context, platform.Invocation) {})
			return nil
		},
	}
	errOut := &bytes.Buffer{}

	result, err := InstallAll([]platform.Plugin{staged, healthy}, errOut)

	if err != nil {
		t.Fatalf("InstallAll() error = %v", err)
	}
	observers := result.Registry.MatchingObservers(testCommandView{}, platform.Before)
	if len(observers) != 1 || observers[0].Name != "healthy.audit" {
		t.Fatalf("committed observers = %#v, want only healthy.audit", observers)
	}
	if !strings.Contains(errOut.String(), `plugin "broken" skipped`) {
		t.Fatalf("warning = %q, want skipped plugin", errOut.String())
	}
}

func TestInstallAllInvalidCapabilityAlwaysFailsClosed(t *testing.T) {
	plugin := testPlugin{
		name:         "invalid",
		capabilities: platform.Capabilities{FailurePolicy: platform.FailurePolicy(99)},
	}

	_, err := InstallAll([]platform.Plugin{plugin}, nil)

	var installErr *PluginInstallError
	if !errors.As(err, &installErr) || installErr.ReasonCode != ReasonInvalidCapability {
		t.Fatalf("error = %#v, want invalid capability", err)
	}
}

func TestInstallAllDuplicateNamesAbortBeforeInstall(t *testing.T) {
	calls := &atomic.Int64{}
	plugin := testPlugin{
		name:         "duplicate",
		capabilities: platform.Capabilities{FailurePolicy: platform.FailOpen},
		installCalls: calls,
	}

	_, err := InstallAll([]platform.Plugin{plugin, plugin}, nil)

	var installErr *PluginInstallError
	if !errors.As(err, &installErr) || installErr.ReasonCode != ReasonDuplicatePluginName {
		t.Fatalf("error = %#v, want duplicate plugin name", err)
	}
	if calls.Load() != 0 {
		t.Fatalf("Install calls = %d, want 0", calls.Load())
	}
}

func TestInstallAllInvalidHookIsAtomic(t *testing.T) {
	plugin := testPlugin{
		name:         "invalid-hook",
		capabilities: platform.Capabilities{FailurePolicy: platform.FailOpen},
		install: func(registrar platform.Registrar) error {
			registrar.Observe(platform.Before, "valid", platform.All(), func(context.Context, platform.Invocation) {})
			registrar.Wrap("invalid", nil, func(next platform.Handler) platform.Handler { return next })
			return nil
		},
	}

	result, err := InstallAll([]platform.Plugin{plugin}, nil)

	if err != nil {
		t.Fatalf("InstallAll() error = %v", err)
	}
	if result.Registry.HasHooks() {
		t.Fatal("invalid FailOpen plugin committed partial hooks")
	}
}

func TestInstallAllRestrictRequiresFailClosed(t *testing.T) {
	plugin := testPlugin{
		name: "unsafe-policy",
		capabilities: platform.Capabilities{
			Restricts:     true,
			FailurePolicy: platform.FailOpen,
		},
		install: func(registrar platform.Registrar) error {
			registrar.Restrict(&platform.Rule{MaxRisk: platform.RiskRead})
			return nil
		},
	}

	_, err := InstallAll([]platform.Plugin{plugin}, nil)

	var installErr *PluginInstallError
	if !errors.As(err, &installErr) || installErr.ReasonCode != ReasonRestrictsMismatch {
		t.Fatalf("error = %#v, want restricts mismatch", err)
	}
}

func TestInstallAllRestrictDeclarationMustMatchInstall(t *testing.T) {
	plugin := testPlugin{
		name:         "missing-policy",
		capabilities: platform.Capabilities{Restricts: true, FailurePolicy: platform.FailClosed},
	}

	_, err := InstallAll([]platform.Plugin{plugin}, nil)

	var installErr *PluginInstallError
	if !errors.As(err, &installErr) || installErr.ReasonCode != ReasonRestrictsMismatch {
		t.Fatalf("error = %#v, want restricts mismatch", err)
	}
}
