package cmdpolicy

import (
	"fmt"
	"strings"

	"github.com/bmatcuk/doublestar/v4"
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
)

type Decision struct {
	Allowed    bool
	ReasonCode string
	Reason     string
}

type Engine struct {
	rules []*platform.Rule
}

func New(rules []*platform.Rule) *Engine {
	copied := make([]*platform.Rule, 0, len(rules))
	for _, rule := range rules {
		if rule != nil {
			copied = append(copied, rule)
		}
	}
	return &Engine{rules: copied}
}

func (e *Engine) EvaluateAll(root *cobra.Command) map[string]Decision {
	decisions := map[string]Decision{}
	walk(root, func(command *cobra.Command) {
		if !command.HasParent() || !command.Runnable() {
			return
		}
		decisions[cmdmeta.Path(command)] = e.Evaluate(command)
	})
	return decisions
}

func (e *Engine) Evaluate(command *cobra.Command) Decision {
	if len(e.rules) == 0 {
		return Decision{Allowed: true}
	}
	view := cmdmeta.View(command)
	if isDiagnosticPath(view.Path()) {
		return Decision{Allowed: true}
	}
	risk, hasRisk := view.Risk()
	if hasRisk && !risk.IsValid() {
		return Decision{ReasonCode: "risk_invalid", Reason: fmt.Sprintf("command has invalid risk %q", risk)}
	}
	denials := make([]Decision, 0, len(e.rules))
	for _, rule := range e.rules {
		decision := evaluateRule(rule, view, risk, hasRisk)
		if decision.Allowed {
			return decision
		}
		denials = append(denials, decision)
	}
	if len(denials) == 1 {
		return denials[0]
	}
	reasons := make([]string, len(denials))
	for index, denial := range denials {
		reasons[index] = denial.ReasonCode
	}
	return Decision{ReasonCode: "no_matching_rule", Reason: "no rule grants this command: " + strings.Join(reasons, ", ")}
}

func isDiagnosticPath(path string) bool {
	switch path {
	case "version", "schema", "doctor", "completion", "config/plugins/show":
		return true
	default:
		return false
	}
}

func evaluateRule(rule *platform.Rule, command platform.CommandView, risk platform.Risk, hasRisk bool) Decision {
	if !hasRisk && !rule.AllowUnannotated {
		return Decision{ReasonCode: "risk_not_annotated", Reason: "command has no risk annotation"}
	}
	if pattern, ok := firstMatch(rule.Deny, command.Path()); ok {
		return Decision{ReasonCode: "command_denylisted", Reason: fmt.Sprintf("command matched deny pattern %q", pattern)}
	}
	if len(rule.Allow) > 0 && !matchesAny(rule.Allow, command.Path()) {
		return Decision{ReasonCode: "command_not_allowed", Reason: "command is outside the rule allow list"}
	}
	if rule.MaxRisk != "" && hasRisk {
		actualRank, _ := risk.Rank()
		maxRank, _ := rule.MaxRisk.Rank()
		if actualRank > maxRank {
			return Decision{ReasonCode: riskReasonCode(risk), Reason: fmt.Sprintf("command risk %q exceeds max_risk %q", risk, rule.MaxRisk)}
		}
	}
	if len(rule.Identities) > 0 && !identityIntersection(rule.Identities, command.Identities()) {
		return Decision{ReasonCode: "identity_not_supported", Reason: "command does not support an allowed identity"}
	}
	return Decision{Allowed: true}
}

func firstMatch(patterns []string, value string) (string, bool) {
	for _, pattern := range patterns {
		if matched, err := doublestar.Match(pattern, value); err == nil && matched {
			return pattern, true
		}
	}
	return "", false
}

func matchesAny(patterns []string, value string) bool {
	_, ok := firstMatch(patterns, value)
	return ok
}

func identityIntersection(left, right []platform.Identity) bool {
	for _, a := range left {
		for _, b := range right {
			if a == b {
				return true
			}
		}
	}
	return false
}

func riskReasonCode(risk platform.Risk) string {
	switch risk {
	case platform.RiskWrite:
		return "write_not_allowed"
	case platform.RiskHighRiskWrite:
		return "high_risk_write_not_allowed"
	default:
		return "risk_not_allowed"
	}
}

func walk(root *cobra.Command, visit func(*cobra.Command)) {
	if root == nil {
		return
	}
	visit(root)
	for _, child := range root.Commands() {
		walk(child, visit)
	}
}
