package cmd

import (
	"errors"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/internal/hook"
	internalplatform "github.com/util6/assetiweave/internal/platform"
)

func installPluginInstallGuard(root *cobra.Command, installErr error) {
	installFatalGuard(root, func() error {
		details := map[string]any{
			"reason_code": internalplatform.ReasonInstallFailed,
		}
		var pluginErr *internalplatform.PluginInstallError
		if errors.As(installErr, &pluginErr) {
			details["plugin"] = pluginErr.PluginName
			details["reason_code"] = pluginErr.ReasonCode
			details["reason"] = pluginErr.Reason
		}
		return errs.NewConfigError(errs.SubtypePluginInstallFailed, installErr.Error()).
			WithCode("plugin_install").
			WithDetails(details).
			WithCause(installErr)
	})
}

func installPluginLifecycleGuard(root *cobra.Command, lifecycleErr error) {
	installFatalGuard(root, func() error {
		reason := "lifecycle_failed"
		details := map[string]any{"reason_code": reason}
		var typed *hook.LifecycleError
		if errors.As(lifecycleErr, &typed) {
			if typed.Panic {
				reason = "lifecycle_panic"
			}
			details = map[string]any{
				"event":       "startup",
				"hook_name":   typed.HookName,
				"reason_code": reason,
			}
		}
		return errs.NewConfigError(errs.SubtypePluginLifecycleFailed, lifecycleErr.Error()).
			WithCode("plugin_lifecycle").
			WithDetails(details).
			WithCause(lifecycleErr)
	})
}

func installPluginPolicyGuard(root *cobra.Command, policyErr error) {
	installFatalGuard(root, func() error {
		return errs.NewPolicyError(errs.SubtypePluginPolicyConflict, policyErr.Error()).
			WithCode("plugin_policy").
			WithDetails(map[string]any{
				"reason_code": "policy_conflict",
			}).
			WithCause(policyErr)
	})
}

func installPluginConfigGuard(root *cobra.Command, configErr error) {
	installFatalGuard(root, func() error {
		return errs.NewConfigError(errs.SubtypeInvalidPluginConfig, configErr.Error()).
			WithCode("invalid_plugin_config").
			WithDetails(map[string]any{
				"reason_code": "invalid_plugin_config",
			}).
			WithCause(configErr)
	})
}

func installFatalGuard(root *cobra.Command, makeError func() error) {
	if root == nil {
		return
	}
	walkGuard(root, makeError)
}

func walkGuard(command *cobra.Command, makeError func() error) {
	command.PersistentPreRun = nil
	command.PersistentPreRunE = func(current *cobra.Command, _ []string) error {
		current.SilenceUsage = true
		return makeError()
	}
	if command.Runnable() {
		command.Args = cobra.ArbitraryArgs
		command.PreRun = nil
		command.PreRunE = func(current *cobra.Command, _ []string) error {
			current.SilenceUsage = true
			return makeError()
		}
		command.Run = nil
		command.RunE = func(current *cobra.Command, _ []string) error {
			current.SilenceUsage = true
			return makeError()
		}
	}
	for _, child := range command.Commands() {
		walkGuard(child, makeError)
	}
}
