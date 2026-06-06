package hook

import (
	"context"
	"sync"

	"github.com/util6/assetiweave/extension/platform"
)

type ObserverEntry struct {
	Name     string
	When     platform.When
	Selector platform.Selector
	Fn       platform.Observer
}

type WrapperEntry struct {
	Name     string
	Selector platform.Selector
	Fn       platform.Wrapper
}

type LifecycleEntry struct {
	Name  string
	Event platform.LifecycleEvent
	Fn    platform.LifecycleHandler
}

type Registry struct {
	mu         sync.RWMutex
	observers  []ObserverEntry
	wrappers   []WrapperEntry
	lifecycles []LifecycleEntry
}

func NewRegistry() *Registry {
	return &Registry{}
}

func (r *Registry) HasHooks() bool {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return len(r.observers) > 0 || len(r.wrappers) > 0 || len(r.lifecycles) > 0
}

func (r *Registry) AddObserver(entry ObserverEntry) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.observers = append(r.observers, entry)
}

func (r *Registry) AddWrapper(entry WrapperEntry) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.wrappers = append(r.wrappers, entry)
}

func (r *Registry) AddLifecycle(entry LifecycleEntry) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.lifecycles = append(r.lifecycles, entry)
}

func (r *Registry) MatchingObservers(command platform.CommandView, when platform.When) []ObserverEntry {
	r.mu.RLock()
	defer r.mu.RUnlock()
	matches := make([]ObserverEntry, 0, len(r.observers))
	for _, entry := range r.observers {
		if entry.When == when && entry.Selector != nil && entry.Selector(command) {
			matches = append(matches, entry)
		}
	}
	return matches
}

func (r *Registry) MatchingWrappers(command platform.CommandView) []WrapperEntry {
	r.mu.RLock()
	defer r.mu.RUnlock()
	matches := make([]WrapperEntry, 0, len(r.wrappers))
	for _, entry := range r.wrappers {
		if entry.Selector != nil && entry.Selector(command) {
			matches = append(matches, entry)
		}
	}
	return matches
}

func (r *Registry) LifecycleHandlers(event platform.LifecycleEvent) []LifecycleEntry {
	r.mu.RLock()
	defer r.mu.RUnlock()
	matches := make([]LifecycleEntry, 0, len(r.lifecycles))
	for _, entry := range r.lifecycles {
		if entry.Event == event {
			matches = append(matches, entry)
		}
	}
	return matches
}

func (r *Registry) Observers() []ObserverEntry {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return append([]ObserverEntry(nil), r.observers...)
}

func (r *Registry) Wrappers() []WrapperEntry {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return append([]WrapperEntry(nil), r.wrappers...)
}

func (r *Registry) Lifecycles() []LifecycleEntry {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return append([]LifecycleEntry(nil), r.lifecycles...)
}

func ComposeWrappers(wrappers []platform.Wrapper) platform.Wrapper {
	return func(next platform.Handler) platform.Handler {
		for index := len(wrappers) - 1; index >= 0; index-- {
			next = wrappers[index](next)
		}
		return next
	}
}

func identityHandler(next platform.Handler) platform.Handler {
	return func(ctx context.Context, invocation platform.Invocation) error {
		return next(ctx, invocation)
	}
}
