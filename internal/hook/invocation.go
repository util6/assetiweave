package hook

import (
	"time"

	"github.com/util6/assetiweave/extension/platform"
	"github.com/util6/assetiweave/internal/cmdmeta"
)

type invocation struct {
	command platform.CommandView
	args    []string
	started time.Time
	err     error
	layer   string
	source  string
}

func newInvocation(command platform.CommandView, args []string) *invocation {
	layer, _ := command.Annotation(cmdmeta.AnnotationDenialLayer)
	source, _ := command.Annotation(cmdmeta.AnnotationDenialSource)
	return &invocation{
		command: command,
		args:    append([]string(nil), args...),
		started: time.Now(),
		layer:   layer,
		source:  source,
	}
}

func (i *invocation) Cmd() platform.CommandView { return i.command }
func (i *invocation) Started() time.Time        { return i.started }
func (i *invocation) Err() error                { return i.err }
func (i *invocation) DeniedByPolicy() bool      { return i.layer != "" }
func (i *invocation) DenialLayer() string       { return i.layer }
func (i *invocation) DenialPolicySource() string {
	return i.source
}

func (i *invocation) Args() []string {
	return append([]string(nil), i.args...)
}

func (i *invocation) setErr(err error) {
	i.err = err
}
