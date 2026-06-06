package hook

import (
	"context"
	"fmt"
	"io"
	"time"

	"github.com/util6/assetiweave/extension/platform"
)

const shutdownDeadline = 2 * time.Second

type LifecycleError struct {
	Event    platform.LifecycleEvent
	HookName string
	Panic    bool
	Cause    error
}

func (e *LifecycleError) Error() string {
	return fmt.Sprintf("lifecycle hook %q failed: %v", e.HookName, e.Cause)
}

func (e *LifecycleError) Unwrap() error {
	return e.Cause
}

func Emit(
	ctx context.Context,
	registry *Registry,
	event platform.LifecycleEvent,
	lastErr error,
	errOut io.Writer,
) error {
	if registry == nil {
		return nil
	}
	if errOut == nil {
		errOut = io.Discard
	}
	lifecycle := &platform.LifecycleContext{Event: event, Err: lastErr}
	if event == platform.Shutdown {
		ctx, cancel := context.WithTimeout(ctx, shutdownDeadline)
		defer cancel()
		for _, entry := range registry.LifecycleHandlers(event) {
			if err := callLifecycle(ctx, entry, lifecycle); err != nil {
				fmt.Fprintf(errOut, "warning: shutdown hook %q: %v\n", entry.Name, err)
			}
		}
		return nil
	}
	for _, entry := range registry.LifecycleHandlers(event) {
		if err := callLifecycle(ctx, entry, lifecycle); err != nil {
			return err
		}
	}
	return nil
}

func callLifecycle(
	ctx context.Context,
	entry LifecycleEntry,
	lifecycle *platform.LifecycleContext,
) (err error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			err = &LifecycleError{
				Event:    lifecycle.Event,
				HookName: entry.Name,
				Panic:    true,
				Cause:    fmt.Errorf("%v", recovered),
			}
		}
	}()
	if cause := entry.Fn(ctx, lifecycle); cause != nil {
		return &LifecycleError{
			Event:    lifecycle.Event,
			HookName: entry.Name,
			Cause:    cause,
		}
	}
	return nil
}
