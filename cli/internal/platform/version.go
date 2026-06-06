package internalplatform

import (
	"fmt"
	"strconv"
	"strings"

	"github.com/util6/assetiweave/internal/protocol"
)

var currentCLIVersion = func() string { return protocol.CLIVersion }

func SetCurrentCLIVersionForTesting(version string) func() {
	previous := currentCLIVersion
	currentCLIVersion = func() string { return version }
	return func() { currentCLIVersion = previous }
}

func satisfiesRequiredCLIVersion(buildVersion, constraint string) (bool, error) {
	constraint = strings.TrimSpace(constraint)
	if constraint == "" {
		return true, nil
	}
	operator, requiredValue := splitVersionConstraint(constraint)
	required, err := parseVersion(requiredValue)
	if err != nil {
		return false, fmt.Errorf("invalid RequiredCLIVersion %q: %w", constraint, err)
	}
	if buildVersion == "" || strings.EqualFold(buildVersion, "dev") {
		return true, nil
	}
	actual, err := parseVersion(buildVersion)
	if err != nil {
		return true, nil
	}
	comparison := compareVersions(actual, required)
	switch operator {
	case "", "=":
		return comparison == 0, nil
	case ">=":
		return comparison >= 0, nil
	case ">":
		return comparison > 0, nil
	case "<=":
		return comparison <= 0, nil
	case "<":
		return comparison < 0, nil
	default:
		return false, fmt.Errorf("invalid RequiredCLIVersion %q: unknown operator %q", constraint, operator)
	}
}

func splitVersionConstraint(value string) (operator, version string) {
	switch {
	case strings.HasPrefix(value, ">="):
		return ">=", strings.TrimSpace(value[2:])
	case strings.HasPrefix(value, "<="):
		return "<=", strings.TrimSpace(value[2:])
	case strings.HasPrefix(value, ">"):
		return ">", strings.TrimSpace(value[1:])
	case strings.HasPrefix(value, "<"):
		return "<", strings.TrimSpace(value[1:])
	case strings.HasPrefix(value, "="):
		return "=", strings.TrimSpace(value[1:])
	default:
		return "", value
	}
}

func parseVersion(value string) ([3]int, error) {
	value = strings.TrimPrefix(strings.TrimSpace(value), "v")
	var version [3]int
	if value == "" {
		return version, fmt.Errorf("empty version")
	}
	for index, char := range value {
		if char == '-' || char == '+' {
			value = value[:index]
			break
		}
	}
	parts := strings.Split(value, ".")
	if len(parts) > 3 {
		return version, fmt.Errorf("too many version components")
	}
	for index, part := range parts {
		parsed, err := strconv.Atoi(strings.TrimSpace(part))
		if err != nil || parsed < 0 {
			return version, fmt.Errorf("non-numeric version component %q", part)
		}
		version[index] = parsed
	}
	return version, nil
}

func compareVersions(left, right [3]int) int {
	for index := range left {
		switch {
		case left[index] < right[index]:
			return -1
		case left[index] > right[index]:
			return 1
		}
	}
	return 0
}
