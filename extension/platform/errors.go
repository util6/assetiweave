package platform

import "fmt"

type CommandDeniedError struct {
	Path         string
	Layer        string
	PolicySource string
	RuleName     string
	ReasonCode   string
	Reason       string
}

func (e *CommandDeniedError) Error() string {
	if e.Reason != "" {
		return fmt.Sprintf("command %q denied: %s", e.Path, e.Reason)
	}
	return fmt.Sprintf("command %q denied (%s/%s)", e.Path, e.Layer, e.ReasonCode)
}
