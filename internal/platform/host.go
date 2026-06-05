package internalplatform

import (
	"errors"
	"fmt"
	"io"

	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdpolicy"
	"github.com/util6/assetiweave/internal/hook"
)

type PluginInfo struct {
	Name         string
	Version      string
	Capabilities platform.Capabilities
	ConfigKeys   []string
}

type InstallResult struct {
	Registry    *hook.Registry
	PluginRules []cmdpolicy.PluginRule
	Plugins     []PluginInfo
}

func InstallAll(plugins []platform.Plugin, errOut io.Writer) (*InstallResult, error) {
	return InstallAllWithOptions(plugins, errOut)
}

type installOptions struct {
	pluginConfig *PluginConfigStore
}

type InstallOption func(*installOptions)

func WithPluginConfig(store *PluginConfigStore) InstallOption {
	return func(options *installOptions) {
		options.pluginConfig = store
	}
}

func InstallAllWithOptions(plugins []platform.Plugin, errOut io.Writer, opts ...InstallOption) (*InstallResult, error) {
	if errOut == nil {
		errOut = io.Discard
	}
	options := installOptions{pluginConfig: EmptyPluginConfigStore()}
	for _, opt := range opts {
		opt(&options)
	}
	named, err := validateAndNamePlugins(plugins)
	if err != nil {
		return nil, err
	}
	result := &InstallResult{Registry: hook.NewRegistry()}
	for _, entry := range named {
		capabilities, err := safeCapabilities(entry.name, entry.plugin)
		if err != nil {
			return nil, err
		}
		if capabilities.FailurePolicy != platform.FailOpen &&
			capabilities.FailurePolicy != platform.FailClosed {
			return nil, &PluginInstallError{
				PluginName: entry.name,
				ReasonCode: ReasonInvalidCapability,
				Reason:     fmt.Sprintf("unknown failure policy %d", capabilities.FailurePolicy),
			}
		}
		if capabilities.Restricts && capabilities.FailurePolicy != platform.FailClosed {
			return nil, &PluginInstallError{
				PluginName: entry.name,
				ReasonCode: ReasonRestrictsMismatch,
				Reason:     "Restricts=true requires FailurePolicy=FailClosed",
			}
		}
		if err := requiredCLIVersionError(entry.name, capabilities.RequiredCLIVersion); err != nil {
			if capabilities.FailurePolicy == platform.FailClosed || mustFailClosed(err) {
				return nil, err
			}
			fmt.Fprintf(errOut, "warning: plugin %q skipped: %v\n", entry.name, err)
			continue
		}
		err = installOne(entry.name, entry.plugin, capabilities, options.pluginConfig, result)
		if err != nil {
			if capabilities.FailurePolicy == platform.FailClosed || mustFailClosed(err) {
				return nil, err
			}
			fmt.Fprintf(errOut, "warning: plugin %q skipped: %v\n", entry.name, err)
		}
	}
	return result, nil
}

func installOne(
	name string,
	plugin platform.Plugin,
	capabilities platform.Capabilities,
	pluginConfig *PluginConfigStore,
	result *InstallResult,
) error {
	staging := newStagingRegistrar(name, pluginConfig.ForPlugin(name))
	if err := safeInstall(name, plugin, staging); err != nil {
		return err
	}
	if err := staging.validate(capabilities); err != nil {
		return err
	}
	staging.commit(result.Registry)
	result.PluginRules = append(result.PluginRules, staging.pluginRules()...)
	result.Plugins = append(result.Plugins, PluginInfo{
		Name:         name,
		Version:      safeVersion(plugin),
		Capabilities: capabilities,
		ConfigKeys:   pluginConfig.Keys(name),
	})
	return nil
}

func mustFailClosed(err error) bool {
	var installErr *PluginInstallError
	if !errors.As(err, &installErr) {
		return false
	}
	return installErr.ReasonCode == ReasonRestrictsMismatch ||
		installErr.ReasonCode == ReasonInvalidCapability ||
		installErr.ReasonCode == ReasonInvalidRule
}

func requiredCLIVersionError(name, constraint string) error {
	satisfied, err := satisfiesRequiredCLIVersion(currentCLIVersion(), constraint)
	if err != nil {
		return &PluginInstallError{
			PluginName: name,
			ReasonCode: ReasonInvalidCapability,
			Reason:     err.Error(),
			Cause:      err,
		}
	}
	if !satisfied {
		return &PluginInstallError{
			PluginName: name,
			ReasonCode: ReasonCapabilityUnmet,
			Reason:     fmt.Sprintf("CLI version %q does not satisfy %q", currentCLIVersion(), constraint),
		}
	}
	return nil
}

type namedPlugin struct {
	name   string
	plugin platform.Plugin
}

func validateAndNamePlugins(plugins []platform.Plugin) ([]namedPlugin, error) {
	named := make([]namedPlugin, 0, len(plugins))
	seen := map[string]bool{}
	for _, plugin := range plugins {
		name, err := safeName(plugin)
		if err != nil {
			return nil, err
		}
		if seen[name] {
			return nil, &PluginInstallError{
				PluginName: name,
				ReasonCode: ReasonDuplicatePluginName,
				Reason:     "plugin names must be unique",
			}
		}
		seen[name] = true
		named = append(named, namedPlugin{name: name, plugin: plugin})
	}
	return named, nil
}

func safeName(plugin platform.Plugin) (name string, err error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			err = &PluginInstallError{
				PluginName: "<unknown>",
				ReasonCode: ReasonInvalidPluginName,
				Reason:     fmt.Sprintf("Name panicked: %v", recovered),
			}
		}
	}()
	if plugin == nil {
		return "", &PluginInstallError{
			PluginName: "<nil>",
			ReasonCode: ReasonInvalidPluginName,
			Reason:     "plugin is nil",
		}
	}
	name = plugin.Name()
	if !namePattern.MatchString(name) {
		return "", &PluginInstallError{
			PluginName: name,
			ReasonCode: ReasonInvalidPluginName,
			Reason:     "name must match ^[a-z0-9][a-z0-9-]*$",
		}
	}
	return name, nil
}

func safeCapabilities(name string, plugin platform.Plugin) (capabilities platform.Capabilities, err error) {
	capabilities.FailurePolicy = platform.FailClosed
	defer func() {
		if recovered := recover(); recovered != nil {
			err = &PluginInstallError{
				PluginName: name,
				ReasonCode: ReasonCapabilitiesPanic,
				Reason:     fmt.Sprintf("Capabilities panicked: %v", recovered),
			}
		}
	}()
	capabilities = plugin.Capabilities()
	return capabilities, nil
}

func safeInstall(name string, plugin platform.Plugin, registrar platform.Registrar) (err error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			err = &PluginInstallError{
				PluginName: name,
				ReasonCode: ReasonInstallPanic,
				Reason:     fmt.Sprintf("Install panicked: %v", recovered),
			}
		}
	}()
	if err := plugin.Install(registrar); err != nil {
		return &PluginInstallError{
			PluginName: name,
			ReasonCode: ReasonInstallFailed,
			Reason:     "Install returned an error",
			Cause:      err,
		}
	}
	return nil
}

func safeVersion(plugin platform.Plugin) (version string) {
	defer func() { _ = recover() }()
	return plugin.Version()
}
