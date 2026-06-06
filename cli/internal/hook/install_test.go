package hook

import (
	"bytes"
	"context"
	"errors"
	"testing"

	"github.com/spf13/cobra"
	"github.com/util6/assetiweave/errs"
	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
)

func TestAfterObserverSeesCommandError(t *testing.T) {
	want := errors.New("command failed")
	var observed error
	registry := NewRegistry()
	registry.AddObserver(ObserverEntry{
		Name:     "audit.after",
		When:     platform.After,
		Selector: platform.All(),
		Fn: func(_ context.Context, invocation platform.Invocation) {
			observed = invocation.Err()
		},
	})
	root := testRoot(func(*cobra.Command, []string) error { return want })
	Install(root, registry, &bytes.Buffer{})
	root.SetArgs([]string{"run"})

	err := root.Execute()

	if !errors.Is(err, want) || !errors.Is(observed, want) {
		t.Fatalf("command error = %v, observed error = %v, want %v", err, observed, want)
	}
}

func TestWrapperAbortBecomesStructuredError(t *testing.T) {
	registry := NewRegistry()
	registry.AddWrapper(WrapperEntry{
		Name:     "policy.block",
		Selector: platform.All(),
		Fn: func(platform.Handler) platform.Handler {
			return func(context.Context, platform.Invocation) error {
				return &platform.AbortError{Reason: "denied"}
			}
		},
	})
	root := testRoot(func(*cobra.Command, []string) error {
		t.Fatal("command handler ran after wrapper abort")
		return nil
	})
	Install(root, registry, &bytes.Buffer{})
	root.SetArgs([]string{"run"})

	err := root.Execute()

	problem, ok := errs.ProblemOf(err)
	if !ok ||
		problem.Category != errs.CategoryValidation ||
		problem.Subtype != errs.SubtypeHookAborted {
		t.Fatalf("error = %#v, want typed validation.hook_aborted", err)
	}
	details, ok := problem.Details.(map[string]any)
	if !ok || details["hook_name"] != "policy.block" {
		t.Fatalf("details = %#v, want namespaced hook", problem.Details)
	}
	var abort *platform.AbortError
	if !errors.As(err, &abort) || abort.HookName != "policy.block" {
		t.Fatalf("error chain = %#v, want namespaced AbortError", err)
	}
}

func TestWrapperPanicBecomesTypedInternalError(t *testing.T) {
	registry := NewRegistry()
	registry.AddWrapper(WrapperEntry{
		Name:     "broken.wrapper",
		Selector: platform.All(),
		Fn: func(platform.Handler) platform.Handler {
			return func(context.Context, platform.Invocation) error {
				panic("broken")
			}
		},
	})
	root := testRoot(func(*cobra.Command, []string) error {
		t.Fatal("command handler ran after wrapper panic")
		return nil
	})
	Install(root, registry, &bytes.Buffer{})
	root.SetArgs([]string{"run"})

	err := root.Execute()

	problem, ok := errs.ProblemOf(err)
	if !ok ||
		problem.Category != errs.CategoryInternal ||
		problem.Subtype != errs.SubtypeHookPanic ||
		problem.Code != "hook_panic" {
		t.Fatalf("error = %#v, want typed internal.hook_panic", err)
	}
}

func TestWrapperContextReachesCommand(t *testing.T) {
	type contextKey string
	const key contextKey = "trace"
	registry := NewRegistry()
	registry.AddWrapper(WrapperEntry{
		Name:     "trace.inject",
		Selector: platform.All(),
		Fn: func(next platform.Handler) platform.Handler {
			return func(ctx context.Context, invocation platform.Invocation) error {
				return next(context.WithValue(ctx, key, "trace-id"), invocation)
			}
		},
	})
	var got any
	root := testRoot(func(command *cobra.Command, _ []string) error {
		got = command.Context().Value(key)
		return nil
	})
	Install(root, registry, &bytes.Buffer{})
	root.SetArgs([]string{"run"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if got != "trace-id" {
		t.Fatalf("context value = %#v, want trace-id", got)
	}
}

func TestInstallSkipsMarkedPureGroups(t *testing.T) {
	observed := false
	registry := NewRegistry()
	registry.AddObserver(ObserverEntry{
		Name:     "audit.before",
		When:     platform.Before,
		Selector: platform.All(),
		Fn: func(context.Context, platform.Invocation) {
			observed = true
		},
	})
	root := &cobra.Command{Use: "test", SilenceUsage: true, SilenceErrors: true}
	group := &cobra.Command{
		Use:  "group",
		RunE: func(*cobra.Command, []string) error { return nil },
	}
	cmdmeta.MarkPureGroup(group)
	group.AddCommand(&cobra.Command{Use: "run", RunE: func(*cobra.Command, []string) error { return nil }})
	root.AddCommand(group)

	Install(root, registry, &bytes.Buffer{})
	root.SetArgs([]string{"group"})

	if err := root.Execute(); err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	if observed {
		t.Fatal("pure group help handler was wrapped as a plugin invocation")
	}
}

func testRoot(run func(*cobra.Command, []string) error) *cobra.Command {
	root := &cobra.Command{Use: "test", SilenceUsage: true, SilenceErrors: true}
	root.AddCommand(&cobra.Command{Use: "run", RunE: run})
	return root
}
