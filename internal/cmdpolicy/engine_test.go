package cmdpolicy

import (
	"errors"
	"testing"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
	"github.com/util6/assetiweave/internal/output"
)

func TestEngineDeniesWriteAboveMaxRisk(t *testing.T) {
	command := &cobra.Command{Use: "delete", RunE: func(*cobra.Command, []string) error { return nil }}
	root := &cobra.Command{Use: "test"}
	root.AddCommand(command)
	cmdmeta.Set(command, cmdmeta.Metadata{Domain: "skill", Risk: platform.RiskHighRiskWrite})

	decision := New([]*platform.Rule{{MaxRisk: platform.RiskRead}}).Evaluate(command)

	if decision.Allowed || decision.ReasonCode != "high_risk_write_not_allowed" {
		t.Fatalf("decision = %+v, want high-risk denial", decision)
	}
}

func TestEngineAllowsWhenAnyRuleMatches(t *testing.T) {
	command := &cobra.Command{Use: "list", RunE: func(*cobra.Command, []string) error { return nil }}
	root := &cobra.Command{Use: "test"}
	skill := &cobra.Command{Use: "skill"}
	root.AddCommand(skill)
	skill.AddCommand(command)
	cmdmeta.Set(command, cmdmeta.Metadata{Domain: "skill", Risk: platform.RiskRead})

	decision := New([]*platform.Rule{
		{Allow: []string{"source/**"}},
		{Allow: []string{"skill/**", "skill"}},
	}).Evaluate(command)

	if !decision.Allowed {
		t.Fatalf("decision = %+v, want allowed by second rule", decision)
	}
}

func TestApplyReturnsStructuredCommandDenial(t *testing.T) {
	called := false
	command := &cobra.Command{
		Use: "delete",
		RunE: func(*cobra.Command, []string) error {
			called = true
			return nil
		},
	}
	root := &cobra.Command{Use: "test", SilenceUsage: true, SilenceErrors: true}
	root.AddCommand(command)
	cmdmeta.Set(command, cmdmeta.Metadata{Domain: "skill", Risk: platform.RiskHighRiskWrite})
	engine := New([]*platform.Rule{{MaxRisk: platform.RiskRead}})
	Apply(root, engine.EvaluateAll(root), "plugin:readonly", "read-only")
	root.SetArgs([]string{"delete"})

	err := root.Execute()

	if called {
		t.Fatal("denied command handler was called")
	}
	var exitErr *output.ExitError
	if !errors.As(err, &exitErr) || exitErr.Detail.Type != "command_denied" {
		t.Fatalf("error = %#v, want structured command_denied", err)
	}
	var denied *platform.CommandDeniedError
	if !errors.As(err, &denied) || denied.PolicySource != "plugin:readonly" {
		t.Fatalf("error chain = %#v, want plugin policy denial", err)
	}
}

func TestApplyAggregatesParentWhenAllChildrenDenied(t *testing.T) {
	root := &cobra.Command{Use: "test", SilenceUsage: true, SilenceErrors: true}
	skill := &cobra.Command{Use: "skill"}
	root.AddCommand(skill)
	list := &cobra.Command{Use: "list", RunE: func(*cobra.Command, []string) error { return nil }}
	remove := &cobra.Command{Use: "delete", RunE: func(*cobra.Command, []string) error { return nil }}
	skill.AddCommand(list)
	skill.AddCommand(remove)
	cmdmeta.Set(list, cmdmeta.Metadata{Domain: "skill", Risk: platform.RiskRead})
	cmdmeta.Set(remove, cmdmeta.Metadata{Domain: "skill", Risk: platform.RiskHighRiskWrite})
	engine := New([]*platform.Rule{{Allow: []string{"overview"}}})

	Apply(root, engine.EvaluateAll(root), "plugin:readonly", "read-only")
	root.SetArgs([]string{"skill"})

	err := root.Execute()

	var denied *platform.CommandDeniedError
	if !errors.As(err, &denied) {
		t.Fatalf("error = %#v, want aggregated command denial", err)
	}
	if denied.Path != "skill" || denied.ReasonCode != "all_children_denied" {
		t.Fatalf("denial = %+v, want parent all_children_denied", denied)
	}
}

func TestApplyKeepsParentWhenSomeChildAllowed(t *testing.T) {
	root := &cobra.Command{Use: "test", SilenceUsage: true, SilenceErrors: true}
	skill := &cobra.Command{Use: "skill"}
	root.AddCommand(skill)
	list := &cobra.Command{Use: "list", RunE: func(*cobra.Command, []string) error { return nil }}
	remove := &cobra.Command{Use: "delete", RunE: func(*cobra.Command, []string) error { return nil }}
	skill.AddCommand(list)
	skill.AddCommand(remove)
	cmdmeta.Set(list, cmdmeta.Metadata{Domain: "skill", Risk: platform.RiskRead})
	cmdmeta.Set(remove, cmdmeta.Metadata{Domain: "skill", Risk: platform.RiskHighRiskWrite})
	engine := New([]*platform.Rule{{MaxRisk: platform.RiskRead}})

	Apply(root, engine.EvaluateAll(root), "plugin:readonly", "read-only")

	if skill.Hidden || skill.RunE != nil {
		t.Fatalf("parent skill was denied even though one child remains allowed")
	}
}
