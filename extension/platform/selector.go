package platform

import "github.com/bmatcuk/doublestar/v4"

type Selector func(CommandView) bool

func All() Selector {
	return func(CommandView) bool { return true }
}

func None() Selector {
	return func(CommandView) bool { return false }
}

func ByDomain(domains ...string) Selector {
	wanted := make(map[string]bool, len(domains))
	for _, domain := range domains {
		wanted[domain] = true
	}
	return func(command CommandView) bool {
		domain := command.Domain()
		return domain != "" && wanted[domain]
	}
}

func ByCommandPath(patterns ...string) Selector {
	return func(command CommandView) bool {
		for _, pattern := range patterns {
			if matched, err := doublestar.Match(pattern, command.Path()); err == nil && matched {
				return true
			}
		}
		return false
	}
}

func ByIdentity(identity Identity) Selector {
	return func(command CommandView) bool {
		for _, candidate := range command.Identities() {
			if candidate == identity {
				return true
			}
		}
		return false
	}
}

func ByExactRisk(risk Risk) Selector {
	return func(command CommandView) bool {
		actual, ok := command.Risk()
		return ok && actual == risk
	}
}

func ByWrite() Selector {
	return func(command CommandView) bool {
		actual, ok := command.Risk()
		return ok && (actual == RiskWrite || actual == RiskHighRiskWrite)
	}
}

func ByReadOnly() Selector {
	return ByExactRisk(RiskRead)
}

func (selector Selector) And(other Selector) Selector {
	left, right := normalize(selector), normalize(other)
	return func(command CommandView) bool { return left(command) && right(command) }
}

func (selector Selector) Or(other Selector) Selector {
	left, right := normalize(selector), normalize(other)
	return func(command CommandView) bool { return left(command) || right(command) }
}

func (selector Selector) Not() Selector {
	inner := normalize(selector)
	return func(command CommandView) bool { return !inner(command) }
}

func normalize(selector Selector) Selector {
	if selector == nil {
		return None()
	}
	return selector
}
