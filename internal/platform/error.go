package internalplatform

import "fmt"

type PluginInstallError struct {
	PluginName string
	ReasonCode string
	Reason     string
	Cause      error
}

func (e *PluginInstallError) Error() string {
	message := fmt.Sprintf("plugin %q (%s)", e.PluginName, e.ReasonCode)
	if e.Reason != "" {
		message += ": " + e.Reason
	}
	if e.Cause != nil {
		message += ": " + e.Cause.Error()
	}
	return message
}

func (e *PluginInstallError) Unwrap() error {
	return e.Cause
}

const (
	ReasonInvalidPluginName   = "invalid_plugin_name"
	ReasonDuplicatePluginName = "duplicate_plugin_name"
	ReasonCapabilitiesPanic   = "capabilities_panic"
	ReasonInvalidCapability   = "invalid_capability"
	ReasonInstallFailed       = "install_failed"
	ReasonInstallPanic        = "install_panic"
	ReasonInvalidHook         = "invalid_hook"
	ReasonInvalidRule         = "invalid_rule"
	ReasonRestrictsMismatch   = "restricts_mismatch"
	ReasonCapabilityUnmet     = "capability_unmet"
)
