package cmdpolicy

import (
	"errors"
	"fmt"

	"github.com/util6/assetiweave/extension/platform"
)

var ErrMultipleRestrictPlugins = errors.New("multiple plugins contributed restrictions")

type PluginRule struct {
	PluginName string
	Rule       *platform.Rule
}

func ResolvePluginRules(contributions []PluginRule) ([]*platform.Rule, string, error) {
	owners := make([]string, 0, 1)
	seen := map[string]bool{}
	rules := make([]*platform.Rule, 0, len(contributions))
	for _, contribution := range contributions {
		if !seen[contribution.PluginName] {
			seen[contribution.PluginName] = true
			owners = append(owners, contribution.PluginName)
		}
		if err := ValidateRule(contribution.Rule); err != nil {
			return nil, "", fmt.Errorf("plugin %q rule invalid: %w", contribution.PluginName, err)
		}
		rules = append(rules, contribution.Rule)
	}
	if len(owners) > 1 {
		return nil, "", fmt.Errorf("%w: %v", ErrMultipleRestrictPlugins, owners)
	}
	if len(owners) == 0 {
		return nil, "", nil
	}
	return rules, "plugin:" + owners[0], nil
}
