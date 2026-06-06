package cmdpolicy

import (
	"errors"
	"testing"

	"github.com/util6/assetiweave/extension/platform"
)

func TestResolvePluginRulesRejectsMultipleOwners(t *testing.T) {
	_, _, err := ResolvePluginRules([]PluginRule{
		{PluginName: "one", Rule: &platform.Rule{MaxRisk: platform.RiskRead}},
		{PluginName: "two", Rule: &platform.Rule{MaxRisk: platform.RiskWrite}},
	})

	if !errors.Is(err, ErrMultipleRestrictPlugins) {
		t.Fatalf("error = %v, want multiple restriction owners", err)
	}
}

func TestResolvePluginRulesAllowsMultipleRulesFromOneOwner(t *testing.T) {
	rules, source, err := ResolvePluginRules([]PluginRule{
		{PluginName: "one", Rule: &platform.Rule{Allow: []string{"skill/**"}}},
		{PluginName: "one", Rule: &platform.Rule{Allow: []string{"source/**"}}},
	})

	if err != nil {
		t.Fatalf("ResolvePluginRules() error = %v", err)
	}
	if len(rules) != 2 || source != "plugin:one" {
		t.Fatalf("rules/source = %d/%q, want 2/plugin:one", len(rules), source)
	}
}
