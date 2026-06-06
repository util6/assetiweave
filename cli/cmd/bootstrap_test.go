package cmd

import (
	"testing"

	"github.com/util6/assetiweave/errs"
)

func TestParseBootstrapOptionsReadsGlobalFlagsBeforeCobraExecution(t *testing.T) {
	options, err := parseBootstrapOptions([]string{
		"--plugin-config", "/tmp/plugins.json",
		"overview",
		"--policy=/tmp/policy.json",
		"--engine", "/tmp/engine",
	})
	if err != nil {
		t.Fatalf("parseBootstrapOptions() error = %v", err)
	}
	if options.PluginConfigPath != "/tmp/plugins.json" ||
		options.PolicyPath != "/tmp/policy.json" ||
		options.EnginePath != "/tmp/engine" {
		t.Fatalf("options = %+v", options)
	}
}

func TestParseBootstrapOptionsRejectsMissingValue(t *testing.T) {
	_, err := parseBootstrapOptions([]string{"--plugin-config"})

	assertTypedProblem(t, err, errs.CategoryValidation, errs.SubtypeInvalidArgument)
}

func TestParseBootstrapOptionsStopsAtArgumentSeparator(t *testing.T) {
	options, err := parseBootstrapOptions([]string{"overview", "--", "--engine", "/tmp/ignored"})
	if err != nil {
		t.Fatalf("parseBootstrapOptions() error = %v", err)
	}
	if options.EnginePath != "" {
		t.Fatalf("EnginePath = %q, want empty after -- separator", options.EnginePath)
	}
}
