package cmdpolicy

import (
	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
)

const LayerPolicy = "policy"

func Apply(root *cobra.Command, decisions map[string]Decision, source, ruleName string) int {
	denied := deniedDecisions(root, decisions)
	count := 0
	walk(root, func(command *cobra.Command) {
		if !command.HasParent() {
			return
		}
		path := cmdmeta.Path(command)
		decision, ok := denied[path]
		if !ok {
			return
		}
		installDenial(command, path, source, ruleName, decision)
		count++
	})
	return count
}

func deniedDecisions(root *cobra.Command, decisions map[string]Decision) map[string]Decision {
	denied := map[string]Decision{}
	for path, decision := range decisions {
		if !decision.Allowed {
			denied[path] = decision
		}
	}
	aggregateDeniedParents(root, denied)
	return denied
}

func aggregateDeniedParents(command *cobra.Command, denied map[string]Decision) bool {
	if command == nil {
		return false
	}
	path := cmdmeta.Path(command)
	_, ownDenied := denied[path]
	children := command.Commands()
	if len(children) == 0 {
		return cmdmeta.IsAction(command) && ownDenied
	}

	liveChildSeen := false
	allLiveChildrenDenied := true
	for _, child := range children {
		if !hasRunnableDescendant(child) {
			continue
		}
		liveChildSeen = true
		if !aggregateDeniedParents(child, denied) {
			allLiveChildrenDenied = false
		}
	}

	if cmdmeta.IsAction(command) && !ownDenied {
		return false
	}
	if !liveChildSeen {
		return cmdmeta.IsAction(command) && ownDenied
	}
	if !allLiveChildrenDenied {
		return false
	}
	if command.HasParent() && path != "" && !ownDenied {
		denied[path] = Decision{
			ReasonCode: "all_children_denied",
			Reason:     "all child commands are denied",
		}
	}
	return true
}

func hasRunnableDescendant(command *cobra.Command) bool {
	if command == nil {
		return false
	}
	if cmdmeta.IsAction(command) {
		return true
	}
	for _, child := range command.Commands() {
		if hasRunnableDescendant(child) {
			return true
		}
	}
	return false
}

func installDenial(command *cobra.Command, path, source, ruleName string, decision Decision) {
	command.Hidden = true
	command.DisableFlagParsing = true
	command.Args = cobra.ArbitraryArgs
	command.PersistentPreRun = nil
	command.PersistentPreRunE = func(current *cobra.Command, _ []string) error {
		current.SilenceUsage = true
		return nil
	}
	command.PreRun = nil
	command.PreRunE = nil
	command.Run = nil
	cmdmeta.MarkDenied(command, LayerPolicy, source)
	command.RunE = func(*cobra.Command, []string) error {
		denied := &platform.CommandDeniedError{
			Path:         path,
			Layer:        LayerPolicy,
			PolicySource: source,
			RuleName:     ruleName,
			ReasonCode:   decision.ReasonCode,
			Reason:       decision.Reason,
		}
		return errs.NewPolicyError(errs.SubtypeCommandDenied, denied.Error()).
			WithCode(decision.ReasonCode).
			WithDetails(map[string]any{
				"path":          path,
				"layer":         LayerPolicy,
				"policy_source": source,
				"rule_name":     ruleName,
				"reason_code":   decision.ReasonCode,
				"reason":        decision.Reason,
			}).
			WithCause(denied)
	}
}
