package cmdutil

import (
	"io"
	"os"

	"github.com/util6/assetiweave/internal/client"
)

type IOStreams struct {
	In     io.Reader
	Out    io.Writer
	ErrOut io.Writer
}

type Factory struct {
	IOStreams *IOStreams
	Client    client.EngineCaller
}

func SystemIO() *IOStreams {
	return &IOStreams{In: os.Stdin, Out: os.Stdout, ErrOut: os.Stderr}
}

func NewDefault(streams *IOStreams) *Factory {
	if streams == nil {
		streams = SystemIO()
	}
	return &Factory{
		IOStreams: streams,
		Client:    client.NewEngineClient(""),
	}
}
