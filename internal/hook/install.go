package hook

import (
	"context"
	"errors"
	"fmt"
	"io"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
	"github.com/util6/assetiweave/internal/output"
)

func Install(root *cobra.Command, registry *Registry, errOut io.Writer) {
	if root == nil || registry == nil || !registry.HasHooks() {
		return
	}
	if errOut == nil {
		errOut = io.Discard
	}
	walkTree(root, func(command *cobra.Command) {
		if command.HasParent() && command.Runnable() {
			wrapCommand(command, registry, errOut)
		}
	})
}

func wrapCommand(command *cobra.Command, registry *Registry, errOut io.Writer) {
	originalRunE := command.RunE
	originalRun := command.Run
	command.Run = nil
	command.RunE = func(current *cobra.Command, args []string) error {
		view := cmdmeta.View(current)
		invocation := newInvocation(view, args)
		ctx := current.Context()
		if ctx == nil {
			ctx = context.Background()
		}

		for _, observer := range registry.MatchingObservers(view, platform.Before) {
			runObserver(ctx, observer, invocation, errOut)
		}

		var err error
		if invocation.DeniedByPolicy() {
			err = invokeOriginal(ctx, current, args, originalRunE, originalRun)
		} else {
			matched := registry.MatchingWrappers(view)
			wrappers := make([]platform.Wrapper, 0, len(matched))
			for _, entry := range matched {
				wrappers = append(wrappers, protectWrapper(entry.Name, entry.Fn))
			}
			composed := identityHandler
			if len(wrappers) > 0 {
				composed = ComposeWrappers(wrappers)
			}
			handler := composed(func(nextContext context.Context, _ platform.Invocation) error {
				return invokeOriginal(nextContext, current, args, originalRunE, originalRun)
			})
			err = handler(ctx, invocation)
		}
		err = hookError(err)
		invocation.setErr(err)

		for _, observer := range registry.MatchingObservers(view, platform.After) {
			runObserver(ctx, observer, invocation, errOut)
		}
		return err
	}
}

func invokeOriginal(
	ctx context.Context,
	command *cobra.Command,
	args []string,
	runE func(*cobra.Command, []string) error,
	run func(*cobra.Command, []string),
) error {
	previous := command.Context()
	command.SetContext(ctx)
	defer command.SetContext(previous)
	if runE != nil {
		return runE(command, args)
	}
	if run != nil {
		run(command, args)
	}
	return nil
}

func runObserver(ctx context.Context, entry ObserverEntry, invocation platform.Invocation, errOut io.Writer) {
	defer func() {
		if recovered := recover(); recovered != nil {
			fmt.Fprintf(errOut, "warning: hook %q panicked: %v\n", entry.Name, recovered)
		}
	}()
	entry.Fn(ctx, invocation)
}

func protectWrapper(name string, wrapper platform.Wrapper) platform.Wrapper {
	return func(next platform.Handler) platform.Handler {
		return func(ctx context.Context, invocation platform.Invocation) (err error) {
			defer func() {
				if recovered := recover(); recovered != nil {
					err = output.Errorf(output.ExitInternal, "hook_panic", "hook %q panicked: %v", name, recovered)
				}
			}()
			inner := wrapper(next)
			err = inner(ctx, invocation)
			if abort, ok := err.(*platform.AbortError); ok {
				copied := *abort
				copied.HookName = name
				err = &copied
			}
			return err
		}
	}
}

func hookError(err error) error {
	if err == nil {
		return nil
	}
	var abort *platform.AbortError
	if !errors.As(err, &abort) {
		return err
	}
	return &output.ExitError{
		Code: output.ExitValidation,
		Detail: &output.ErrDetail{
			Type:    "hook",
			Code:    "hook_aborted",
			Message: abort.Error(),
			Details: map[string]any{
				"hook_name": abort.HookName,
				"reason":    abort.Reason,
				"details":   abort.Details,
			},
		},
		Err: err,
	}
}

func walkTree(root *cobra.Command, visit func(*cobra.Command)) {
	visit(root)
	for _, child := range root.Commands() {
		walkTree(child, visit)
	}
}
