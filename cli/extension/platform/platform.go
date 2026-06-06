package platform

import (
	"context"
	"fmt"
	"time"
)

type FailurePolicy int

const (
	FailOpen FailurePolicy = iota
	FailClosed
)

type Capabilities struct {
	RequiredCLIVersion string
	Restricts          bool
	FailurePolicy      FailurePolicy
}

type Plugin interface {
	Name() string
	Version() string
	Capabilities() Capabilities
	Install(Registrar) error
}

type Registrar interface {
	Config() PluginConfig
	Observe(When, string, Selector, Observer)
	Wrap(string, Selector, Wrapper)
	On(LifecycleEvent, string, LifecycleHandler)
	Restrict(*Rule)
}

type When int

const (
	Before When = iota
	After
)

type LifecycleEvent int

const (
	Startup LifecycleEvent = iota
	Shutdown
)

type CommandView interface {
	Path() string
	Domain() string
	Risk() (Risk, bool)
	Identities() []Identity
	Annotation(string) (string, bool)
}

type Invocation interface {
	Cmd() CommandView
	Args() []string
	Started() time.Time
	Err() error
	DeniedByPolicy() bool
	DenialLayer() string
	DenialPolicySource() string
}

type Handler func(context.Context, Invocation) error
type Observer func(context.Context, Invocation)
type Wrapper func(Handler) Handler
type LifecycleHandler func(context.Context, *LifecycleContext) error

type LifecycleContext struct {
	Event LifecycleEvent
	Err   error
}

type AbortError struct {
	HookName string
	Reason   string
	Cause    error
	Details  any
}

func (e *AbortError) Error() string {
	message := fmt.Sprintf("hook %q aborted: %s", e.HookName, e.Reason)
	if e.Cause != nil {
		message += ": " + e.Cause.Error()
	}
	return message
}

func (e *AbortError) Unwrap() error {
	return e.Cause
}
