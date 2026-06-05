package internalplatform

import (
	"context"
	"testing"

	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdpolicy"
	"github.com/util6/assetiweave/internal/hook"
)

func TestBuildInventoryGroupsHooksAndRulesByPlugin(t *testing.T) {
	registry := hook.NewRegistry()
	registry.AddObserver(hook.ObserverEntry{Name: "audit.before", When: platform.Before, Selector: platform.All(), Fn: func(context.Context, platform.Invocation) {}})
	registry.AddWrapper(hook.WrapperEntry{Name: "audit.wrap", Selector: platform.All(), Fn: func(next platform.Handler) platform.Handler { return next }})
	registry.AddLifecycle(hook.LifecycleEntry{Name: "audit.start", Event: platform.Startup, Fn: func(context.Context, *platform.LifecycleContext) error { return nil }})
	rule := &platform.Rule{Name: "read", Allow: []string{"overview"}, MaxRisk: platform.RiskRead}

	inventory := BuildInventory(
		[]PluginInfo{{Name: "audit", Version: "0.1.0", Capabilities: platform.Capabilities{Restricts: true, FailurePolicy: platform.FailClosed}}},
		registry,
		[]cmdpolicy.PluginRule{{PluginName: "audit", Rule: rule}},
	)

	if len(inventory.Plugins) != 1 {
		t.Fatalf("plugins = %d, want 1", len(inventory.Plugins))
	}
	plugin := inventory.Plugins[0]
	if plugin.Name != "audit" || !plugin.Capabilities.Restricts || plugin.Capabilities.FailurePolicy != "FailClosed" {
		t.Fatalf("plugin entry = %+v", plugin)
	}
	if len(plugin.Observers) != 1 || len(plugin.Wrappers) != 1 || len(plugin.Lifecycles) != 1 || len(plugin.Rules) != 1 {
		t.Fatalf("inventory entry missing contributions: %+v", plugin)
	}
}

func TestActiveInventoryReturnsDeepCopy(t *testing.T) {
	original := &Inventory{Plugins: []PluginEntry{{
		Name: "audit",
		Rules: []RuleView{{
			Name:       "read",
			Allow:      []string{"overview"},
			Deny:       []string{"skill/delete"},
			Identities: []string{"agent"},
		}},
	}}}
	SetActiveInventory(original)
	t.Cleanup(func() { SetActiveInventory(nil) })

	first := GetActiveInventory()
	first.Plugins[0].Rules[0].Allow[0] = "mutated"
	first.Plugins[0].Rules[0].Deny[0] = "mutated"
	first.Plugins[0].Rules[0].Identities[0] = "mutated"

	second := GetActiveInventory()
	rule := second.Plugins[0].Rules[0]
	if rule.Allow[0] != "overview" || rule.Deny[0] != "skill/delete" || rule.Identities[0] != "agent" {
		t.Fatalf("active inventory was mutated through clone: %+v", rule)
	}
}
