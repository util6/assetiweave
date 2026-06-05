package internalplatform

import (
	"fmt"
	"regexp"

	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdpolicy"
	"github.com/util6/assetiweave/internal/hook"
)

var namePattern = regexp.MustCompile(`^[a-z0-9][a-z0-9-]*$`)

type stagingRegistrar struct {
	pluginName string
	config     platform.PluginConfig
	seenNames  map[string]bool
	observers  []hook.ObserverEntry
	wrappers   []hook.WrapperEntry
	lifecycles []hook.LifecycleEntry
	rules      []*platform.Rule
	restricted bool
	errs       []stagingError
}

type stagingError struct {
	reasonCode string
	message    string
}

func (e stagingError) Error() string {
	return e.message
}

func newStagingRegistrar(pluginName string, config platform.PluginConfig) *stagingRegistrar {
	return &stagingRegistrar{
		pluginName: pluginName,
		config:     config,
		seenNames:  map[string]bool{},
	}
}

func (r *stagingRegistrar) Config() platform.PluginConfig {
	return r.config
}

func (r *stagingRegistrar) Observe(
	when platform.When,
	name string,
	selector platform.Selector,
	observer platform.Observer,
) {
	if !r.validateHook(name, selector, observer != nil) {
		return
	}
	if when != platform.Before && when != platform.After {
		r.addError(ReasonInvalidHook, fmt.Sprintf("observe %q has invalid stage %d", name, when))
		return
	}
	r.observers = append(r.observers, hook.ObserverEntry{
		Name:     r.namespaced(name),
		When:     when,
		Selector: selector,
		Fn:       observer,
	})
}

func (r *stagingRegistrar) Wrap(name string, selector platform.Selector, wrapper platform.Wrapper) {
	if !r.validateHook(name, selector, wrapper != nil) {
		return
	}
	r.wrappers = append(r.wrappers, hook.WrapperEntry{
		Name:     r.namespaced(name),
		Selector: selector,
		Fn:       wrapper,
	})
}

func (r *stagingRegistrar) On(
	event platform.LifecycleEvent,
	name string,
	handler platform.LifecycleHandler,
) {
	if !r.validateHook(name, platform.All(), handler != nil) {
		return
	}
	if event != platform.Startup && event != platform.Shutdown {
		r.addError(ReasonInvalidHook, fmt.Sprintf("lifecycle %q has invalid event %d", name, event))
		return
	}
	r.lifecycles = append(r.lifecycles, hook.LifecycleEntry{
		Name:  r.namespaced(name),
		Event: event,
		Fn:    handler,
	})
}

func (r *stagingRegistrar) Restrict(rule *platform.Rule) {
	r.restricted = true
	if rule == nil {
		r.addError(ReasonInvalidRule, "Restrict rule is nil")
		return
	}
	cloned := *rule
	cloned.Allow = append([]string(nil), rule.Allow...)
	cloned.Deny = append([]string(nil), rule.Deny...)
	cloned.Identities = append([]platform.Identity(nil), rule.Identities...)
	if err := cmdpolicy.ValidateRule(&cloned); err != nil {
		r.addError(ReasonInvalidRule, err.Error())
		return
	}
	r.rules = append(r.rules, &cloned)
}

func (r *stagingRegistrar) validateHook(name string, selector platform.Selector, handlerPresent bool) bool {
	if !namePattern.MatchString(name) {
		r.addError(ReasonInvalidHook, fmt.Sprintf("hook name %q must match ^[a-z0-9][a-z0-9-]*$", name))
		return false
	}
	if r.seenNames[name] {
		r.addError(ReasonInvalidHook, fmt.Sprintf("hook name %q registered more than once", name))
		return false
	}
	r.seenNames[name] = true
	if selector == nil {
		r.addError(ReasonInvalidHook, fmt.Sprintf("hook %q selector is nil", name))
		return false
	}
	if !handlerPresent {
		r.addError(ReasonInvalidHook, fmt.Sprintf("hook %q handler is nil", name))
		return false
	}
	return true
}

func (r *stagingRegistrar) addError(reasonCode, message string) {
	r.errs = append(r.errs, stagingError{reasonCode: reasonCode, message: message})
}

func (r *stagingRegistrar) validate(capabilities platform.Capabilities) error {
	if len(r.errs) == 0 {
		if capabilities.Restricts != r.restricted {
			return &PluginInstallError{
				PluginName: r.pluginName,
				ReasonCode: ReasonRestrictsMismatch,
				Reason:     "Capabilities.Restricts must match calls to Registrar.Restrict",
			}
		}
		return nil
	}
	return &PluginInstallError{
		PluginName: r.pluginName,
		ReasonCode: r.errs[0].reasonCode,
		Reason:     r.errs[0].Error(),
	}
}

func (r *stagingRegistrar) namespaced(name string) string {
	return r.pluginName + "." + name
}

func (r *stagingRegistrar) commit(registry *hook.Registry) {
	for _, observer := range r.observers {
		registry.AddObserver(observer)
	}
	for _, wrapper := range r.wrappers {
		registry.AddWrapper(wrapper)
	}
	for _, lifecycle := range r.lifecycles {
		registry.AddLifecycle(lifecycle)
	}
}

func (r *stagingRegistrar) pluginRules() []cmdpolicy.PluginRule {
	rules := make([]cmdpolicy.PluginRule, 0, len(r.rules))
	for _, rule := range r.rules {
		rules = append(rules, cmdpolicy.PluginRule{PluginName: r.pluginName, Rule: rule})
	}
	return rules
}
