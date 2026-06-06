package platform

import (
	"errors"
	"fmt"
	"regexp"
)

var pluginNamePattern = regexp.MustCompile(`^[a-z0-9][a-z0-9-]*$`)

type Builder struct {
	name      string
	version   string
	caps      Capabilities
	actions   []func(Registrar)
	hookNames map[string]bool
	rules     []*Rule
	errs      []error
}

func NewPlugin(name, version string) *Builder {
	builder := &Builder{
		name:      name,
		version:   version,
		hookNames: map[string]bool{},
	}
	if !pluginNamePattern.MatchString(name) {
		builder.errs = append(builder.errs, fmt.Errorf("invalid plugin name %q", name))
	}
	return builder
}

func (b *Builder) FailOpen() *Builder {
	b.caps.FailurePolicy = FailOpen
	return b
}

func (b *Builder) FailClosed() *Builder {
	b.caps.FailurePolicy = FailClosed
	return b
}

func (b *Builder) RequireCLI(constraint string) *Builder {
	b.caps.RequiredCLIVersion = constraint
	return b
}

func (b *Builder) Observer(when When, hookName string, selector Selector, observer Observer) *Builder {
	if !b.validateHookName(hookName) {
		return b
	}
	b.actions = append(b.actions, func(registrar Registrar) {
		registrar.Observe(when, hookName, selector, observer)
	})
	return b
}

func (b *Builder) Wrap(hookName string, selector Selector, wrapper Wrapper) *Builder {
	if !b.validateHookName(hookName) {
		return b
	}
	b.actions = append(b.actions, func(registrar Registrar) {
		registrar.Wrap(hookName, selector, wrapper)
	})
	return b
}

func (b *Builder) On(event LifecycleEvent, hookName string, handler LifecycleHandler) *Builder {
	if !b.validateHookName(hookName) {
		return b
	}
	b.actions = append(b.actions, func(registrar Registrar) {
		registrar.On(event, hookName, handler)
	})
	return b
}

func (b *Builder) Restrict(rule *Rule) *Builder {
	if rule == nil {
		b.errs = append(b.errs, errors.New("Restrict rule must not be nil"))
		return b
	}
	b.caps.Restricts = true
	b.caps.FailurePolicy = FailClosed
	cloned := *rule
	cloned.Allow = append([]string(nil), rule.Allow...)
	cloned.Deny = append([]string(nil), rule.Deny...)
	cloned.Identities = append([]Identity(nil), rule.Identities...)
	b.rules = append(b.rules, &cloned)
	return b
}

func (b *Builder) Build() (Plugin, error) {
	if len(b.rules) > 0 && b.caps.FailurePolicy != FailClosed {
		b.errs = append(b.errs, errors.New("Restrict requires FailClosed"))
	}
	if len(b.errs) > 0 {
		return nil, errors.Join(b.errs...)
	}
	return &builtPlugin{
		name:    b.name,
		version: b.version,
		caps:    b.caps,
		actions: append([]func(Registrar){}, b.actions...),
		rules:   append([]*Rule(nil), b.rules...),
	}, nil
}

func (b *Builder) MustBuild() Plugin {
	plugin, err := b.Build()
	if err != nil {
		panic(err)
	}
	return plugin
}

func (b *Builder) validateHookName(hookName string) bool {
	if !pluginNamePattern.MatchString(hookName) {
		b.errs = append(b.errs, fmt.Errorf("invalid hook name %q", hookName))
		return false
	}
	if b.hookNames[hookName] {
		b.errs = append(b.errs, fmt.Errorf("duplicate hook name %q", hookName))
		return false
	}
	b.hookNames[hookName] = true
	return true
}

type builtPlugin struct {
	name    string
	version string
	caps    Capabilities
	actions []func(Registrar)
	rules   []*Rule
}

func (p *builtPlugin) Name() string {
	return p.name
}

func (p *builtPlugin) Version() string {
	return p.version
}

func (p *builtPlugin) Capabilities() Capabilities {
	return p.caps
}

func (p *builtPlugin) Install(registrar Registrar) error {
	for _, rule := range p.rules {
		registrar.Restrict(rule)
	}
	for _, action := range p.actions {
		action(registrar)
	}
	return nil
}
