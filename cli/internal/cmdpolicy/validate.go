package cmdpolicy

import (
	"fmt"

	"github.com/bmatcuk/doublestar/v4"
	"github.com/util6/assetiweave/extension/platform"
)

func ValidateRule(rule *platform.Rule) error {
	if rule == nil {
		return fmt.Errorf("rule is nil")
	}
	if rule.MaxRisk != "" && !rule.MaxRisk.IsValid() {
		return fmt.Errorf("invalid max_risk %q", rule.MaxRisk)
	}
	for _, identity := range rule.Identities {
		if !identity.IsValid() {
			return fmt.Errorf("invalid identity %q", identity)
		}
	}
	for _, pattern := range append(append([]string(nil), rule.Allow...), rule.Deny...) {
		if pattern == "" {
			return fmt.Errorf("policy glob must not be empty")
		}
		if _, err := doublestar.Match(pattern, ""); err != nil {
			return fmt.Errorf("invalid policy glob %q: %w", pattern, err)
		}
	}
	return nil
}
